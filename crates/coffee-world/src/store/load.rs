use super::rows::*;
use super::*;

impl Store {
    pub(crate) async fn load_operators(&self) -> Result<Vec<Operator>, StoreError> {
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

    pub(crate) async fn load_log(&self) -> Result<Vec<LogEntry>, StoreError> {
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

    pub(crate) async fn load_geography(&self) -> Result<GeographyState, StoreError> {
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

    pub(crate) async fn load_waypoints(&self) -> Result<Vec<Waypoint>, StoreError> {
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

    pub(crate) async fn load_homes(&self) -> Result<HomeState, StoreError> {
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

    pub(crate) async fn load_mail(&self) -> Result<Mailbox, StoreError> {
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

    pub(crate) async fn load_clue_seen(&self) -> Result<Vec<crate::threads::ClueSeen>, StoreError> {
        let rows = sqlx::query_as::<_, ClueSeenSql>(
            "SELECT operator_id, clue_id, first_seen_at FROM clue_seen",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut seen = Vec::with_capacity(rows.len());
        for row in rows {
            seen.push(crate::threads::ClueSeen {
                operator_id: PlayerId::from_str(&row.operator_id).map_err(|_| StoreError::Data)?,
                clue_id: row.clue_id,
                first_seen_at: u64_col(row.first_seen_at)?,
            });
        }
        Ok(seen)
    }

    pub(crate) async fn load_theories(&self) -> Result<TheoryBoard, StoreError> {
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

    pub(crate) async fn load_invites(&self) -> Result<InviteBook, StoreError> {
        let rows = sqlx::query_as::<_, AccountInviteSql>(
            "SELECT id, code_key, display_code, created_by, created_at, note, redeemed_by, redeemed_at
             FROM account_invites",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut codes = Vec::with_capacity(rows.len());
        for row in rows {
            codes.push(AccountInvite {
                id: Uuid::from_str(&row.id).map_err(|_| StoreError::Data)?,
                code_key: row.code_key,
                display_code: row.display_code,
                created_by: optional_player(row.created_by)?,
                created_at: u64_col(row.created_at)?,
                note: row.note,
                redeemed_by: optional_player(row.redeemed_by)?,
                redeemed_at: row.redeemed_at.map(u64_col).transpose()?,
            });
        }
        Ok(InviteBook { codes })
    }

    pub(crate) async fn load_social(&self) -> Result<SocialState, StoreError> {
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

    pub(crate) async fn load_settlements(&self) -> Result<SettlementState, StoreError> {
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
