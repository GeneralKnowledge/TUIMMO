//! SQLite persistence. The running simulation keeps operators and the recent log
//! in memory; this store writes the rows that changed and reloads them on boot.

use std::collections::BTreeSet;
use std::path::Path;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use terminus_core::{Audience, Channel, LogEntry, PlayerId};
use thiserror::Error;
use uuid::Uuid;

use crate::model::{Item, Operator, PersistOp, LOG_CAP};
use crate::undernet::{ExitDef, LocationDef};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("migration failed")]
    Migrate(String),
    #[error("stored data is invalid")]
    Data,
}

pub struct Store {
    pool: SqlitePool,
}

pub struct LoadedWorld {
    pub operators: Vec<Operator>,
    pub log: Vec<LogEntry>,
}

impl Store {
    pub async fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| StoreError::Migrate(format!("create data dir: {err}")))?;
        }
        let url = format!("sqlite://{}", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .map_err(|err| StoreError::Migrate(err.to_string()))?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new().max_connections(8).connect_with(options).await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|err| StoreError::Migrate(err.to_string()))?;
        Ok(Self { pool })
    }

    pub async fn seed(
        &self,
        locations: &[LocationDef],
        exits: &[ExitDef],
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        for loc in locations {
            sqlx::query(
                "INSERT INTO locations (id, name, description) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description",
            )
            .bind(loc.id)
            .bind(loc.name)
            .bind(loc.description)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM exits").execute(&mut *tx).await?;
        for exit in exits {
            sqlx::query(
                "INSERT INTO exits (from_id, keyword, to_id, label, requires_flag)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(exit.from)
            .bind(exit.keyword)
            .bind(exit.to)
            .bind(exit.label)
            .bind(exit.requires_flag)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn load(&self) -> Result<LoadedWorld, StoreError> {
        let operators = self.load_operators().await?;
        let log = self.load_log().await?;
        Ok(LoadedWorld { operators, log })
    }

    pub async fn create_account(
        &self,
        account_id: Uuid,
        username: &str,
        password_hash: &str,
        operator: &Operator,
        created_at: u64,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO accounts (id, username, password_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(account_id.to_string())
        .bind(username)
        .bind(password_hash)
        .bind(created_at as i64)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO operators (id, account_id, name, location_id, created_at, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(operator.id.to_string())
        .bind(operator.account_id.to_string())
        .bind(&operator.name)
        .bind(&operator.location_id)
        .bind(created_at as i64)
        .bind(created_at as i64)
        .execute(&mut *tx)
        .await?;
        for item in &operator.items {
            sqlx::query(
                "INSERT INTO operator_items (operator_id, item_key, name, detail) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(operator.id.to_string())
            .bind(&item.keyword)
            .bind(&item.name)
            .bind(&item.detail)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn find_account(&self, username: &str) -> Result<Option<AccountRow>, StoreError> {
        let row = sqlx::query_as::<_, AccountSql>(
            "SELECT a.id AS id, a.username, a.password_hash, o.id AS operator_id, o.name, o.location_id
             FROM accounts a
             JOIN operators o ON o.account_id = a.id
             WHERE a.username = ?1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        row.map(AccountSql::into_row).transpose()
    }

    pub async fn commit(&self, ops: &[PersistOp]) -> Result<(), StoreError> {
        if ops.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for op in ops {
            match op {
                PersistOp::Move { operator_id, location_id, last_seen } => {
                    sqlx::query(
                        "UPDATE operators SET location_id = ?1, last_seen = ?2 WHERE id = ?3",
                    )
                    .bind(location_id)
                    .bind(*last_seen as i64)
                    .bind(operator_id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::Seen { operator_id, last_seen } => {
                    sqlx::query("UPDATE operators SET last_seen = ?1 WHERE id = ?2")
                        .bind(*last_seen as i64)
                        .bind(operator_id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::Message { at_ms, channel, audience, audience_key, speaker, body } => {
                    sqlx::query(
                        "INSERT INTO messages (at_ms, channel, audience, audience_key, speaker, body)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )
                    .bind(*at_ms as i64)
                    .bind(channel)
                    .bind(audience)
                    .bind(audience_key)
                    .bind(speaker)
                    .bind(body)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::Flag { operator_id, flag } => {
                    sqlx::query(
                        "INSERT OR IGNORE INTO operator_flags (operator_id, flag) VALUES (?1, ?2)",
                    )
                    .bind(operator_id.to_string())
                    .bind(flag)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::Item { operator_id, keyword, name, detail } => {
                    sqlx::query(
                        "INSERT OR IGNORE INTO operator_items (operator_id, item_key, name, detail)
                         VALUES (?1, ?2, ?3, ?4)",
                    )
                    .bind(operator_id.to_string())
                    .bind(keyword)
                    .bind(name)
                    .bind(detail)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
        sqlx::query(
            "DELETE FROM messages WHERE id NOT IN (
                SELECT id FROM messages ORDER BY id DESC LIMIT ?1
             )",
        )
        .bind(LOG_CAP as i64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn load_operators(&self) -> Result<Vec<Operator>, StoreError> {
        let rows = sqlx::query_as::<_, OpSql>(
            "SELECT o.id, o.account_id, a.username, o.name, o.location_id
             FROM operators o
             JOIN accounts a ON a.id = o.account_id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut operators = Vec::with_capacity(rows.len());
        for row in rows {
            let id = PlayerId::from_str(&row.id).map_err(|_| StoreError::Data)?;
            let account_id = Uuid::from_str(&row.account_id).map_err(|_| StoreError::Data)?;
            let flags = sqlx::query_scalar::<_, String>(
                "SELECT flag FROM operator_flags WHERE operator_id = ?1",
            )
            .bind(&row.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .collect::<BTreeSet<_>>();
            let items = sqlx::query_as::<_, ItemSql>(
                "SELECT item_key, name, detail FROM operator_items WHERE operator_id = ?1",
            )
            .bind(&row.id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|item| Item { keyword: item.item_key, name: item.name, detail: item.detail })
            .collect();
            operators.push(Operator {
                id,
                account_id,
                username: row.username,
                name: row.name,
                location_id: row.location_id,
                flags,
                items,
            });
        }
        Ok(operators)
    }

    async fn load_log(&self) -> Result<Vec<LogEntry>, StoreError> {
        let rows = sqlx::query_as::<_, MsgSql>(
            "SELECT at_ms, channel, audience, audience_key, speaker, body
             FROM messages ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut log = Vec::with_capacity(rows.len());
        for row in rows {
            let channel = match row.channel.as_str() {
                "system" => Channel::System,
                "local" => Channel::Local,
                "global" => Channel::Global,
                _ => continue,
            };
            let audience = match row.audience.as_str() {
                "everyone" => Audience::Everyone,
                "location" => Audience::Location(row.audience_key),
                "player" => {
                    let id = PlayerId::from_str(&row.audience_key).map_err(|_| StoreError::Data)?;
                    Audience::Player(id)
                }
                _ => continue,
            };
            log.push(LogEntry {
                at_ms: row.at_ms.max(0) as u64,
                channel,
                audience,
                speaker: row.speaker,
                text: row.body,
            });
        }
        Ok(log)
    }
}

pub struct AccountRow {
    pub account_id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub operator_id: PlayerId,
    pub operator_name: String,
    pub location_id: String,
}

#[derive(sqlx::FromRow)]
struct AccountSql {
    id: String,
    username: String,
    password_hash: String,
    operator_id: String,
    name: String,
    location_id: String,
}

impl AccountSql {
    fn into_row(self) -> Result<AccountRow, StoreError> {
        Ok(AccountRow {
            account_id: Uuid::from_str(&self.id).map_err(|_| StoreError::Data)?,
            username: self.username,
            password_hash: self.password_hash,
            operator_id: PlayerId::from_str(&self.operator_id).map_err(|_| StoreError::Data)?,
            operator_name: self.name,
            location_id: self.location_id,
        })
    }
}

#[derive(sqlx::FromRow)]
struct OpSql {
    id: String,
    account_id: String,
    username: String,
    name: String,
    location_id: String,
}

#[derive(sqlx::FromRow)]
struct ItemSql {
    item_key: String,
    name: String,
    detail: String,
}

#[derive(sqlx::FromRow)]
struct MsgSql {
    at_ms: i64,
    channel: String,
    audience: String,
    audience_key: String,
    speaker: Option<String>,
    body: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::STARTER_LOCATION;
    use crate::undernet;

    #[tokio::test]
    async fn account_and_move_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(undernet::locations(), &undernet::all_exits()).await.unwrap();
        let op_id = PlayerId::new();
        let account_id = Uuid::new_v4();
        let operator = Operator {
            id: op_id,
            account_id,
            username: "ada".into(),
            name: "ada".into(),
            location_id: STARTER_LOCATION.into(),
            flags: BTreeSet::new(),
            items: vec![Item::chip()],
        };
        store.create_account(account_id, "ada", "hash", &operator, 10).await.unwrap();
        store
            .commit(&[PersistOp::Move {
                operator_id: op_id,
                location_id: "neon-alley".into(),
                last_seen: 11,
            }])
            .await
            .unwrap();
        drop(store);
        let store = Store::open(&path).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.operators.len(), 1);
        assert_eq!(loaded.operators[0].location_id, "neon-alley");
        assert_eq!(loaded.operators[0].items[0].keyword, "chip");
        let locs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM locations")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert!(locs >= 10);
    }
}
