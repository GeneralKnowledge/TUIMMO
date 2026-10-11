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

use crate::geography::{
    parse_seed, Discovery, GeographyState, MaterializedRegion, RegionIdentity, Structure, Terrain,
    TerrainMod, WorldConnection, WorldId,
};
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
    pub geography: GeographyState,
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
        let geography = self.load_geography().await?;
        Ok(LoadedWorld { operators, log, geography })
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
                PersistOp::MaterializeRegion {
                    id,
                    world_id,
                    epoch_id,
                    origin_x,
                    origin_y,
                    generation_version,
                    region_seed_hex,
                    placement_x,
                    placement_y,
                    discovered_at,
                } => {
                    sqlx::query(
                        "INSERT INTO world_regions (
                            id, world_id, epoch_id, origin_x, origin_y, generation_version,
                            region_seed, placement_x, placement_y, discovered_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                         ON CONFLICT (world_id, epoch_id, origin_x, origin_y) DO NOTHING",
                    )
                    .bind(id.to_string())
                    .bind(world_id)
                    .bind(i64::from(*epoch_id))
                    .bind(i64::from(*origin_x))
                    .bind(i64::from(*origin_y))
                    .bind(i64::from(*generation_version))
                    .bind(region_seed_hex)
                    .bind(i64::from(*placement_x))
                    .bind(i64::from(*placement_y))
                    .bind(*discovered_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::RelocateRegion { id, placement_x, placement_y } => {
                    sqlx::query(
                        "UPDATE world_regions SET placement_x = ?1, placement_y = ?2 WHERE id = ?3",
                    )
                    .bind(i64::from(*placement_x))
                    .bind(i64::from(*placement_y))
                    .bind(id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::Structure {
                    id,
                    region_id,
                    local_x,
                    local_y,
                    kind,
                    name,
                    owner_id,
                    created_at,
                } => {
                    sqlx::query(
                        "INSERT INTO world_structures (
                            id, region_id, local_x, local_y, kind, name, owner_id, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    )
                    .bind(id.to_string())
                    .bind(region_id.to_string())
                    .bind(i64::from(*local_x))
                    .bind(i64::from(*local_y))
                    .bind(kind)
                    .bind(name)
                    .bind(owner_id.map(|id| id.to_string()))
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::TerrainMod {
                    region_id,
                    local_x,
                    local_y,
                    terrain,
                    reason,
                    actor_id,
                    created_at,
                } => {
                    sqlx::query(
                        "INSERT INTO terrain_modifications (
                            region_id, local_x, local_y, terrain, reason, actor_id, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                         ON CONFLICT (region_id, local_x, local_y) DO UPDATE SET
                            terrain = excluded.terrain,
                            reason = excluded.reason,
                            actor_id = excluded.actor_id,
                            created_at = excluded.created_at",
                    )
                    .bind(region_id.to_string())
                    .bind(i64::from(*local_x))
                    .bind(i64::from(*local_y))
                    .bind(terrain)
                    .bind(reason)
                    .bind(actor_id.map(|id| id.to_string()))
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::Discovery {
                    id,
                    region_id,
                    local_x,
                    local_y,
                    kind,
                    note,
                    actor_id,
                    created_at,
                } => {
                    sqlx::query(
                        "INSERT INTO region_discoveries (
                            id, region_id, local_x, local_y, kind, note, actor_id, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    )
                    .bind(id.to_string())
                    .bind(region_id.to_string())
                    .bind(i64::from(*local_x))
                    .bind(i64::from(*local_y))
                    .bind(kind)
                    .bind(note)
                    .bind(actor_id.map(|id| id.to_string()))
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::Connection {
                    id,
                    from_place,
                    keyword,
                    to_place,
                    label,
                    kind,
                    created_by,
                    created_at,
                } => {
                    sqlx::query(
                        "INSERT INTO world_connections (
                            id, from_place, keyword, to_place, label, kind, created_by, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    )
                    .bind(id.to_string())
                    .bind(from_place)
                    .bind(keyword)
                    .bind(to_place)
                    .bind(label)
                    .bind(kind)
                    .bind(created_by.map(|id| id.to_string()))
                    .bind(*created_at as i64)
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

    async fn load_geography(&self) -> Result<GeographyState, StoreError> {
        let mut geography = GeographyState::default();
        let regions = sqlx::query_as::<_, RegionSql>(
            "SELECT id, world_id, epoch_id, origin_x, origin_y, generation_version,
                    region_seed, placement_x, placement_y, discovered_at
             FROM world_regions",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in regions {
            let identity = row.identity()?;
            let id = Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?;
            let region_seed = parse_seed(&row.region_seed).ok_or(StoreError::Data)?;
            geography.regions.insert(
                identity.coord_key(),
                MaterializedRegion {
                    id,
                    identity,
                    region_seed,
                    placement_x: i32_col(row.placement_x)?,
                    placement_y: i32_col(row.placement_y)?,
                    discovered_at: u64_col(row.discovered_at)?,
                },
            );
        }

        let structures = sqlx::query_as::<_, StructureSql>(
            "SELECT id, region_id, local_x, local_y, kind, name, owner_id, created_at
             FROM world_structures",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in structures {
            geography.structures.push(Structure {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                region_id: Uuid::from_str(&row.region_id).map_err(|_| StoreError::Data)?,
                local_x: local_col(row.local_x)?,
                local_y: local_col(row.local_y)?,
                kind: row.kind,
                name: row.name,
                owner_id: optional_player(row.owner_id)?,
                created_at: u64_col(row.created_at)?,
            });
        }

        let mods = sqlx::query_as::<_, ModSql>(
            "SELECT region_id, local_x, local_y, terrain, reason, actor_id, created_at
             FROM terrain_modifications",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in mods {
            geography.terrain_mods.push(TerrainMod {
                region_id: Uuid::from_str(&row.region_id).map_err(|_| StoreError::Data)?,
                local_x: local_col(row.local_x)?,
                local_y: local_col(row.local_y)?,
                terrain: Terrain::parse(&row.terrain).ok_or(StoreError::Data)?,
                reason: row.reason,
                actor_id: optional_player(row.actor_id)?,
                created_at: u64_col(row.created_at)?,
            });
        }

        let discoveries = sqlx::query_as::<_, DiscoverySql>(
            "SELECT id, region_id, local_x, local_y, kind, note, actor_id, created_at
             FROM region_discoveries",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in discoveries {
            geography.discoveries.push(Discovery {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                region_id: Uuid::from_str(&row.region_id).map_err(|_| StoreError::Data)?,
                local_x: local_col(row.local_x)?,
                local_y: local_col(row.local_y)?,
                kind: row.kind,
                note: row.note,
                actor_id: optional_player(row.actor_id)?,
                created_at: u64_col(row.created_at)?,
            });
        }

        let links = sqlx::query_as::<_, LinkSql>(
            "SELECT id, from_place, keyword, to_place, label, kind, created_by, created_at
             FROM world_connections",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in links {
            geography.connections.push(WorldConnection {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                from_place: row.from_place,
                keyword: row.keyword,
                to_place: row.to_place,
                label: row.label,
                kind: row.kind,
                created_by: optional_player(row.created_by)?,
                created_at: u64_col(row.created_at)?,
            });
        }
        Ok(geography)
    }
}

fn i32_col(value: i64) -> Result<i32, StoreError> {
    i32::try_from(value).map_err(|_| StoreError::Data)
}

fn u64_col(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Data)
}

fn local_col(value: i64) -> Result<u8, StoreError> {
    let local = u8::try_from(value).map_err(|_| StoreError::Data)?;
    if u32::from(local) >= u32::try_from(crate::geography::REGION_SIZE).unwrap_or(16) {
        return Err(StoreError::Data);
    }
    Ok(local)
}

fn optional_player(value: Option<String>) -> Result<Option<PlayerId>, StoreError> {
    value.map(|raw| PlayerId::from_str(&raw).map_err(|_| StoreError::Data)).transpose()
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

#[derive(sqlx::FromRow)]
struct RegionSql {
    id: String,
    world_id: String,
    epoch_id: i64,
    origin_x: i64,
    origin_y: i64,
    generation_version: i64,
    region_seed: String,
    placement_x: i64,
    placement_y: i64,
    discovered_at: i64,
}

impl RegionSql {
    fn identity(&self) -> Result<RegionIdentity, StoreError> {
        let world = WorldId::parse(&self.world_id).ok_or(StoreError::Data)?;
        let epoch_id = u32::try_from(self.epoch_id).map_err(|_| StoreError::Data)?;
        let generation_version =
            u16::try_from(self.generation_version).map_err(|_| StoreError::Data)?;
        Ok(RegionIdentity {
            world,
            epoch_id,
            origin_x: i32_col(self.origin_x)?,
            origin_y: i32_col(self.origin_y)?,
            generation_version,
        })
    }
}

#[derive(sqlx::FromRow)]
struct StructureSql {
    id: String,
    region_id: String,
    local_x: i64,
    local_y: i64,
    kind: String,
    name: String,
    owner_id: Option<String>,
    created_at: i64,
}

#[derive(sqlx::FromRow)]
struct ModSql {
    region_id: String,
    local_x: i64,
    local_y: i64,
    terrain: String,
    reason: String,
    actor_id: Option<String>,
    created_at: i64,
}

#[derive(sqlx::FromRow)]
struct DiscoverySql {
    id: String,
    region_id: String,
    local_x: i64,
    local_y: i64,
    kind: String,
    note: String,
    actor_id: Option<String>,
    created_at: i64,
}

#[derive(sqlx::FromRow)]
struct LinkSql {
    id: String,
    from_place: String,
    keyword: String,
    to_place: String,
    label: String,
    kind: String,
    created_by: Option<String>,
    created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::{gate_identity, generate_region, Terrain};
    use crate::model::STARTER_LOCATION;
    use crate::state::WorldState;
    use crate::undernet;
    use terminus_core::{Game, Permissions, SessionIdentity};

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
        assert!(loaded.geography.regions.is_empty());
    }

    #[tokio::test]
    async fn wilderness_survives_reopen_and_reseed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(undernet::locations(), &undernet::all_exits()).await.unwrap();

        let mut world = WorldState::for_tests();
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
        world.insert_operator(operator);
        let identity = SessionIdentity::new(op_id, "ada", Permissions::player());
        let session = identity.session_id;
        let (connected, _) = world.connect_player(identity).unwrap();
        store.commit(&connected.ops).await.unwrap();

        let mut ops = Vec::new();
        for line in ["go up", "outskirts"] {
            ops.extend(world.apply(session, line).unwrap().ops);
        }
        let region_identity = gate_identity();
        ops.push(
            world.apply_terrain_mod(region_identity, 8, 9, Terrain::Grass, "trail", op_id).unwrap(),
        );
        ops.extend(world.apply(session, "north").unwrap().ops);
        ops.extend(world.apply(session, "mark camp").unwrap().ops);
        ops.push(world.relocate_region(region_identity, 12, -2).unwrap());
        let future_id = Uuid::new_v4();
        ops.push(PersistOp::MaterializeRegion {
            id: future_id,
            world_id: "undernet".into(),
            epoch_id: 1,
            origin_x: 9,
            origin_y: 9,
            generation_version: 9,
            region_seed_hex: "0000000000000011".into(),
            placement_x: 9,
            placement_y: 9,
            discovered_at: 4,
        });
        store.commit(&ops).await.unwrap();
        sqlx::query("UPDATE world_regions SET region_seed = ?1 WHERE generation_version = 1")
            .bind("0000000000000000")
            .execute(&store.pool)
            .await
            .unwrap();

        drop(store);
        let store = Store::open(&path).await.unwrap();
        store.seed(undernet::locations(), &undernet::all_exits()).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.operators.len(), 1);
        assert_eq!(loaded.operators[0].username, "ada");
        assert_eq!(loaded.operators[0].items[0].keyword, "chip");
        assert!(
            loaded.operators[0].location_id.ends_with(":8:9"),
            "{}",
            loaded.operators[0].location_id
        );
        assert_eq!(loaded.geography.structures.len(), 1);
        assert_eq!(loaded.geography.structures[0].name, "camp");
        assert_eq!(loaded.geography.structures[0].owner_id, Some(op_id));
        assert_eq!(loaded.geography.terrain_mods.len(), 1);
        assert_eq!(loaded.geography.discoveries.len(), 1);
        assert_eq!(loaded.geography.regions.len(), 2);
        let origin = loaded
            .geography
            .regions
            .values()
            .find(|region| region.identity.generation_version == 1)
            .unwrap();
        assert_eq!(origin.region_seed, 0);
        assert_eq!((origin.placement_x, origin.placement_y), (12, -2));
        assert_eq!((origin.identity.origin_x, origin.identity.origin_y), (0, 0));
        let fresh = generate_region(origin.identity).unwrap();
        assert_eq!(fresh.identity.generation_version, 1);

        let mut again = WorldState::for_tests();
        again.install_loaded(loaded.operators.clone(), loaded.log.clone());
        again.install_geography(loaded.geography.clone());
        let back = SessionIdentity::new(op_id, "ada", Permissions::player());
        let back_session = back.session_id;
        again.connect_player(back).unwrap();
        let view = again.view(back_session).unwrap();
        assert!(view.objects.iter().any(|object| object.keyword == "camp"));
        assert!(view.description.contains("changed"), "{}", view.description);
        assert!(view.status.iter().any(|line| line.contains("atlas (12, -2)")));

        let structures: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM world_structures")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(structures, 1);
        let exits: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM exits").fetch_one(&store.pool).await.unwrap();
        assert!(exits >= 20);
    }
}
