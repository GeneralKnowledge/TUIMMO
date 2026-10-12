use super::*;

pub(crate) async fn insert_account_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: Uuid,
    username: &str,
    password_hash: &str,
    operator: &Operator,
    created_at: u64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO accounts (id, username, password_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(account_id.to_string())
    .bind(username)
    .bind(password_hash)
    .bind(created_at as i64)
    .execute(&mut **tx)
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
    .execute(&mut **tx)
    .await?;
    for item in &operator.items {
        sqlx::query(
            "INSERT INTO operator_items (operator_id, item_key, name, detail) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(operator.id.to_string())
        .bind(&item.keyword)
        .bind(&item.name)
        .bind(&item.detail)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub(crate) async fn insert_invite(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    row: &AccountInvite,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO account_invites (
            id, code_key, display_code, created_by, created_at, note, redeemed_by, redeemed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(row.id.to_string())
    .bind(&row.code_key)
    .bind(&row.display_code)
    .bind(row.created_by.map(|id| id.to_string()))
    .bind(row.created_at as i64)
    .bind(&row.note)
    .bind(row.redeemed_by.map(|id| id.to_string()))
    .bind(row.redeemed_at.map(|at| at as i64))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) fn is_unique_constraint(err: &StoreError) -> bool {
    let mut current: Option<&dyn std::error::Error> = Some(err);
    while let Some(err) = current {
        let text = err.to_string();
        if text.contains("UNIQUE") || text.contains("unique") {
            return true;
        }
        current = err.source();
    }
    false
}

pub(crate) fn mail_id(id: u64) -> Result<i64, StoreError> {
    i64::try_from(id).map_err(|_| StoreError::Data)
}

pub(crate) fn i32_col(value: i64) -> Result<i32, StoreError> {
    i32::try_from(value).map_err(|_| StoreError::Data)
}

pub(crate) fn u64_col(value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| StoreError::Data)
}

pub(crate) fn u32_col(value: i64) -> Result<u32, StoreError> {
    u32::try_from(value).map_err(|_| StoreError::Data)
}

pub(crate) fn local_col(value: i64) -> Result<u8, StoreError> {
    let local = u8::try_from(value).map_err(|_| StoreError::Data)?;
    if u32::from(local) >= u32::try_from(crate::geography::REGION_SIZE).unwrap_or(16) {
        return Err(StoreError::Data);
    }
    Ok(local)
}

pub(crate) fn optional_player(value: Option<String>) -> Result<Option<PlayerId>, StoreError> {
    value.map(|raw| PlayerId::from_str(&raw).map_err(|_| StoreError::Data)).transpose()
}

pub(crate) fn optional_uuid(value: Option<String>) -> Result<Option<Uuid>, StoreError> {
    value.map(|raw| Uuid::from_str(&raw).map_err(|_| StoreError::Data)).transpose()
}

pub(crate) fn blank_to_none(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

/// Fields needed to insert a new account row set.
pub struct NewAccount<'a> {
    pub account_id: Uuid,
    pub username: &'a str,
    pub password_hash: &'a str,
    pub operator: &'a Operator,
    pub created_at: u64,
}

/// Invite gate applied while creating an account.
pub struct AccountInvitePolicy<'a> {
    pub code: Option<&'a str>,
    pub require_code: bool,
    pub max_accounts: u32,
    pub starter_invites: u32,
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
pub(crate) struct AccountSql {
    pub(crate) id: String,
    pub(crate) username: String,
    pub(crate) password_hash: String,
    pub(crate) operator_id: String,
    pub(crate) name: String,
    pub(crate) location_id: String,
}

impl AccountSql {
    pub(crate) fn into_row(self) -> Result<AccountRow, StoreError> {
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
pub(crate) struct OpSql {
    pub(crate) id: String,
    pub(crate) account_id: String,
    pub(crate) username: String,
    pub(crate) name: String,
    pub(crate) location_id: String,
    pub(crate) bio: String,
    pub(crate) status: String,
}

#[derive(sqlx::FromRow)]
pub(crate) struct ItemSql {
    pub(crate) item_key: String,
    pub(crate) name: String,
    pub(crate) detail: String,
}

#[derive(sqlx::FromRow)]
pub(crate) struct MsgSql {
    pub(crate) at_ms: i64,
    pub(crate) channel: String,
    pub(crate) audience: String,
    pub(crate) audience_key: String,
    pub(crate) speaker: Option<String>,
    pub(crate) body: String,
}

#[derive(sqlx::FromRow)]
pub(crate) struct RegionSql {
    pub(crate) id: String,
    pub(crate) world_id: String,
    pub(crate) epoch_id: i64,
    pub(crate) origin_x: i64,
    pub(crate) origin_y: i64,
    pub(crate) generation_version: i64,
    pub(crate) region_seed: String,
    pub(crate) placement_x: i64,
    pub(crate) placement_y: i64,
    pub(crate) discovered_at: i64,
}

impl RegionSql {
    pub(crate) fn identity(&self) -> Result<RegionIdentity, StoreError> {
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
pub(crate) struct StructureSql {
    pub(crate) id: String,
    pub(crate) region_id: String,
    pub(crate) local_x: i64,
    pub(crate) local_y: i64,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) owner_id: Option<String>,
    pub(crate) access: String,
    pub(crate) interior_place: Option<String>,
    pub(crate) settlement_id: Option<String>,
    pub(crate) share: String,
    pub(crate) created_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct SettlementSql {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) name_key: String,
    pub(crate) description: String,
    pub(crate) visibility: String,
    pub(crate) anchor_place: String,
    pub(crate) owner_id: String,
    pub(crate) created_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct SettlementMemberSql {
    pub(crate) settlement_id: String,
    pub(crate) operator_id: String,
    pub(crate) role: String,
    pub(crate) joined_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct SettlementInviteSql {
    pub(crate) id: String,
    pub(crate) settlement_id: String,
    pub(crate) inviter_id: String,
    pub(crate) invitee_id: String,
    pub(crate) status: String,
    pub(crate) created_at: i64,
    pub(crate) expires_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct FriendshipSql {
    pub(crate) requester_id: String,
    pub(crate) recipient_id: String,
    pub(crate) status: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct IgnoreSql {
    pub(crate) operator_id: String,
    pub(crate) ignored_id: String,
    pub(crate) created_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct GrantSql {
    pub(crate) structure_id: String,
    pub(crate) grantee_id: String,
    pub(crate) created_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct NoticeSql {
    pub(crate) id: String,
    pub(crate) place_id: String,
    pub(crate) author_id: String,
    pub(crate) body: String,
    pub(crate) created_at: i64,
    pub(crate) expires_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct ModSql {
    pub(crate) region_id: String,
    pub(crate) local_x: i64,
    pub(crate) local_y: i64,
    pub(crate) terrain: String,
    pub(crate) reason: String,
    pub(crate) actor_id: Option<String>,
    pub(crate) created_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct MailSql {
    pub(crate) id: i64,
    pub(crate) sender_id: String,
    pub(crate) recipient_id: String,
    pub(crate) body: String,
    pub(crate) created_at: i64,
    pub(crate) read_at: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct TheorySql {
    pub(crate) id: i64,
    pub(crate) topic: String,
    pub(crate) author_id: String,
    pub(crate) kind: String,
    pub(crate) body: String,
    pub(crate) created_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct AccountInviteSql {
    pub(crate) id: String,
    pub(crate) code_key: String,
    pub(crate) display_code: String,
    pub(crate) created_by: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) note: String,
    pub(crate) redeemed_by: Option<String>,
    pub(crate) redeemed_at: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct InviteOpenSql {
    pub(crate) redeemed_by: Option<String>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct WaypointSql {
    pub(crate) id: String,
    pub(crate) operator_id: String,
    pub(crate) name_key: String,
    pub(crate) display_name: String,
    pub(crate) place_key: String,
    pub(crate) note: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

#[derive(sqlx::FromRow)]
pub(crate) struct HomeSql {
    pub(crate) id: String,
    pub(crate) operator_id: String,
    pub(crate) place_key: String,
    pub(crate) claimed_at: i64,
    pub(crate) abandoned_at: Option<i64>,
    pub(crate) public_name: String,
    pub(crate) public_description: String,
    pub(crate) meaningful_returns: i64,
    pub(crate) last_departed_at: Option<i64>,
    pub(crate) last_returned_at: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct DiscoverySql {
    pub(crate) id: String,
    pub(crate) region_id: Option<String>,
    pub(crate) local_x: i64,
    pub(crate) local_y: i64,
    pub(crate) kind: String,
    pub(crate) note: String,
    pub(crate) actor_id: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) content_key: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) place_key: Option<String>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct LinkSql {
    pub(crate) id: String,
    pub(crate) from_place: String,
    pub(crate) keyword: String,
    pub(crate) to_place: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) created_by: Option<String>,
    pub(crate) created_at: i64,
}
