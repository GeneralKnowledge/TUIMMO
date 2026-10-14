//! Hidden clue threads scattered across the Squalor and Hub.

/// Set when this operator has entered the municipal subway (station platform).
pub const USED_SUBWAY_FLAG: &str = "used-subway";

mod knowledge;
mod load;
mod placement;
mod sim_report;

pub use knowledge::{best_reveal_text, thread_final_clue_id, Knowledge, KnowledgeContext};
pub use load::{load_default_threads, load_threads_from_str, LoadError};
pub use placement::{
    build_placement_map, clue_at_static, clue_at_wild_pos, placement_hash, resolve_all_sites,
};
pub use sim_report::{simulation_thread_reports, ThreadSimReport};

use std::collections::HashMap;

use crate::geography::{DensityBand, DistrictKind, WildernessPos};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClueCatalog {
    pub threads: Vec<ThreadDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadDef {
    pub id: String,
    pub tags: Vec<String>,
    pub clues: Vec<ClueDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClueDef {
    pub id: String,
    pub kind: ClueKind,
    pub surface_text: String,
    pub tags: Vec<String>,
    pub journal_title: String,
    pub site: SiteRule,
    pub reveals: Vec<RevealDef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClueKind {
    Graffiti,
    Object,
    Record,
    Sign,
}

impl ClueKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Graffiti => "graffiti",
            Self::Object => "object",
            Self::Record => "record",
            Self::Sign => "sign",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Graffiti => "wall mark",
            Self::Record => "ledger margin",
            Self::Object => "discarded object",
            Self::Sign => "handwritten sign",
        }
    }

    pub fn matches_target(self, target: &str) -> bool {
        let t = target.to_ascii_lowercase();
        if t == self.keyword() || t == self.label() {
            return true;
        }
        match self {
            Self::Graffiti => {
                matches!(
                    t.as_str(),
                    "mark"
                        | "writing"
                        | "scratch"
                        | "shutter"
                        | "circle"
                        | "chalk"
                        | "stain"
                        | "line"
                        | "water"
                        | "wall"
                        | "wear"
                        | "groove"
                )
            }
            Self::Object => {
                matches!(
                    t.as_str(),
                    "cup"
                        | "tin"
                        | "dent"
                        | "locker"
                        | "groove"
                        | "floor"
                        | "threshold"
                        | "stub"
                        | "ticket"
                        | "map"
                )
            }
            Self::Record => {
                matches!(t.as_str(), "margin" | "ledger" | "strokes" | "column" | "map")
            }
            Self::Sign => {
                matches!(
                    t.as_str(),
                    "note"
                        | "placard"
                        | "notice"
                        | "letter"
                        | "doorbell"
                        | "label"
                        | "board"
                        | "noticeboard"
                        | "timetable"
                        | "schedule"
                        | "scratch"
                        | "scratched"
                        | "map"
                )
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SiteRule {
    Wild(WildSiteFilter),
    Static { location: String },
    MetroEntrance { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WildSiteFilter {
    pub band: DensityBand,
    pub district: Option<DistrictKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevealDef {
    pub text: String,
    pub requires: KnowledgeRequirement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnowledgeRequirement {
    SeenTag { tag: String, distinct_sites: u32 },
    SeenClue(String),
    CorroboratedTheory { subject: String, min: u32 },
    UsedSubway,
    All(Vec<KnowledgeRequirement>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ClueSite {
    Wild(WildernessPos),
    Static(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClueSeen {
    pub operator_id: terminus_core::PlayerId,
    pub clue_id: String,
    pub first_seen_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadState {
    pub catalog: ClueCatalog,
    pub sites: HashMap<String, ClueSite>,
    pub seen: Vec<ClueSeen>,
}

impl ClueCatalog {
    pub fn clue(&self, id: &str) -> Option<&ClueDef> {
        self.clues().find(|clue| clue.id == id)
    }

    pub fn clues(&self) -> impl Iterator<Item = &ClueDef> {
        self.threads.iter().flat_map(|thread| thread.clues.iter())
    }

    pub fn thread_for_clue(&self, clue_id: &str) -> Option<&ThreadDef> {
        self.threads.iter().find(|thread| thread.clues.iter().any(|clue| clue.id == clue_id))
    }
}

#[cfg(test)]
mod acceptance;

impl ThreadState {
    pub fn new(catalog: ClueCatalog, world_seed: u64) -> Self {
        let sites = build_placement_map(&catalog, world_seed);
        Self { catalog, sites, seen: Vec::new() }
    }

    pub fn has_seen(&self, operator: terminus_core::PlayerId, clue_id: &str) -> bool {
        self.seen.iter().any(|row| row.operator_id == operator && row.clue_id == clue_id)
    }

    pub fn site_for_clue(&self, clue_id: &str) -> Option<&ClueSite> {
        self.sites.get(clue_id)
    }

    pub fn clue_at_place(&self, location: &str, wild: Option<WildernessPos>) -> Option<String> {
        self.clue_ids_at_place(location, wild).into_iter().next()
    }

    pub fn clue_ids_at_place(&self, location: &str, wild: Option<WildernessPos>) -> Vec<String> {
        self.sites
            .iter()
            .filter(|(_, site)| site_matches_place(site, location, wild))
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn clue_at_place_for_target(
        &self,
        location: &str,
        wild: Option<WildernessPos>,
        target: &str,
    ) -> Option<String> {
        for clue_id in self.clue_ids_at_place(location, wild) {
            let Some(clue) = self.catalog.clue(&clue_id) else {
                continue;
            };
            if clue.kind.matches_target(target) {
                return Some(clue_id);
            }
        }
        None
    }
}

fn site_matches_place(site: &ClueSite, location: &str, wild: Option<WildernessPos>) -> bool {
    match (wild, site) {
        (Some(pos), ClueSite::Wild(p)) => *p == pos,
        (None, ClueSite::Static(loc)) => loc == location,
        _ => false,
    }
}
