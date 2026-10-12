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
    TerrainMod, WorldConnection, WorldId, ACCESS_PUBLIC, SHARE_PRIVATE,
};
use crate::home::{HomeState, PersonalHome};
use crate::invite::{new_invite, normalize_invite_code, AccountInvite, InviteBook};
use crate::mail::{MailMessage, Mailbox};
use crate::model::{Item, Operator, PersistOp, LOG_CAP};
use crate::settlement::{
    InviteStatus, MemberRole, Settlement, SettlementInvite, SettlementMember, SettlementState,
    SettlementVisibility,
};
use crate::social::{
    Friendship, FriendshipStatus, IgnoreEdge, NoticePost, SocialState, StructureGrant,
};
use crate::theories::{TheoryBoard, TheoryEntry, TheoryKind};
use crate::undernet::{ExitDef, LocationDef};
use crate::waypoint::Waypoint;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("migration failed")]
    Migrate(String),
    #[error("stored data is invalid")]
    Data,
    #[error("that username is already registered")]
    NameTaken,
    #[error("that invite code is not valid")]
    InviteMissing,
    #[error("that invite code was already used")]
    InviteUsed,
    #[error("this server is full")]
    AccountCap,
}

pub struct Store {
    pool: SqlitePool,
}

pub struct LoadedWorld {
    pub operators: Vec<Operator>,
    pub log: Vec<LogEntry>,
    pub geography: GeographyState,
    pub social: SocialState,
    pub settlements: SettlementState,
    pub waypoints: Vec<Waypoint>,
    pub homes: HomeState,
    pub mailbox: Mailbox,
    pub theories: TheoryBoard,
    pub invites: InviteBook,
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
        let social = self.load_social().await?;
        let settlements = self.load_settlements().await?;
        let waypoints = self.load_waypoints().await?;
        let homes = self.load_homes().await?;
        let mailbox = self.load_mail().await?;
        let theories = self.load_theories().await?;
        let invites = self.load_invites().await?;
        Ok(LoadedWorld {
            operators,
            log,
            geography,
            social,
            settlements,
            waypoints,
            homes,
            mailbox,
            theories,
            invites,
        })
    }

    pub async fn count_accounts(&self) -> Result<u64, StoreError> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM accounts")
            .fetch_one(&self.pool)
            .await?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// Insert unused invite codes. `created_by` is None for admin/seed batches.
    pub async fn create_invite_codes(
        &self,
        count: u32,
        created_by: Option<PlayerId>,
        created_at: u64,
        note: &str,
    ) -> Result<Vec<AccountInvite>, StoreError> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut rows = Vec::with_capacity(count as usize);
        for _ in 0..count {
            rows.push(new_invite(created_by, created_at, note));
        }
        let mut tx = self.pool.begin().await?;
        for row in &rows {
            insert_invite(&mut tx, row).await?;
        }
        tx.commit().await?;
        Ok(rows)
    }

    /// Create an account, optionally redeeming an invite and minting starter friend codes.
    pub async fn create_account_with_invite(
        &self,
        account: NewAccount<'_>,
        invite: AccountInvitePolicy<'_>,
    ) -> Result<Vec<AccountInvite>, StoreError> {
        let code_key = match invite.code.map(str::trim).filter(|code| !code.is_empty()) {
            Some(raw) => Some(normalize_invite_code(raw).map_err(|_| StoreError::InviteMissing)?),
            None if invite.require_code => return Err(StoreError::InviteMissing),
            None => None,
        };

        let mut tx = self.pool.begin().await?;
        if invite.max_accounts > 0 {
            let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM accounts")
                .fetch_one(&mut *tx)
                .await?;
            if count >= i64::from(invite.max_accounts) {
                return Err(StoreError::AccountCap);
            }
        }

        if let Some(key) = code_key.as_deref() {
            let row = sqlx::query_as::<_, InviteOpenSql>(
                "SELECT redeemed_by FROM account_invites WHERE code_key = ?1",
            )
            .bind(key)
            .fetch_optional(&mut *tx)
            .await?;
            match row {
                None => return Err(StoreError::InviteMissing),
                Some(existing) if existing.redeemed_by.is_some() => {
                    return Err(StoreError::InviteUsed);
                }
                Some(_) => {}
            }
        }

        if let Err(err) = insert_account_rows(
            &mut tx,
            account.account_id,
            account.username,
            account.password_hash,
            account.operator,
            account.created_at,
        )
        .await
        {
            if is_unique_constraint(&err) {
                return Err(StoreError::NameTaken);
            }
            return Err(err);
        }

        if let Some(key) = code_key.as_deref() {
            let updated = sqlx::query(
                "UPDATE account_invites
                 SET redeemed_by = ?1, redeemed_at = ?2
                 WHERE code_key = ?3 AND redeemed_by IS NULL",
            )
            .bind(account.operator.id.to_string())
            .bind(account.created_at as i64)
            .bind(key)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(StoreError::InviteUsed);
            }
        }

        let mut minted = Vec::new();
        if code_key.is_some() {
            for _ in 0..invite.starter_invites {
                let row = new_invite(Some(account.operator.id), account.created_at, "");
                insert_invite(&mut tx, &row).await?;
                minted.push(row);
            }
        }

        tx.commit().await?;
        Ok(minted)
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
        if let Err(err) =
            insert_account_rows(&mut tx, account_id, username, password_hash, operator, created_at)
                .await
        {
            if is_unique_constraint(&err) {
                return Err(StoreError::NameTaken);
            }
            return Err(err);
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
}

mod commit;
mod load;
mod rows;

use rows::{insert_account_rows, insert_invite, is_unique_constraint, AccountSql, InviteOpenSql};
pub use rows::{AccountInvitePolicy, AccountRow, NewAccount};

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
