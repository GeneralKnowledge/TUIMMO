//! Hidden clue threads scattered across the Squalor and Hub.

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
                    "mark" | "writing" | "scratch" | "shutter" | "circle" | "chalk"
                )
            }
            Self::Object => matches!(t.as_str(), "cup" | "tin" | "dent"),
            Self::Record => matches!(t.as_str(), "margin" | "ledger" | "strokes" | "column"),
            Self::Sign => matches!(t.as_str(), "note" | "placard" | "notice" | "letter"),
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
        if let Some(pos) = wild {
            return clue_at_wild_pos(&self.sites, pos);
        }
        clue_at_static(&self.sites, location)
    }
}
