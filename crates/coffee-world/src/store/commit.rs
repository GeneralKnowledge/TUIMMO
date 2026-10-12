use super::rows::*;
use super::*;

impl Store {
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
}
