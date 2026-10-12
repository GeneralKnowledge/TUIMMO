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
        })
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
            "INSERT INTO operators (id, account_id, name, location_id, created_at, last_seen, bio, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(operator.id.to_string())
        .bind(operator.account_id.to_string())
        .bind(&operator.name)
        .bind(&operator.location_id)
        .bind(created_at as i64)
        .bind(created_at as i64)
        .bind(&operator.bio)
        .bind(&operator.status)
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
                PersistOp::Profile { operator_id, bio, status } => {
                    sqlx::query("UPDATE operators SET bio = ?1, status = ?2 WHERE id = ?3")
                        .bind(bio)
                        .bind(status)
                        .bind(operator_id.to_string())
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
                    access,
                    interior_place,
                    settlement_id,
                    share,
                    created_at,
                } => {
                    sqlx::query(
                        "INSERT INTO world_structures (
                            id, region_id, local_x, local_y, kind, name, owner_id, access,
                            interior_place, settlement_id, share, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    )
                    .bind(id.to_string())
                    .bind(region_id.to_string())
                    .bind(i64::from(*local_x))
                    .bind(i64::from(*local_y))
                    .bind(kind)
                    .bind(name)
                    .bind(owner_id.map(|id| id.to_string()))
                    .bind(access)
                    .bind(interior_place)
                    .bind(settlement_id.map(|id| id.to_string()))
                    .bind(share)
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::UpdateStructure { id, name, access, interior_place, share } => {
                    sqlx::query(
                        "UPDATE world_structures
                         SET name = ?1, access = ?2, interior_place = ?3, share = ?4 WHERE id = ?5",
                    )
                    .bind(name)
                    .bind(access)
                    .bind(interior_place)
                    .bind(share)
                    .bind(id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::LinkStructureSettlement { id, settlement_id } => {
                    sqlx::query("UPDATE world_structures SET settlement_id = ?1 WHERE id = ?2")
                        .bind(settlement_id.map(|id| id.to_string()))
                        .bind(id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::DeleteStructure { id } => {
                    sqlx::query("DELETE FROM structure_grants WHERE structure_id = ?1")
                        .bind(id.to_string())
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("DELETE FROM world_structures WHERE id = ?1")
                        .bind(id.to_string())
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
                    content_key,
                    title,
                    place_key,
                } => {
                    sqlx::query(
                        "INSERT INTO region_discoveries (
                            id, region_id, local_x, local_y, kind, note, actor_id, created_at,
                            content_key, title, place_key
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    )
                    .bind(id.to_string())
                    .bind(region_id.map(|id| id.to_string()))
                    .bind(i64::from(*local_x))
                    .bind(i64::from(*local_y))
                    .bind(kind)
                    .bind(note)
                    .bind(actor_id.map(|id| id.to_string()))
                    .bind(*created_at as i64)
                    .bind(content_key)
                    .bind(title)
                    .bind(place_key)
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
                PersistOp::DeleteConnection { id } => {
                    sqlx::query("DELETE FROM world_connections WHERE id = ?1")
                        .bind(id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::FriendshipUpsert {
                    requester_id,
                    recipient_id,
                    status,
                    created_at,
                    updated_at,
                } => {
                    sqlx::query(
                        "INSERT INTO friendships (
                            requester_id, recipient_id, status, created_at, updated_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT (requester_id, recipient_id) DO UPDATE SET
                            status = excluded.status,
                            updated_at = excluded.updated_at",
                    )
                    .bind(requester_id.to_string())
                    .bind(recipient_id.to_string())
                    .bind(status.as_str())
                    .bind(*created_at as i64)
                    .bind(*updated_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::FriendshipDelete { requester_id, recipient_id } => {
                    sqlx::query(
                        "DELETE FROM friendships WHERE requester_id = ?1 AND recipient_id = ?2",
                    )
                    .bind(requester_id.to_string())
                    .bind(recipient_id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::IgnoreUpsert { operator_id, ignored_id, created_at } => {
                    sqlx::query(
                        "INSERT OR IGNORE INTO ignores (operator_id, ignored_id, created_at)
                         VALUES (?1, ?2, ?3)",
                    )
                    .bind(operator_id.to_string())
                    .bind(ignored_id.to_string())
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::IgnoreDelete { operator_id, ignored_id } => {
                    sqlx::query("DELETE FROM ignores WHERE operator_id = ?1 AND ignored_id = ?2")
                        .bind(operator_id.to_string())
                        .bind(ignored_id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::StructureGrantUpsert { structure_id, grantee_id, created_at } => {
                    sqlx::query(
                        "INSERT OR IGNORE INTO structure_grants (structure_id, grantee_id, created_at)
                         VALUES (?1, ?2, ?3)",
                    )
                    .bind(structure_id.to_string())
                    .bind(grantee_id.to_string())
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::StructureGrantDelete { structure_id, grantee_id } => {
                    sqlx::query(
                        "DELETE FROM structure_grants WHERE structure_id = ?1 AND grantee_id = ?2",
                    )
                    .bind(structure_id.to_string())
                    .bind(grantee_id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::NoticeUpsert {
                    id,
                    place_id,
                    author_id,
                    body,
                    created_at,
                    expires_at,
                } => {
                    sqlx::query(
                        "INSERT INTO noticeboard_posts (
                            id, place_id, author_id, body, created_at, expires_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                         ON CONFLICT(id) DO UPDATE SET
                            body = excluded.body,
                            expires_at = excluded.expires_at",
                    )
                    .bind(id.to_string())
                    .bind(place_id)
                    .bind(author_id.to_string())
                    .bind(body)
                    .bind(*created_at as i64)
                    .bind(*expires_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::NoticeDelete { id } => {
                    sqlx::query("DELETE FROM noticeboard_posts WHERE id = ?1")
                        .bind(id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::SettlementUpsert {
                    id,
                    name,
                    name_key,
                    description,
                    visibility,
                    anchor_place,
                    owner_id,
                    created_at,
                } => {
                    sqlx::query(
                        "INSERT INTO settlements (
                            id, name, name_key, description, visibility, anchor_place, owner_id, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                         ON CONFLICT(id) DO UPDATE SET
                            name = excluded.name,
                            name_key = excluded.name_key,
                            description = excluded.description,
                            visibility = excluded.visibility,
                            anchor_place = excluded.anchor_place,
                            owner_id = excluded.owner_id",
                    )
                    .bind(id.to_string())
                    .bind(name)
                    .bind(name_key)
                    .bind(description)
                    .bind(visibility.as_str())
                    .bind(anchor_place)
                    .bind(owner_id.to_string())
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::SettlementDelete { id } => {
                    sqlx::query(
                        "UPDATE world_structures SET settlement_id = NULL WHERE settlement_id = ?1",
                    )
                    .bind(id.to_string())
                    .execute(&mut *tx)
                    .await?;
                    sqlx::query("DELETE FROM noticeboard_posts WHERE place_id = ?1")
                        .bind(format!("settle:{id}"))
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("DELETE FROM settlement_invites WHERE settlement_id = ?1")
                        .bind(id.to_string())
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("DELETE FROM settlement_members WHERE settlement_id = ?1")
                        .bind(id.to_string())
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("DELETE FROM settlements WHERE id = ?1")
                        .bind(id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::SettlementMemberUpsert {
                    settlement_id,
                    operator_id,
                    role,
                    joined_at,
                } => {
                    sqlx::query(
                        "INSERT INTO settlement_members (
                            settlement_id, operator_id, role, joined_at
                         ) VALUES (?1, ?2, ?3, ?4)
                         ON CONFLICT(settlement_id, operator_id) DO UPDATE SET
                            role = excluded.role",
                    )
                    .bind(settlement_id.to_string())
                    .bind(operator_id.to_string())
                    .bind(role.as_str())
                    .bind(*joined_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::SettlementMemberDelete { settlement_id, operator_id } => {
                    sqlx::query(
                        "DELETE FROM settlement_members
                         WHERE settlement_id = ?1 AND operator_id = ?2",
                    )
                    .bind(settlement_id.to_string())
                    .bind(operator_id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::SettlementInviteUpsert {
                    id,
                    settlement_id,
                    inviter_id,
                    invitee_id,
                    status,
                    created_at,
                    expires_at,
                } => {
                    sqlx::query(
                        "INSERT INTO settlement_invites (
                            id, settlement_id, inviter_id, invitee_id, status, created_at, expires_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                         ON CONFLICT(id) DO UPDATE SET
                            status = excluded.status,
                            expires_at = excluded.expires_at",
                    )
                    .bind(id.to_string())
                    .bind(settlement_id.to_string())
                    .bind(inviter_id.to_string())
                    .bind(invitee_id.to_string())
                    .bind(status.as_str())
                    .bind(*created_at as i64)
                    .bind(*expires_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::WaypointInsert {
                    id,
                    operator_id,
                    name_key,
                    display_name,
                    place_key,
                    note,
                    created_at,
                    updated_at,
                } => {
                    sqlx::query(
                        "INSERT INTO waypoints (
                            id, operator_id, name_key, display_name, place_key, note,
                            created_at, updated_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    )
                    .bind(id.to_string())
                    .bind(operator_id.to_string())
                    .bind(name_key)
                    .bind(display_name)
                    .bind(place_key)
                    .bind(note)
                    .bind(*created_at as i64)
                    .bind(*updated_at as i64)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::WaypointNote { id, operator_id, note, updated_at } => {
                    sqlx::query(
                        "UPDATE waypoints SET note = ?1, updated_at = ?2
                         WHERE id = ?3 AND operator_id = ?4",
                    )
                    .bind(note)
                    .bind(*updated_at as i64)
                    .bind(id.to_string())
                    .bind(operator_id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::WaypointDelete { id, operator_id } => {
                    sqlx::query("DELETE FROM waypoints WHERE id = ?1 AND operator_id = ?2")
                        .bind(id.to_string())
                        .bind(operator_id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::MailInsert { id, sender_id, recipient_id, body, created_at } => {
                    let id = mail_id(*id)?;
                    let next_id = id.checked_add(1).ok_or(StoreError::Data)?;
                    sqlx::query(
                        "INSERT INTO mail (id, sender_id, recipient_id, body, created_at, read_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                    )
                    .bind(id)
                    .bind(sender_id.to_string())
                    .bind(recipient_id.to_string())
                    .bind(body)
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                    sqlx::query(
                        "UPDATE mail_seq SET next_id = CASE WHEN next_id < ?1 THEN ?1 ELSE next_id END
                         WHERE singleton = 1",
                    )
                    .bind(next_id)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::MailRead { id, recipient_id, read_at } => {
                    sqlx::query(
                        "UPDATE mail SET read_at = ?1
                         WHERE id = ?2 AND recipient_id = ?3 AND read_at IS NULL",
                    )
                    .bind(*read_at as i64)
                    .bind(mail_id(*id)?)
                    .bind(recipient_id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::MailDelete { id, recipient_id } => {
                    sqlx::query("DELETE FROM mail WHERE id = ?1 AND recipient_id = ?2")
                        .bind(mail_id(*id)?)
                        .bind(recipient_id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::TheoryInsert { id, topic, author_id, kind, body, created_at } => {
                    let id = i64::try_from(*id).map_err(|_| StoreError::Data)?;
                    let next_id = id.checked_add(1).ok_or(StoreError::Data)?;
                    sqlx::query(
                        "INSERT INTO shared_theories (id, topic, author_id, kind, body, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    )
                    .bind(id)
                    .bind(topic)
                    .bind(author_id.to_string())
                    .bind(kind.as_str())
                    .bind(body)
                    .bind(*created_at as i64)
                    .execute(&mut *tx)
                    .await?;
                    sqlx::query(
                        "UPDATE shared_theory_seq
                         SET next_id = CASE WHEN next_id < ?1 THEN ?1 ELSE next_id END
                         WHERE singleton = 1",
                    )
                    .bind(next_id)
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::TheoryDelete { id, author_id } => {
                    sqlx::query("DELETE FROM shared_theories WHERE id = ?1 AND author_id = ?2")
                        .bind(i64::try_from(*id).map_err(|_| StoreError::Data)?)
                        .bind(author_id.to_string())
                        .execute(&mut *tx)
                        .await?;
                }
                PersistOp::HomeClaim {
                    id,
                    operator_id,
                    place_key,
                    claimed_at,
                    public_name,
                    public_description,
                    meaningful_returns,
                    last_departed_at,
                    last_returned_at,
                } => {
                    sqlx::query(
                        "INSERT INTO personal_homes (
                            id, operator_id, place_key, claimed_at, abandoned_at,
                            public_name, public_description,
                            meaningful_returns, last_departed_at, last_returned_at
                         ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9)",
                    )
                    .bind(id.to_string())
                    .bind(operator_id.to_string())
                    .bind(place_key)
                    .bind(*claimed_at as i64)
                    .bind(public_name)
                    .bind(public_description)
                    .bind(i64::from(*meaningful_returns))
                    .bind(last_departed_at.map(|v| v as i64))
                    .bind(last_returned_at.map(|v| v as i64))
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::HomeAbandon { id, operator_id, abandoned_at } => {
                    sqlx::query(
                        "UPDATE personal_homes
                         SET abandoned_at = ?1, last_departed_at = NULL
                         WHERE id = ?2 AND operator_id = ?3 AND abandoned_at IS NULL",
                    )
                    .bind(*abandoned_at as i64)
                    .bind(id.to_string())
                    .bind(operator_id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::HomeRename { id, operator_id, public_name } => {
                    sqlx::query(
                        "UPDATE personal_homes
                         SET public_name = ?1
                         WHERE id = ?2 AND operator_id = ?3 AND abandoned_at IS NULL",
                    )
                    .bind(public_name)
                    .bind(id.to_string())
                    .bind(operator_id.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                PersistOp::HomeHabitation {
                    id,
                    operator_id,
                    meaningful_returns,
                    last_departed_at,
                    last_returned_at,
                } => {
                    sqlx::query(
                        "UPDATE personal_homes
                         SET meaningful_returns = ?1,
                             last_departed_at = ?2,
                             last_returned_at = ?3
                         WHERE id = ?4 AND operator_id = ?5 AND abandoned_at IS NULL",
                    )
                    .bind(i64::from(*meaningful_returns))
                    .bind(last_departed_at.map(|v| v as i64))
                    .bind(last_returned_at.map(|v| v as i64))
                    .bind(id.to_string())
                    .bind(operator_id.to_string())
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
            "SELECT o.id, o.account_id, a.username, o.name, o.location_id, o.bio, o.status
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
                bio: row.bio,
                status: row.status,
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
                "region" => Channel::Region,
                "private" => Channel::Private,
                _ => continue,
            };
            let audience = match row.audience.as_str() {
                "everyone" => Audience::Everyone,
                "location" => Audience::Location(row.audience_key),
                "region" => Audience::Region(row.audience_key),
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
            "SELECT id, region_id, local_x, local_y, kind, name, owner_id, access, interior_place,
                    settlement_id, share, created_at
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
                access: if row.access.is_empty() { ACCESS_PUBLIC.to_string() } else { row.access },
                interior_place: row.interior_place,
                settlement_id: optional_uuid(row.settlement_id)?,
                share: if row.share.is_empty() { SHARE_PRIVATE.to_string() } else { row.share },
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
            "SELECT id, region_id, local_x, local_y, kind, note, actor_id, created_at,
                    content_key, title, place_key
             FROM region_discoveries",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in discoveries {
            geography.discoveries.push(Discovery {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                region_id: optional_uuid(row.region_id)?,
                local_x: local_col(row.local_x)?,
                local_y: local_col(row.local_y)?,
                kind: row.kind,
                note: row.note,
                actor_id: optional_player(row.actor_id)?,
                created_at: u64_col(row.created_at)?,
                content_key: blank_to_none(row.content_key),
                title: blank_to_none(row.title),
                place_key: blank_to_none(row.place_key),
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

    async fn load_waypoints(&self) -> Result<Vec<Waypoint>, StoreError> {
        let rows = sqlx::query_as::<_, WaypointSql>(
            "SELECT id, operator_id, name_key, display_name, place_key, note, created_at, updated_at
             FROM waypoints",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut waypoints = Vec::with_capacity(rows.len());
        for row in rows {
            waypoints.push(Waypoint {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                operator_id: PlayerId::from_str(&row.operator_id).map_err(|_| StoreError::Data)?,
                name_key: row.name_key,
                display_name: row.display_name,
                place_key: row.place_key,
                note: row.note,
                created_at: u64_col(row.created_at)?,
                updated_at: u64_col(row.updated_at)?,
            });
        }
        Ok(waypoints)
    }

    async fn load_homes(&self) -> Result<HomeState, StoreError> {
        let rows = sqlx::query_as::<_, HomeSql>(
            "SELECT id, operator_id, place_key, claimed_at, abandoned_at,
                    public_name, public_description,
                    meaningful_returns, last_departed_at, last_returned_at
             FROM personal_homes",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut homes = Vec::with_capacity(rows.len());
        for row in rows {
            homes.push(PersonalHome {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                operator_id: PlayerId::from_str(&row.operator_id).map_err(|_| StoreError::Data)?,
                place_key: row.place_key,
                claimed_at: u64_col(row.claimed_at)?,
                abandoned_at: row.abandoned_at.map(u64_col).transpose()?,
                public_name: row.public_name,
                public_description: row.public_description,
                meaningful_returns: u32_col(row.meaningful_returns)?,
                last_departed_at: row.last_departed_at.map(u64_col).transpose()?,
                last_returned_at: row.last_returned_at.map(u64_col).transpose()?,
            });
        }
        Ok(HomeState { homes })
    }

    async fn load_mail(&self) -> Result<Mailbox, StoreError> {
        let rows = sqlx::query_as::<_, MailSql>(
            "SELECT id, sender_id, recipient_id, body, created_at, read_at FROM mail",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            messages.push(MailMessage {
                id: u64_col(row.id)?,
                sender_id: PlayerId::from_str(&row.sender_id).map_err(|_| StoreError::Data)?,
                recipient_id: PlayerId::from_str(&row.recipient_id)
                    .map_err(|_| StoreError::Data)?,
                body: row.body,
                created_at: u64_col(row.created_at)?,
                read_at: row.read_at.map(u64_col).transpose()?,
            });
        }
        let next_id =
            sqlx::query_scalar::<_, i64>("SELECT next_id FROM mail_seq WHERE singleton = 1")
                .fetch_one(&self.pool)
                .await?;
        Ok(Mailbox::from_stored(messages, u64_col(next_id)?))
    }

    async fn load_theories(&self) -> Result<TheoryBoard, StoreError> {
        let rows = sqlx::query_as::<_, TheorySql>(
            "SELECT id, topic, author_id, kind, body, created_at FROM shared_theories",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            entries.push(TheoryEntry {
                id: u64_col(row.id)?,
                topic: row.topic,
                author_id: PlayerId::from_str(&row.author_id).map_err(|_| StoreError::Data)?,
                kind: TheoryKind::parse(&row.kind).ok_or(StoreError::Data)?,
                body: row.body,
                created_at: u64_col(row.created_at)?,
            });
        }
        let next_id = sqlx::query_scalar::<_, i64>(
            "SELECT next_id FROM shared_theory_seq WHERE singleton = 1",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(TheoryBoard::from_stored(entries, u64_col(next_id)?))
    }

    async fn load_social(&self) -> Result<SocialState, StoreError> {
        let mut social = SocialState::default();
        let friends = sqlx::query_as::<_, FriendshipSql>(
            "SELECT requester_id, recipient_id, status, created_at, updated_at FROM friendships",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in friends {
            social.friendships.push(Friendship {
                requester_id: PlayerId::from_str(&row.requester_id)
                    .map_err(|_| StoreError::Data)?,
                recipient_id: PlayerId::from_str(&row.recipient_id)
                    .map_err(|_| StoreError::Data)?,
                status: FriendshipStatus::parse(&row.status).ok_or(StoreError::Data)?,
                created_at: u64_col(row.created_at)?,
                updated_at: u64_col(row.updated_at)?,
            });
        }
        let ignores = sqlx::query_as::<_, IgnoreSql>(
            "SELECT operator_id, ignored_id, created_at FROM ignores",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in ignores {
            social.ignores.push(IgnoreEdge {
                operator_id: PlayerId::from_str(&row.operator_id).map_err(|_| StoreError::Data)?,
                ignored_id: PlayerId::from_str(&row.ignored_id).map_err(|_| StoreError::Data)?,
                created_at: u64_col(row.created_at)?,
            });
        }
        let grants = sqlx::query_as::<_, GrantSql>(
            "SELECT structure_id, grantee_id, created_at FROM structure_grants",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in grants {
            social.grants.push(StructureGrant {
                structure_id: Uuid::from_str(&row.structure_id).map_err(|_| StoreError::Data)?,
                grantee_id: PlayerId::from_str(&row.grantee_id).map_err(|_| StoreError::Data)?,
                created_at: u64_col(row.created_at)?,
            });
        }
        let notices = sqlx::query_as::<_, NoticeSql>(
            "SELECT id, place_id, author_id, body, created_at, expires_at FROM noticeboard_posts",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in notices {
            social.notices.push(NoticePost {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                place_id: row.place_id,
                author_id: PlayerId::from_str(&row.author_id).map_err(|_| StoreError::Data)?,
                body: row.body,
                created_at: u64_col(row.created_at)?,
                expires_at: u64_col(row.expires_at)?,
            });
        }
        Ok(social)
    }

    async fn load_settlements(&self) -> Result<SettlementState, StoreError> {
        let mut state = SettlementState::default();
        let settlements = sqlx::query_as::<_, SettlementSql>(
            "SELECT id, name, name_key, description, visibility, anchor_place, owner_id, created_at
             FROM settlements",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in settlements {
            state.settlements.push(Settlement {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                name: row.name,
                name_key: row.name_key,
                description: row.description,
                visibility: SettlementVisibility::parse(&row.visibility).ok_or(StoreError::Data)?,
                anchor_place: row.anchor_place,
                owner_id: PlayerId::from_str(&row.owner_id).map_err(|_| StoreError::Data)?,
                created_at: u64_col(row.created_at)?,
            });
        }
        let members = sqlx::query_as::<_, SettlementMemberSql>(
            "SELECT settlement_id, operator_id, role, joined_at FROM settlement_members",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in members {
            state.members.push(SettlementMember {
                settlement_id: Uuid::from_str(&row.settlement_id).map_err(|_| StoreError::Data)?,
                operator_id: PlayerId::from_str(&row.operator_id).map_err(|_| StoreError::Data)?,
                role: MemberRole::parse(&row.role).ok_or(StoreError::Data)?,
                joined_at: u64_col(row.joined_at)?,
            });
        }
        let invites = sqlx::query_as::<_, SettlementInviteSql>(
            "SELECT id, settlement_id, inviter_id, invitee_id, status, created_at, expires_at
             FROM settlement_invites",
        )
        .fetch_all(&self.pool)
        .await?;
        for row in invites {
            state.invites.push(SettlementInvite {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                settlement_id: Uuid::from_str(&row.settlement_id).map_err(|_| StoreError::Data)?,
                inviter_id: PlayerId::from_str(&row.inviter_id).map_err(|_| StoreError::Data)?,
                invitee_id: PlayerId::from_str(&row.invitee_id).map_err(|_| StoreError::Data)?,
                status: InviteStatus::parse(&row.status).ok_or(StoreError::Data)?,
                created_at: u64_col(row.created_at)?,
                expires_at: u64_col(row.expires_at)?,
            });
        }
        Ok(state)
    }
}

fn mail_id(id: u64) -> Result<i64, StoreError> {
    i64::try_from(id).map_err(|_| StoreError::Data)
}

fn i32_col(value: i64) -> Result<i32, StoreError> {
    i32::try_from(value).map_err(|_| StoreError::Data)
}

fn u64_col(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Data)
}

fn u32_col(value: i64) -> Result<u32, StoreError> {
    u32::try_from(value).map_err(|_| StoreError::Data)
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

fn optional_uuid(value: Option<String>) -> Result<Option<Uuid>, StoreError> {
    value.map(|raw| Uuid::from_str(&raw).map_err(|_| StoreError::Data)).transpose()
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
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
    bio: String,
    status: String,
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
    access: String,
    interior_place: Option<String>,
    settlement_id: Option<String>,
    share: String,
    created_at: i64,
}

#[derive(sqlx::FromRow)]
struct SettlementSql {
    id: String,
    name: String,
    name_key: String,
    description: String,
    visibility: String,
    anchor_place: String,
    owner_id: String,
    created_at: i64,
}

#[derive(sqlx::FromRow)]
struct SettlementMemberSql {
    settlement_id: String,
    operator_id: String,
    role: String,
    joined_at: i64,
}

#[derive(sqlx::FromRow)]
struct SettlementInviteSql {
    id: String,
    settlement_id: String,
    inviter_id: String,
    invitee_id: String,
    status: String,
    created_at: i64,
    expires_at: i64,
}

#[derive(sqlx::FromRow)]
struct FriendshipSql {
    requester_id: String,
    recipient_id: String,
    status: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(sqlx::FromRow)]
struct IgnoreSql {
    operator_id: String,
    ignored_id: String,
    created_at: i64,
}

#[derive(sqlx::FromRow)]
struct GrantSql {
    structure_id: String,
    grantee_id: String,
    created_at: i64,
}

#[derive(sqlx::FromRow)]
struct NoticeSql {
    id: String,
    place_id: String,
    author_id: String,
    body: String,
    created_at: i64,
    expires_at: i64,
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
struct MailSql {
    id: i64,
    sender_id: String,
    recipient_id: String,
    body: String,
    created_at: i64,
    read_at: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct TheorySql {
    id: i64,
    topic: String,
    author_id: String,
    kind: String,
    body: String,
    created_at: i64,
}

#[derive(sqlx::FromRow)]
struct WaypointSql {
    id: String,
    operator_id: String,
    name_key: String,
    display_name: String,
    place_key: String,
    note: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(sqlx::FromRow)]
struct HomeSql {
    id: String,
    operator_id: String,
    place_key: String,
    claimed_at: i64,
    abandoned_at: Option<i64>,
    public_name: String,
    public_description: String,
    meaningful_returns: i64,
    last_departed_at: Option<i64>,
    last_returned_at: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct DiscoverySql {
    id: String,
    region_id: Option<String>,
    local_x: i64,
    local_y: i64,
    kind: String,
    note: String,
    actor_id: Option<String>,
    created_at: i64,
    content_key: Option<String>,
    title: Option<String>,
    place_key: Option<String>,
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
    use std::str::FromStr;

    use super::*;
    use crate::geography::{gate_identity, generate_region, Terrain, WildernessPos};
    use crate::model::STARTER_LOCATION;
    use crate::state::WorldState;
    use crate::undernet;
    use terminus_core::{Game, Permissions, SessionIdentity};

    #[tokio::test]
    async fn account_and_move_survive_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let op_id = PlayerId::new();
        let account_id = Uuid::new_v4();
        let operator = Operator {
            id: op_id,
            account_id,
            username: "ada".into(),
            name: "ada".into(),
            location_id: STARTER_LOCATION.into(),
            bio: String::new(),
            status: String::new(),
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
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();

        let mut world = WorldState::for_tests();
        let op_id = PlayerId::new();
        let account_id = Uuid::new_v4();
        let operator = Operator {
            id: op_id,
            account_id,
            username: "ada".into(),
            name: "ada".into(),
            location_id: STARTER_LOCATION.into(),
            bio: String::new(),
            status: String::new(),
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
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
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
        again.install_social(loaded.social.clone());
        again.install_settlements(loaded.settlements.clone());
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

    #[tokio::test]
    async fn social_cabin_and_notices_survive_reopen_and_reseed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();

        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        let ada_account = Uuid::new_v4();
        let bob_account = Uuid::new_v4();
        let ada = Operator {
            id: ada_id,
            account_id: ada_account,
            username: "ada".into(),
            name: "ada".into(),
            location_id: STARTER_LOCATION.into(),
            bio: String::new(),
            status: String::new(),
            flags: BTreeSet::new(),
            items: vec![Item::chip()],
        };
        let bob = Operator {
            id: bob_id,
            account_id: bob_account,
            username: "bob".into(),
            name: "bob".into(),
            location_id: STARTER_LOCATION.into(),
            bio: String::new(),
            status: String::new(),
            flags: BTreeSet::new(),
            items: vec![Item::chip()],
        };
        store.create_account(ada_account, "ada", "hash", &ada, 10).await.unwrap();
        store.create_account(bob_account, "bob", "hash", &bob, 11).await.unwrap();
        world.insert_operator(ada);
        world.insert_operator(bob);
        let ada_ident = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let bob_ident = SessionIdentity::new(bob_id, "bob", Permissions::player());
        let ada_session = ada_ident.session_id;
        let bob_session = bob_ident.session_id;
        store.commit(&world.connect_player(ada_ident).unwrap().0.ops).await.unwrap();
        store.commit(&world.connect_player(bob_ident).unwrap().0.ops).await.unwrap();

        let mut ops = Vec::new();
        ops.extend(world.apply(ada_session, "friend add bob").unwrap().ops);
        ops.extend(world.apply(bob_session, "friend accept ada").unwrap().ops);
        ops.extend(world.apply(ada_session, "profile bio Homesteader").unwrap().ops);
        ops.extend(world.apply(ada_session, "board post Visit Haven").unwrap().ops);
        for line in ["go up", "outskirts"] {
            ops.extend(world.apply(ada_session, line).unwrap().ops);
        }
        let identity = gate_identity();
        ops.push(world.apply_terrain_mod(identity, 8, 9, Terrain::Grass, "pad", ada_id).unwrap());
        ops.extend(world.apply(ada_session, "north").unwrap().ops);
        ops.extend(world.apply(ada_session, "build cabin Haven").unwrap().ops);
        ops.extend(world.apply(ada_session, "sign Come visit").unwrap().ops);
        store.commit(&ops).await.unwrap();

        drop(store);
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert!(loaded.social.are_friends(ada_id, bob_id));
        assert_eq!(loaded.operators.iter().find(|op| op.id == ada_id).unwrap().bio, "Homesteader");
        assert_eq!(loaded.social.notices.len(), 1);
        assert!(loaded.geography.structures.iter().any(|s| s.kind == "cabin" && s.name == "Haven"));
        assert!(loaded.geography.structures.iter().any(|s| s.kind == "sign"));
        assert_eq!(loaded.geography.connections.len(), 2);

        let mut again = WorldState::for_tests();
        again.install_loaded(loaded.operators, loaded.log);
        again.install_geography(loaded.geography);
        again.install_social(loaded.social);
        again.install_settlements(loaded.settlements);
        let back = SessionIdentity::new(bob_id, "bob", Permissions::player());
        let bob_back = back.session_id;
        again.connect_player(back).unwrap();
        // Place bob at the cabin exterior.
        let cabin = again.geography.structures.iter().find(|s| s.kind == "cabin").unwrap();
        let region = again.geography.region_by_id(cabin.region_id).unwrap();
        let exterior = WildernessPos {
            identity: region.identity,
            local_x: cabin.local_x,
            local_y: cabin.local_y,
        }
        .key();
        again.operators.get_mut(&bob_id).unwrap().location_id = exterior;
        again.apply(bob_back, "enter Haven").unwrap();
        assert!(again.view(bob_back).unwrap().location_name.contains("Haven"));
    }

    #[tokio::test]
    async fn settlements_survive_reopen_and_reseed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();

        let mut world = WorldState::for_tests();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        let ada_account = Uuid::new_v4();
        let bob_account = Uuid::new_v4();
        let ada = Operator {
            id: ada_id,
            account_id: ada_account,
            username: "ada".into(),
            name: "ada".into(),
            location_id: STARTER_LOCATION.into(),
            bio: String::new(),
            status: String::new(),
            flags: BTreeSet::new(),
            items: vec![Item::chip()],
        };
        let bob = Operator {
            id: bob_id,
            account_id: bob_account,
            username: "bob".into(),
            name: "bob".into(),
            location_id: STARTER_LOCATION.into(),
            bio: String::new(),
            status: String::new(),
            flags: BTreeSet::new(),
            items: vec![Item::chip()],
        };
        store.create_account(ada_account, "ada", "hash", &ada, 10).await.unwrap();
        store.create_account(bob_account, "bob", "hash", &bob, 11).await.unwrap();
        world.insert_operator(ada);
        world.insert_operator(bob);
        let ada_ident = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let bob_ident = SessionIdentity::new(bob_id, "bob", Permissions::player());
        let ada_session = ada_ident.session_id;
        let bob_session = bob_ident.session_id;
        store.commit(&world.connect_player(ada_ident).unwrap().0.ops).await.unwrap();
        store.commit(&world.connect_player(bob_ident).unwrap().0.ops).await.unwrap();

        let mut ops = Vec::new();
        for line in ["go up", "outskirts"] {
            ops.extend(world.apply(ada_session, line).unwrap().ops);
        }
        let identity = gate_identity();
        ops.push(world.apply_terrain_mod(identity, 8, 9, Terrain::Grass, "pad", ada_id).unwrap());
        ops.extend(world.apply(ada_session, "north").unwrap().ops);
        ops.extend(world.apply(ada_session, "build cabin Haven").unwrap().ops);
        ops.extend(world.apply(ada_session, "settle create Havenhold").unwrap().ops);
        ops.extend(world.apply(ada_session, "settle invite bob").unwrap().ops);
        ops.extend(world.apply(bob_session, "settle accept Havenhold").unwrap().ops);
        ops.extend(world.apply(ada_session, "settle board post Meet on the ridge").unwrap().ops);
        store.commit(&ops).await.unwrap();

        drop(store);
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.settlements.settlements.len(), 1);
        assert_eq!(loaded.settlements.settlements[0].name, "Havenhold");
        assert!(loaded.settlements.is_member(loaded.settlements.settlements[0].id, bob_id));
        assert!(loaded.geography.structures.iter().any(|s| {
            s.kind == "cabin" && s.settlement_id == Some(loaded.settlements.settlements[0].id)
        }));
        assert!(loaded.social.notices.iter().any(|n| n.place_id.starts_with("settle:")));
    }

    #[tokio::test]
    async fn migration_applies_to_fresh_and_preexisting_databases() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("fresh.sqlite");
        let store = Store::open(&fresh).await.unwrap();
        let tables: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .fetch_all(&store.pool)
                .await
                .unwrap();
        assert!(tables.iter().any(|name| name == "friendships"));
        assert!(tables.iter().any(|name| name == "noticeboard_posts"));
        assert!(tables.iter().any(|name| name == "ignores"));
        assert!(tables.iter().any(|name| name == "settlements"));
        assert!(tables.iter().any(|name| name == "settlement_members"));
        assert!(tables.iter().any(|name| name == "settlement_invites"));
        assert!(tables.iter().any(|name| name == "waypoints"));
        assert!(tables.iter().any(|name| name == "mail"));
        assert!(tables.iter().any(|name| name == "mail_seq"));
        assert!(tables.iter().any(|name| name == "shared_theories"));
        assert!(tables.iter().any(|name| name == "shared_theory_seq"));
        assert!(tables.iter().any(|name| name == "personal_homes"));
        let home_returns: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('personal_homes') WHERE name = 'meaningful_returns'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(home_returns, 1);
        let access_col: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('world_structures') WHERE name = 'access'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(access_col, 1);
        let settle_col: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('world_structures') WHERE name = 'settlement_id'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(settle_col, 1);
        let bio_col: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('operators') WHERE name = 'bio'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(bio_col, 1);
        let content_key_col: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('region_discoveries') WHERE name = 'content_key'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(content_key_col, 1);
        drop(store);

        // Re-open an already-migrated database (forward-only migrations stay idempotent).
        let store = Store::open(&fresh).await.unwrap();
        let friendships: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM friendships")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(friendships, 0);
    }

    #[tokio::test]
    async fn failed_discovery_commit_rolls_back_the_whole_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        let err = store
            .commit(&[
                PersistOp::Message {
                    at_ms: 1,
                    channel: "system".into(),
                    audience: "player".into(),
                    audience_key: "nope".into(),
                    speaker: None,
                    body: "partial".into(),
                },
                PersistOp::Discovery {
                    id: Uuid::new_v4(),
                    region_id: Some(Uuid::new_v4()),
                    local_x: 0,
                    local_y: 0,
                    kind: "field".into(),
                    note: "should not stick".into(),
                    actor_id: None,
                    created_at: 1,
                    content_key: Some("survey-scratch".into()),
                    title: Some("Gate scratch".into()),
                    place_key: Some("roof-array".into()),
                },
            ])
            .await;
        assert!(err.is_err());
        let loaded = store.load().await.unwrap();
        assert!(loaded.log.is_empty());
        assert!(loaded.geography.discoveries.is_empty());
    }

    #[tokio::test]
    async fn delisted_floor_survives_reopen_and_a_failed_write_adds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        let ada = Operator {
            id: ada_id,
            account_id: Uuid::new_v4(),
            username: "ada".into(),
            name: "ada".into(),
            location_id: STARTER_LOCATION.into(),
            bio: String::new(),
            status: String::new(),
            flags: BTreeSet::new(),
            items: vec![Item::chip()],
        };
        let bob = Operator {
            id: bob_id,
            account_id: Uuid::new_v4(),
            username: "bob".into(),
            name: "bob".into(),
            location_id: STARTER_LOCATION.into(),
            bio: String::new(),
            status: String::new(),
            flags: BTreeSet::new(),
            items: vec![Item::chip()],
        };
        store.create_account(ada.account_id, "ada", "hash", &ada, 10).await.unwrap();
        store.create_account(bob.account_id, "bob", "hash", &bob, 11).await.unwrap();

        let mut world = WorldState::for_tests();
        world.insert_operator(ada);
        world.insert_operator(bob);
        let identity = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let session = identity.session_id;
        let (connected, _) = world.connect_player(identity).unwrap();
        store.commit(&connected.ops).await.unwrap();
        let mut ops = Vec::new();
        for line in
            ["alley", "exchange", "service", "down", "inspect terminal", "up", "journal record"]
        {
            ops.extend(world.apply(session, line).unwrap().ops);
        }
        store.commit(&ops).await.unwrap();
        drop(store);

        let store = Store::open(&path).await.unwrap();
        let loaded = store.load().await.unwrap();
        let ada = loaded.operators.iter().find(|op| op.id == ada_id).unwrap();
        assert!(ada.flags.contains(undernet::READ_TERMINAL_FLAG));
        assert!(ada.items.iter().any(|item| {
            item.keyword == "torn-log"
                && item.detail.contains("B-4")
                && item.detail.contains("public list")
        }));
        assert_eq!(loaded.geography.field_notes_for(ada_id).len(), 1);
        let note = loaded.geography.field_note(ada_id, "delisted-floor").unwrap();
        assert!(note.note.contains("struck from the public list"));
        assert_eq!(note.place_key.as_deref(), Some("service-corridor"));
        let bob = loaded.operators.iter().find(|op| op.id == bob_id).unwrap();
        assert!(!bob.flags.contains(undernet::READ_TERMINAL_FLAG));
        assert!(bob.items.iter().all(|item| item.keyword != "torn-log"));
        assert!(loaded.geography.field_notes_for(bob_id).is_empty());

        let ghost = PlayerId::new();
        let err = store
            .commit(&[
                PersistOp::Discovery {
                    id: Uuid::new_v4(),
                    region_id: None,
                    local_x: 0,
                    local_y: 0,
                    kind: "field".into(),
                    note: "should not stick".into(),
                    actor_id: Some(ada_id),
                    created_at: 9,
                    content_key: Some("should-not-stick".into()),
                    title: Some("Nope".into()),
                    place_key: Some("service-corridor".into()),
                },
                PersistOp::Flag { operator_id: ghost, flag: undernet::READ_TERMINAL_FLAG.into() },
                PersistOp::Item {
                    operator_id: ghost,
                    keyword: "torn-log".into(),
                    name: "torn log".into(),
                    detail: "partial".into(),
                },
            ])
            .await;
        assert!(err.is_err());
        let loaded = store.load().await.unwrap();
        assert!(loaded.geography.field_note(ada_id, "should-not-stick").is_none());
        assert_eq!(loaded.geography.field_notes_for(ada_id).len(), 1);
        assert_eq!(loaded.operators.len(), 2);
        assert!(loaded.operators.iter().all(|op| op.id != ghost));
        let ada = loaded.operators.iter().find(|op| op.id == ada_id).unwrap();
        assert_eq!(ada.items.iter().filter(|item| item.keyword == "torn-log").count(), 1);
    }

    #[tokio::test]
    async fn field_notes_migrate_existing_rows_and_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite");
        let legacy_dir = dir.path().join("legacy-migrations");
        std::fs::create_dir(&legacy_dir).unwrap();
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        for name in [
            "0001_init.sql",
            "0002_world.sql",
            "0003_social_and_places.sql",
            "0004_settlements.sql",
            "0005_sharing_and_workshops.sql",
        ] {
            std::fs::copy(manifest.join(name), legacy_dir.join(name)).unwrap();
        }
        let url = format!("sqlite://{}", path.display());
        let options = sqlx::sqlite::SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate::Migrator::new(legacy_dir).await.unwrap().run(&pool).await.unwrap();

        let ada_account = Uuid::new_v4();
        let bob_account = Uuid::new_v4();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        let region_id = Uuid::new_v4();
        let cabin_id = Uuid::new_v4();
        let sign_id = Uuid::new_v4();
        let trail_id = Uuid::new_v4();
        let entered_id = Uuid::new_v4();
        let settle_id = Uuid::new_v4();
        let invite_id = Uuid::new_v4();
        let notice_id = Uuid::new_v4();
        let place = "w:undernet:1:0:0:1:8:9";

        sqlx::query(
            "INSERT INTO accounts (id, username, password_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(ada_account.to_string())
        .bind("ada")
        .bind("hash")
        .bind(10_i64)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO accounts (id, username, password_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(bob_account.to_string())
        .bind("bob")
        .bind("hash")
        .bind(11_i64)
        .execute(&pool)
        .await
        .unwrap();
        for (id, account, name, at) in
            [(ada_id, ada_account, "ada", 10_i64), (bob_id, bob_account, "bob", 11_i64)]
        {
            sqlx::query(
                "INSERT INTO operators (id, account_id, name, location_id, created_at, last_seen)
                 VALUES (?1, ?2, ?3, 'coffee-shop', ?4, ?4)",
            )
            .bind(id.to_string())
            .bind(account.to_string())
            .bind(name)
            .bind(at)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO operator_flags (operator_id, flag) VALUES (?1, 'cipher-plate')")
            .bind(ada_id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO operator_items (operator_id, item_key, name, detail) VALUES (?1, 'chip', 'loyalty chip', 'scuffed')",
        )
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO friendships (requester_id, recipient_id, status, created_at, updated_at)
             VALUES (?1, ?2, 'accepted', 12, 13)",
        )
        .bind(ada_id.to_string())
        .bind(bob_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ignores (operator_id, ignored_id, created_at) VALUES (?1, ?2, 14)",
        )
        .bind(bob_id.to_string())
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO world_regions (
                id, world_id, epoch_id, origin_x, origin_y, generation_version, region_seed,
                placement_x, placement_y, discovered_at
             ) VALUES (?1, 'undernet', 1, 0, 0, 1, '0000000000000001', 0, 0, 20)",
        )
        .bind(region_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO world_structures (
                id, region_id, local_x, local_y, kind, name, owner_id, access, interior_place,
                settlement_id, share, created_at
             ) VALUES (?1, ?2, 8, 9, 'cabin', 'Haven', ?3, 'public', NULL, NULL, 'private', 21)",
        )
        .bind(cabin_id.to_string())
        .bind(region_id.to_string())
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO world_structures (
                id, region_id, local_x, local_y, kind, name, owner_id, access, interior_place,
                settlement_id, share, created_at
             ) VALUES (?1, ?2, 8, 9, 'sign', 'Come visit', ?3, 'public', NULL, NULL, 'private', 22)",
        )
        .bind(sign_id.to_string())
        .bind(region_id.to_string())
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO world_structures (
                id, region_id, local_x, local_y, kind, name, owner_id, access, interior_place,
                settlement_id, share, created_at
             ) VALUES (?1, ?2, 8, 9, 'trail', 'ridge', ?3, 'public', NULL, NULL, 'private', 23)",
        )
        .bind(trail_id.to_string())
        .bind(region_id.to_string())
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO terrain_modifications (
                region_id, local_x, local_y, terrain, reason, actor_id, created_at
             ) VALUES (?1, 8, 9, 'grass', 'pad', ?2, 24)",
        )
        .bind(region_id.to_string())
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO region_discoveries (
                id, region_id, local_x, local_y, kind, note, actor_id, created_at
             ) VALUES (?1, ?2, 8, 8, 'entered', 'Entered the region.', ?3, 25)",
        )
        .bind(entered_id.to_string())
        .bind(region_id.to_string())
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO settlements (
                id, name, name_key, description, visibility, anchor_place, owner_id, created_at
             ) VALUES (?1, 'Havenhold', 'havenhold', 'ridge camp', 'public', ?2, ?3, 26)",
        )
        .bind(settle_id.to_string())
        .bind(place)
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO settlement_members (settlement_id, operator_id, role, joined_at)
             VALUES (?1, ?2, 'owner', 26)",
        )
        .bind(settle_id.to_string())
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO settlement_invites (
                id, settlement_id, inviter_id, invitee_id, status, created_at, expires_at
             ) VALUES (?1, ?2, ?3, ?4, 'pending', 27, 900000)",
        )
        .bind(invite_id.to_string())
        .bind(settle_id.to_string())
        .bind(ada_id.to_string())
        .bind(bob_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO noticeboard_posts (id, place_id, author_id, body, created_at, expires_at)
             VALUES (?1, 'coffee-shop', ?2, 'Visit Haven', 28, 900000)",
        )
        .bind(notice_id.to_string())
        .bind(ada_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        drop(pool);

        let store = Store::open(&path).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert!(loaded.waypoints.is_empty());
        let ada = loaded.operators.iter().find(|op| op.id == ada_id).unwrap();
        let bob = loaded.operators.iter().find(|op| op.id == bob_id).unwrap();
        assert_eq!(ada.location_id, "coffee-shop");
        assert!(ada.flags.contains("cipher-plate"));
        assert_eq!(ada.items[0].keyword, "chip");
        assert_eq!(bob.username, "bob");
        assert!(loaded.social.are_friends(ada_id, bob_id));
        assert!(loaded.social.ignores.iter().any(|edge| edge.operator_id == bob_id));
        assert_eq!(loaded.settlements.settlements.len(), 1);
        assert_eq!(loaded.settlements.settlements[0].name, "Havenhold");
        assert_eq!(loaded.settlements.invites.len(), 1);
        assert!(loaded
            .geography
            .structures
            .iter()
            .any(|row| row.kind == "cabin" && row.name == "Haven"));
        assert!(loaded.geography.structures.iter().any(|row| row.kind == "sign"));
        assert!(loaded.geography.structures.iter().any(|row| row.kind == "trail"));
        assert_eq!(loaded.geography.terrain_mods.len(), 1);
        assert_eq!(loaded.social.notices.len(), 1);
        let entered = loaded.geography.discoveries.iter().find(|row| row.id == entered_id).unwrap();
        assert_eq!(entered.kind, "entered");
        assert!(entered.content_key.is_none());
        assert_eq!(entered.actor_id, Some(ada_id));
        assert_eq!(entered.region_id, Some(region_id));

        let mut world = WorldState::for_tests();
        world.install_loaded(loaded.operators, loaded.log);
        world.install_geography(loaded.geography);
        world.install_social(loaded.social);
        world.install_settlements(loaded.settlements);
        let ada_ident = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let ada_session = ada_ident.session_id;
        let ops = world.connect_player(ada_ident).unwrap().0.ops;
        store.commit(&ops).await.unwrap();
        let mut ops = world.apply(ada_session, "go up").unwrap().ops;
        ops.extend(world.apply(ada_session, "journal record").unwrap().ops);
        store.commit(&ops).await.unwrap();
        drop(store);

        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let loaded = store.load().await.unwrap();
        let ada = loaded.operators.iter().find(|op| op.id == ada_id).unwrap();
        assert_eq!(ada.location_id, "roof-array");
        assert!(ada.flags.contains("cipher-plate"));
        assert!(loaded.social.are_friends(ada_id, bob_id));
        assert!(loaded.geography.structures.iter().any(|row| row.name == "Haven"));
        assert!(loaded
            .geography
            .discoveries
            .iter()
            .any(|row| row.id == entered_id && row.kind == "entered"));
        let note = loaded
            .geography
            .discoveries
            .iter()
            .find(|row| row.kind == "field" && row.actor_id == Some(ada_id))
            .unwrap();
        assert_eq!(note.content_key.as_deref(), Some("survey-scratch"));
        assert_eq!(note.place_key.as_deref(), Some("roof-array"));
        assert!(note.region_id.is_none());
        assert_eq!(loaded.settlements.settlements[0].owner_id, ada_id);

        let mut again = WorldState::for_tests();
        again.install_loaded(loaded.operators, loaded.log);
        again.install_geography(loaded.geography);
        let back = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let session = back.session_id;
        again.connect_player(back).unwrap();
        again.apply(session, "journal read survey-scratch").unwrap();
        let page = again.view(session).unwrap().overlay.unwrap_or_default();
        assert!(page.contains("one step west of the gate"));
        assert!(page.contains("Roof Array"));
        again.apply(session, "journal record").unwrap();
        assert_eq!(again.geography.field_notes_for(ada_id).len(), 1);
    }

    #[tokio::test]
    async fn waypoints_survive_reopen_and_stay_with_their_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        let ada = crate::state::starter_operator(ada_id, Uuid::new_v4(), "ada", "ada");
        let bob = crate::state::starter_operator(bob_id, Uuid::new_v4(), "bob", "bob");
        store.create_account(ada.account_id, "ada", "hash", &ada, 1).await.unwrap();
        store.create_account(bob.account_id, "bob", "hash", &bob, 2).await.unwrap();
        let loaded = store.load().await.unwrap();
        let mut world = WorldState::for_tests();
        world.install_loaded(loaded.operators, loaded.log);
        world.install_geography(loaded.geography);
        world.install_social(loaded.social);
        world.install_settlements(loaded.settlements);
        world.install_waypoints(loaded.waypoints);
        let ada_ident = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let bob_ident = SessionIdentity::new(bob_id, "bob", Permissions::player());
        let ada_session = ada_ident.session_id;
        let bob_session = bob_ident.session_id;
        let mut ops = world.connect_player(ada_ident).unwrap().0.ops;
        ops.extend(world.connect_player(bob_ident).unwrap().0.ops);
        store.commit(&ops).await.unwrap();

        let mut ops = world.apply(ada_session, "waypoint add shop-door").unwrap().ops;
        ops.extend(
            world
                .apply(ada_session, "waypoint note shop-door The counter faces the alley.")
                .unwrap()
                .ops,
        );
        ops.extend(world.apply(bob_session, "go up").unwrap().ops);
        ops.extend(world.apply(bob_session, "waypoint add shop-door").unwrap().ops);
        store.commit(&ops).await.unwrap();
        sqlx::query(
            "INSERT INTO waypoints (
                id, operator_id, name_key, display_name, place_key, note, created_at, updated_at
             ) VALUES (?1, ?2, 'old-epoch', 'old-epoch', 'w:undernet:9:0:0:1:4:4', 'keep me', 3, 3)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(ada_id.to_string())
        .execute(&store.pool)
        .await
        .unwrap();

        drop(store);
        let store = Store::open(&path).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.waypoints.len(), 3);
        let ada_mark = loaded
            .waypoints
            .iter()
            .find(|row| row.operator_id == ada_id && row.name_key == "shop-door")
            .unwrap();
        assert_eq!(ada_mark.place_key, "coffee-shop");
        assert_eq!(ada_mark.display_name, "shop-door");
        assert_eq!(ada_mark.note, "The counter faces the alley.");
        let bob_mark = loaded
            .waypoints
            .iter()
            .find(|row| row.operator_id == bob_id && row.name_key == "shop-door")
            .unwrap();
        assert_eq!(bob_mark.place_key, "roof-array");
        assert!(bob_mark.note.is_empty());
        let stale = loaded.waypoints.iter().find(|row| row.name_key == "old-epoch").unwrap();
        assert_eq!(stale.operator_id, ada_id);
        assert_eq!(stale.place_key, "w:undernet:9:0:0:1:4:4");
        assert_eq!(stale.note, "keep me");

        let mut again = WorldState::for_tests();
        again.install_loaded(loaded.operators, loaded.log);
        again.install_geography(loaded.geography);
        again.install_social(loaded.social);
        again.install_settlements(loaded.settlements);
        again.install_waypoints(loaded.waypoints);
        let back = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let session = back.session_id;
        again.connect_player(back).unwrap();
        again.apply(session, "waypoint show shop-door").unwrap();
        let page = again.view(session).unwrap().overlay.unwrap_or_default();
        assert!(page.contains("The counter faces the alley."), "{page}");
        assert!(page.contains("coffee-shop"), "{page}");
        again.apply(session, "waypoint show old-epoch").unwrap();
        let stale_page = again.view(session).unwrap().overlay.unwrap_or_default();
        assert!(stale_page.contains("not moved"), "{stale_page}");
        assert!(stale_page.contains("keep me"), "{stale_page}");
        assert_eq!(
            again.waypoints.iter().find(|row| row.name_key == "old-epoch").unwrap().place_key,
            "w:undernet:9:0:0:1:4:4"
        );
        let bob_back = SessionIdentity::new(bob_id, "bob", Permissions::player());
        let bob_session = bob_back.session_id;
        again.connect_player(bob_back).unwrap();
        let hidden = again.apply(bob_session, "waypoint show shop-door").unwrap();
        let bob_page = again.view(bob_session).unwrap().overlay.unwrap_or_default();
        assert!(bob_page.contains("roof-array"), "{bob_page}");
        assert!(!bob_page.contains("faces the alley"), "{bob_page}");
        assert!(hidden.ops.is_empty());
    }

    #[tokio::test]
    async fn duplicate_waypoint_names_do_not_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        let ada_id = PlayerId::new();
        let ada = crate::state::starter_operator(ada_id, Uuid::new_v4(), "ada", "ada");
        store.create_account(ada.account_id, "ada", "hash", &ada, 1).await.unwrap();
        let row = |id: Uuid| PersistOp::WaypointInsert {
            id,
            operator_id: ada_id,
            name_key: "shop-door".into(),
            display_name: "shop-door".into(),
            place_key: "coffee-shop".into(),
            note: String::new(),
            created_at: 1,
            updated_at: 1,
        };
        let err = store.commit(&[row(Uuid::new_v4()), row(Uuid::new_v4())]).await;
        assert!(err.is_err());
        let loaded = store.load().await.unwrap();
        assert!(loaded.waypoints.is_empty());
    }

    #[tokio::test]
    async fn mail_survives_reopen_and_a_failed_write_keeps_the_old_mailbox() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        let ada = crate::state::starter_operator(ada_id, Uuid::new_v4(), "ada", "Ada");
        let bob = crate::state::starter_operator(bob_id, Uuid::new_v4(), "bob", "Robert");
        store.create_account(ada.account_id, "ada", "hash", &ada, 1).await.unwrap();
        store.create_account(bob.account_id, "bob", "hash", &bob, 2).await.unwrap();

        let loaded = store.load().await.unwrap();
        let mut world = WorldState::for_tests();
        world.install_loaded(loaded.operators, loaded.log);
        world.install_mail(loaded.mailbox);
        let ada_ident = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let ada_session = ada_ident.session_id;
        store.commit(&world.connect_player(ada_ident).unwrap().0.ops).await.unwrap();
        let token = "zinc-token-441";
        let ops = world
            .apply(ada_session, &format!("mail send bob The dish still faces the gate. {token}"))
            .unwrap()
            .ops;
        assert!(store.commit(&ops).await.is_ok());
        let leaked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE body LIKE ?1")
            .bind(format!("%{token}%"))
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(leaked, 0);

        drop(store);
        let store = Store::open(&path).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.mailbox.messages.len(), 1);
        assert_eq!(loaded.mailbox.messages[0].recipient_id, bob_id);
        assert_eq!(loaded.mailbox.messages[0].sender_id, ada_id);
        assert!(loaded.mailbox.messages[0].read_at.is_none());
        assert!(loaded.mailbox.messages[0].body.contains(token));
        assert_eq!(loaded.mailbox.next_id, 2);

        let mut world = WorldState::for_tests();
        world.install_loaded(loaded.operators, loaded.log);
        world.install_mail(loaded.mailbox);
        let bob_ident = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        let bob_session = bob_ident.session_id;
        world.connect_player(bob_ident).unwrap();
        let read_ops = world.apply(bob_session, "mail read 1").unwrap().ops;
        store.commit(&read_ops).await.unwrap();
        let read_back = store.load().await.unwrap();
        assert!(read_back.mailbox.messages[0].read_at.is_some());
        assert!(read_back.mailbox.messages[0].body.contains(token));
        let delete_missing =
            store.commit(&[PersistOp::MailDelete { id: 1, recipient_id: ada_id }]).await;
        assert!(delete_missing.is_ok());
        let still: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM mail WHERE id = 1 AND recipient_id = ?1")
                .bind(bob_id.to_string())
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(still, 1, "a sender cannot delete the recipient's copy");

        let delete_ops = world.apply(bob_session, "mail delete 1").unwrap().ops;
        store.commit(&delete_ops).await.unwrap();
        let after = store.load().await.unwrap();
        assert!(after.mailbox.messages.is_empty());
        assert_eq!(after.mailbox.next_id, 2);

        let ghost = PlayerId::new();
        let failed = store
            .commit(&[PersistOp::MailInsert {
                id: 2,
                sender_id: ada_id,
                recipient_id: ghost,
                body: "should roll back".into(),
                created_at: 9,
            }])
            .await;
        assert!(failed.is_err());
        let kept = store.load().await.unwrap();
        assert!(kept.mailbox.messages.is_empty());
        assert_eq!(kept.mailbox.next_id, 2);
        let seq: i64 = sqlx::query_scalar("SELECT next_id FROM mail_seq WHERE singleton = 1")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(seq, 2);
    }

    #[tokio::test]
    async fn shared_theories_survive_reopen_and_a_failed_write_keeps_the_old_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coffee.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        let ada = crate::state::starter_operator(ada_id, Uuid::new_v4(), "ada", "Ada");
        let bob = crate::state::starter_operator(bob_id, Uuid::new_v4(), "bob", "Robert");
        store.create_account(ada.account_id, "ada", "hash", &ada, 1).await.unwrap();
        store.create_account(bob.account_id, "bob", "hash", &bob, 2).await.unwrap();

        let loaded = store.load().await.unwrap();
        assert!(loaded.theories.entries.is_empty());
        assert_eq!(loaded.theories.next_id, 1);
        let mut world = WorldState::for_tests();
        world.install_loaded(loaded.operators, loaded.log);
        world.install_theories(loaded.theories);
        let ada_ident = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let ada_session = ada_ident.session_id;
        let bob_ident = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        store.commit(&world.connect_player(ada_ident).unwrap().0.ops).await.unwrap();
        store.commit(&world.connect_player(bob_ident).unwrap().0.ops).await.unwrap();

        let token = "panel-token-441";
        world.now_ms = world.now_ms.saturating_add(2_000);
        let first = world
            .apply(
                ada_session,
                &format!("theory post observation The panel still shows B-4. {token}"),
            )
            .unwrap()
            .ops;
        store.commit(&first).await.unwrap();
        world.now_ms = world.now_ms.saturating_add(2_000);
        let second = world
            .apply(ada_session, "theory post theory The lock may still be with the relay.")
            .unwrap()
            .ops;
        store.commit(&second).await.unwrap();
        let leaked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE body LIKE ?1")
            .bind(format!("%{token}%"))
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(leaked, 0);
        let notices: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM noticeboard_posts")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(notices, 0);

        drop(store);
        let store = Store::open(&path).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.theories.entries.len(), 2);
        assert_eq!(loaded.theories.next_id, 3);
        let observation = loaded.theories.entries.iter().find(|entry| entry.id == 1).unwrap();
        assert_eq!(observation.author_id, ada_id);
        assert_eq!(observation.kind, TheoryKind::Observation);
        assert_eq!(observation.topic, "delisted-floor");
        assert!(observation.body.contains(token));
        let theory = loaded.theories.entries.iter().find(|entry| entry.id == 2).unwrap();
        assert_eq!(theory.kind, TheoryKind::Theory);
        assert_eq!(theory.author_id, ada_id);

        let mut world = WorldState::for_tests();
        world.install_loaded(loaded.operators, loaded.log);
        world.install_theories(loaded.theories);
        let bob_ident = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        let bob_session = bob_ident.session_id;
        world.connect_player(bob_ident).unwrap();
        world.apply(bob_session, "theory read 1").unwrap();
        let page = world.view(bob_session).unwrap().overlay.unwrap_or_default();
        assert!(page.contains("Ada"));
        assert!(page.contains("observation"));
        assert!(page.contains(token));
        let stolen = store.commit(&[PersistOp::TheoryDelete { id: 1, author_id: bob_id }]).await;
        assert!(stolen.is_ok());
        let still: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM shared_theories WHERE id = 1 AND author_id = ?1",
        )
        .bind(ada_id.to_string())
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(still, 1, "another operator cannot delete the row");

        let ada_ident = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let ada_session = ada_ident.session_id;
        world.connect_player(ada_ident).unwrap();
        let delete_ops = world.apply(ada_session, "theory remove 1").unwrap().ops;
        store.commit(&delete_ops).await.unwrap();
        let after = store.load().await.unwrap();
        assert!(after.theories.get("delisted-floor", 1).is_none());
        assert!(after.theories.get("delisted-floor", 2).is_some());
        assert_eq!(after.theories.next_id, 3);

        let ghost = PlayerId::new();
        let failed = store
            .commit(&[PersistOp::TheoryInsert {
                id: 3,
                topic: "delisted-floor".into(),
                author_id: ghost,
                kind: TheoryKind::Theory,
                body: "should roll back".into(),
                created_at: 9,
            }])
            .await;
        assert!(failed.is_err());
        let kept = store.load().await.unwrap();
        assert_eq!(kept.theories.entries.len(), 1);
        assert_eq!(kept.theories.next_id, 3);
        assert!(kept.theories.entries[0].body.contains("relay"));
        let seq: i64 =
            sqlx::query_scalar("SELECT next_id FROM shared_theory_seq WHERE singleton = 1")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(seq, 3);
    }

    #[tokio::test]
    async fn personal_homes_survive_reopen_and_unique_active_indexes_hold() {
        use crate::geography::{survey_position, WildernessPos};
        use crate::home::{FORGOTTEN_MS, RECENTLY_ABANDONED_MS};
        use terminus_core::{Permissions, SessionIdentity};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("homes.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let ada_id = PlayerId::new();
        let bob_id = PlayerId::new();
        let ada = crate::state::starter_operator(ada_id, Uuid::new_v4(), "ada", "Ada");
        let bob = crate::state::starter_operator(bob_id, Uuid::new_v4(), "bob", "Robert");
        store.create_account(ada.account_id, "ada", "hash", &ada, 1).await.unwrap();
        store.create_account(bob.account_id, "bob", "hash", &bob, 2).await.unwrap();

        let loaded = store.load().await.unwrap();
        assert!(loaded.homes.homes.is_empty());
        let mut world = WorldState::for_tests();
        world.install_loaded(loaded.operators, loaded.log);
        world.install_geography(loaded.geography);
        world.install_homes(loaded.homes);
        let ada_ident = SessionIdentity::new(ada_id, "Ada", Permissions::player());
        let ada_session = ada_ident.session_id;
        store.commit(&world.connect_player(ada_ident).unwrap().0.ops).await.unwrap();
        let bob_ident = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        store.commit(&world.connect_player(bob_ident).unwrap().0.ops).await.unwrap();

        let first = survey_position();
        let _ = world
            .ensure_region(first.identity, Some(ada_id), first.local_x, first.local_y)
            .unwrap();
        world.operators.get_mut(&ada_id).unwrap().location_id = first.key();
        let claim = world.apply(ada_session, "home claim").unwrap().ops;
        store.commit(&claim).await.unwrap();

        let mut second = first;
        second.local_x = 6;
        let _ = world
            .ensure_region(second.identity, Some(ada_id), second.local_x, second.local_y)
            .unwrap();
        world.operators.get_mut(&ada_id).unwrap().location_id = second.key();
        let moved = world.apply(ada_session, "home move confirm").unwrap().ops;
        store.commit(&moved).await.unwrap();

        // A second active claim for the same operator must fail the unique index.
        let dup = store
            .commit(&[PersistOp::HomeClaim {
                id: Uuid::new_v4(),
                operator_id: ada_id,
                place_key: first.key(),
                claimed_at: 99,
                public_name: String::new(),
                public_description: String::new(),
                meaningful_returns: 0,
                last_departed_at: None,
                last_returned_at: None,
            }])
            .await;
        assert!(dup.is_err());

        drop(store);
        let store = Store::open(&path).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.homes.homes.len(), 2);
        assert_eq!(loaded.homes.active_for(ada_id).unwrap().place_key, second.key());
        let abandoned_at = loaded
            .homes
            .homes
            .iter()
            .find(|h| h.place_key == first.key())
            .and_then(|h| h.abandoned_at)
            .expect("old home abandoned");

        let mut again = WorldState::for_tests();
        again.install_loaded(loaded.operators, loaded.log);
        again.install_geography(loaded.geography);
        again.install_homes(loaded.homes);
        again.now_ms = abandoned_at + RECENTLY_ABANDONED_MS;
        let scene =
            again.wild_scene_for_tests(WildernessPos::parse(&first.key()).unwrap(), Some(bob_id));
        assert!(scene.contains("Dust covers"), "{scene}");
        again.now_ms = abandoned_at + FORGOTTEN_MS;
        let absorbed =
            again.wild_scene_for_tests(WildernessPos::parse(&first.key()).unwrap(), Some(bob_id));
        assert!(!absorbed.contains("Dust covers"), "{absorbed}");
        assert!(!absorbed.contains("inhabited until recently"), "{absorbed}");

        // Bob may reclaim the abandoned place; Ada's active home elsewhere stays.
        let bob_again = SessionIdentity::new(bob_id, "Robert", Permissions::player());
        let bob_session = bob_again.session_id;
        store.commit(&again.connect_player(bob_again).unwrap().0.ops).await.unwrap();
        again.operators.get_mut(&bob_id).unwrap().location_id = first.key();
        let bob_claim = again.apply(bob_session, "home claim").unwrap().ops;
        store.commit(&bob_claim).await.unwrap();
        assert_eq!(again.homes.active_for(bob_id).unwrap().place_key, first.key());
        assert_eq!(again.homes.active_for(ada_id).unwrap().place_key, second.key());
    }

    #[tokio::test]
    async fn remote_world_acceptance_persists_without_intervening_regions() {
        use crate::geography::{
            describe_place, generate_region, landmark_sight, DensityBand, UrbanContext,
            WildernessPos, WorldId, GENERATION_V1, REGION_SIZE, REMOTE_ACCEPTANCE_ORIGIN_X,
            REMOTE_ACCEPTANCE_ORIGIN_Y,
        };
        use terminus_core::{Permissions, SessionIdentity};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("remote.sqlite");
        let store = Store::open(&path).await.unwrap();
        store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();
        let ada_id = PlayerId::new();
        let ada = crate::state::starter_operator(ada_id, Uuid::new_v4(), "ada", "ada");
        store.create_account(ada.account_id, "ada", "hash", &ada, 1).await.unwrap();

        let loaded = store.load().await.unwrap();
        let mut world = WorldState::for_tests();
        world.install_loaded(loaded.operators, loaded.log);
        world.install_geography(loaded.geography);
        world.install_homes(loaded.homes);
        world.install_waypoints(loaded.waypoints);
        let ada_ident = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let ada_session = ada_ident.session_id;
        store.commit(&world.connect_player(ada_ident).unwrap().0.ops).await.unwrap();

        let identity = crate::geography::RegionIdentity {
            world: WorldId::Undernet,
            epoch_id: 1,
            origin_x: REMOTE_ACCEPTANCE_ORIGIN_X,
            origin_y: REMOTE_ACCEPTANCE_ORIGIN_Y,
            generation_version: GENERATION_V1,
        };
        assert_eq!(i64::from(REMOTE_ACCEPTANCE_ORIGIN_X) * i64::from(REGION_SIZE), 992);
        let generated = generate_region(identity).unwrap();
        let mut home_pos = None;
        let mut landmark_pos = None;
        for local_y in 0..16u8 {
            for local_x in 0..16u8 {
                let pos = WildernessPos { identity, local_x, local_y };
                let baseline = generated.tile(local_x, local_y).unwrap();
                let tile = world.effective_at(pos).unwrap();
                if !tile.traversable() {
                    continue;
                }
                if home_pos.is_none() {
                    home_pos = Some(pos);
                }
                if landmark_pos.is_none() {
                    if let Some(kind) = baseline.landmark {
                        if landmark_sight(pos, kind).finding.is_some() {
                            landmark_pos = Some(pos);
                        }
                    }
                }
            }
        }
        let home_pos = home_pos.expect("walkable remote tile");
        let ctx = UrbanContext::at(home_pos);
        assert_eq!(ctx.band, DensityBand::Deep);
        assert!(ctx.familiarity().contains("deep Squalor"));

        // Materialize only the remote birth cell — not the path from the Hub.
        let mut ops = world
            .ensure_region(home_pos.identity, Some(ada_id), home_pos.local_x, home_pos.local_y)
            .unwrap();
        assert_eq!(world.geography.regions.len(), 1);
        assert!(world.geography.region(identity.coord_key()).is_some());
        assert!(world
            .geography
            .regions
            .keys()
            .all(|key| key.origin_x == REMOTE_ACCEPTANCE_ORIGIN_X
                && key.origin_y == REMOTE_ACCEPTANCE_ORIGIN_Y));

        world.operators.get_mut(&ada_id).unwrap().location_id = home_pos.key();
        ops.extend(world.apply(ada_session, "home claim").unwrap().ops);
        ops.extend(world.apply(ada_session, "waypoint add far-home").unwrap().ops);
        ops.extend(
            world
                .apply(ada_session, "waypoint note far-home Deep stacks beyond the Hub's maps.")
                .unwrap()
                .ops,
        );

        if let Some(mark) = landmark_pos {
            world.operators.get_mut(&ada_id).unwrap().location_id = mark.key();
            let _ = world
                .ensure_region(mark.identity, Some(ada_id), mark.local_x, mark.local_y)
                .unwrap();
            let kind = generated.tile(mark.local_x, mark.local_y).unwrap().landmark.unwrap();
            world.apply(ada_session, &format!("inspect {}", kind.keyword())).unwrap();
            ops.extend(world.apply(ada_session, "journal record").unwrap().ops);
        }

        // Looking at a remote place does not grant knowledge of an unvisited neighbour.
        let neighbour = crate::geography::RegionIdentity {
            origin_x: REMOTE_ACCEPTANCE_ORIGIN_X + 1,
            ..identity
        };
        assert!(world.geography.region(neighbour.coord_key()).is_none());

        store.commit(&ops).await.unwrap();
        drop(store);

        let store = Store::open(&path).await.unwrap();
        let loaded = store.load().await.unwrap();
        assert_eq!(loaded.geography.regions.len(), 1);
        assert_eq!(
            loaded.geography.regions.values().next().unwrap().identity.origin_x,
            REMOTE_ACCEPTANCE_ORIGIN_X
        );
        assert_eq!(loaded.homes.active_for(ada_id).unwrap().place_key, home_pos.key());
        let mark = loaded
            .waypoints
            .iter()
            .find(|row| row.operator_id == ada_id && row.name_key == "far-home")
            .unwrap();
        assert_eq!(mark.place_key, home_pos.key());
        assert!(mark.note.contains("Deep stacks"));
        let (label, hint) = describe_place(&home_pos.key());
        assert!(label.contains("deep Squalor"), "{label}");
        assert!(hint.contains("not a route"), "{hint}");

        let mut again = WorldState::for_tests();
        again.install_loaded(loaded.operators, loaded.log);
        again.install_geography(loaded.geography);
        again.install_homes(loaded.homes);
        again.install_waypoints(loaded.waypoints);
        let back = SessionIdentity::new(ada_id, "ada", Permissions::player());
        let session = back.session_id;
        again.connect_player(back).unwrap();
        again.apply(session, "home").unwrap();
        let home_page = again.view(session).unwrap().overlay.unwrap_or_default();
        assert!(home_page.contains("Your home"), "{home_page}");
        assert!(home_page.contains("deep Squalor"), "{home_page}");
        again.apply(session, "waypoint show far-home").unwrap();
        let way_page = again.view(session).unwrap().overlay.unwrap_or_default();
        assert!(way_page.contains("Deep stacks"), "{way_page}");
        assert!(way_page.contains("not a route"), "{way_page}");
        // Neighbour still unvisited after restart.
        assert!(again.geography.region(neighbour.coord_key()).is_none());
    }
}
