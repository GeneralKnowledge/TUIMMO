//! Thread clue inspection — surface text, reveals, and first-visit journal rows.

use uuid::Uuid;

use terminus_core::{GameError, PlayerId, SessionId};

use crate::geography::{Discovery, WildernessPos, FIELD_KIND};
use crate::model::PersistOp;
use crate::threads::{best_reveal_text, ClueSite, KnowledgeContext, USED_SUBWAY_FLAG};

use super::WorldState;

impl WorldState {
    /// If `target` matches a thread clue at the operator's place, build inspect text and side effects.
    pub(crate) fn try_inspect_thread_clue(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        location: &str,
        wild: Option<WildernessPos>,
        target: &str,
    ) -> Option<Result<(String, Vec<PersistOp>), GameError>> {
        if self.threads.clue_at_place_for_target_static(location, wild, target).is_none() {
            if let Some(pos) = wild {
                if let Err(err) = self.threads.ensure_procedural(pos.identity) {
                    return Some(Err(GameError::Rejected(crate::state::unreadable_wilderness(
                        &err,
                    ))));
                }
            }
        }
        let clue_id = self.threads.clue_at_place_for_target(location, wild, target)?;
        let clue = self.threads.clue_def(&clue_id)?;
        Some(self.finish_thread_inspect(session_id, player, location, wild, &clue))
    }

    fn finish_thread_inspect(
        &mut self,
        session_id: SessionId,
        player: PlayerId,
        location: &str,
        wild: Option<WildernessPos>,
        clue: &crate::threads::ClueDef,
    ) -> Result<(String, Vec<PersistOp>), GameError> {
        let mut ops = Vec::new();
        if let Some(pos) = wild {
            ops.extend(self.ensure_region(pos.identity, Some(player), pos.local_x, pos.local_y)?);
        }
        let ctx = self.knowledge_context(player);
        let mut text = clue.surface_text.clone();
        if let Some(reveal) = best_reveal_text(&clue.reveals, &ctx) {
            text.push('\n');
            text.push_str(&reveal);
        }
        if !self.threads.has_seen(player, &clue.id) {
            let created_at = self.now_ms;
            self.threads.seen.push(crate::threads::ClueSeen {
                operator_id: player,
                clue_id: clue.id.clone(),
                first_seen_at: created_at,
            });
            ops.push(PersistOp::ClueSeen {
                operator_id: player,
                clue_id: clue.id.clone(),
                first_seen_at: created_at,
            });
            if self.geography.field_note(player, &clue.id).is_none() {
                ops.extend(self.auto_journal_clue(player, location, wild, clue, created_at)?);
            }
        }
        let _ = session_id;
        Ok((text, ops))
    }

    fn auto_journal_clue(
        &mut self,
        player: PlayerId,
        location: &str,
        wild: Option<WildernessPos>,
        clue: &crate::threads::ClueDef,
        created_at: u64,
    ) -> Result<Vec<PersistOp>, GameError> {
        let (region_id, local_x, local_y, place_key) = if let Some(pos) = wild {
            let region_id = self
                .geography
                .region(pos.identity.coord_key())
                .map(|region| region.id)
                .ok_or_else(|| GameError::Rejected("This region is not open.".into()))?;
            (Some(region_id), pos.local_x, pos.local_y, pos.key())
        } else {
            (None, 0, 0, location.to_string())
        };
        let id = Uuid::new_v4();
        self.geography.discoveries.push(Discovery {
            id,
            region_id,
            local_x,
            local_y,
            kind: FIELD_KIND.into(),
            note: clue.surface_text.clone(),
            actor_id: Some(player),
            created_at,
            content_key: Some(clue.id.clone()),
            title: Some(clue.journal_title.clone()),
            place_key: Some(place_key.clone()),
        });
        Ok(vec![PersistOp::Discovery {
            id,
            region_id,
            local_x,
            local_y,
            kind: FIELD_KIND.into(),
            note: clue.surface_text.clone(),
            actor_id: Some(player),
            created_at,
            content_key: Some(clue.id.clone()),
            title: Some(clue.journal_title.clone()),
            place_key: Some(place_key),
        }])
    }

    pub(crate) fn knowledge_context(&self, player: PlayerId) -> KnowledgeContext<'_> {
        let rows: Vec<(PlayerId, String, ClueSite)> = self
            .threads
            .seen
            .iter()
            .filter_map(|row| {
                let site = self.threads.site_for_clue(&row.clue_id)?.clone();
                Some((row.operator_id, row.clue_id.clone(), site))
            })
            .collect();
        let theory_authors = self.simulation_theory_authors_by_topic();
        let used_subway =
            self.operators.get(&player).is_some_and(|op| op.flags.contains(USED_SUBWAY_FLAG));
        let tags_for = |id: &str| self.threads.clue_def(id).map(|clue| clue.tags.clone());
        KnowledgeContext::build(
            &self.threads.catalog,
            player,
            &rows,
            &theory_authors,
            used_subway,
            tags_for,
        )
    }

    /// Look-line object for a thread clue on this tile, if any.
    pub(crate) fn thread_look_objects(
        &self,
        location: &str,
        wild: Option<WildernessPos>,
    ) -> Vec<(String, String)> {
        if let Some(pos) = wild {
            let _ = self.threads.ensure_procedural(pos.identity);
        }
        self.threads
            .clue_ids_at_place(location, wild)
            .into_iter()
            .filter_map(|clue_id| {
                let clue = self.threads.clue_def(&clue_id)?;
                Some((clue.kind.keyword().to_string(), clue.kind.label().to_string()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::frontier_epoch;
    use crate::state::WorldState;
    use crate::threads::{load_default_threads, ThreadState};
    use terminus_core::PlayerId;

    #[test]
    fn reveal_prefers_more_specific_requirement() {
        let mut world = WorldState::for_tests();
        let catalog = load_default_threads();
        world.threads = ThreadState::new(catalog, frontier_epoch().world_seed);
        let player = PlayerId::new();
        let clue = world.threads.catalog.clue("tally-delisted-placard").unwrap().clone();
        world.threads.seen.push(crate::threads::ClueSeen {
            operator_id: player,
            clue_id: "subway-stair-clue".into(),
            first_seen_at: 1,
        });
        for id in ["tally-shutter-graffiti", "tally-ledger-margin", "tally-delisted-placard"] {
            world.threads.seen.push(crate::threads::ClueSeen {
                operator_id: player,
                clue_id: id.into(),
                first_seen_at: 1,
            });
        }
        let ctx = world.knowledge_context(player);
        let text = best_reveal_text(&clue.reveals, &ctx).unwrap();
        assert!(text.contains("fresh scratch"));
    }
}
