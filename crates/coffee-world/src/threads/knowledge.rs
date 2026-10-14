//! Knowledge requirements for thread reveal text (computed at read time).

use std::collections::{BTreeMap, HashMap, HashSet};

use terminus_core::PlayerId;

use super::{ClueCatalog, ClueSite, KnowledgeRequirement, RevealDef, ThreadDef};

/// One satisfied knowledge predicate for an operator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Knowledge {
    SeenTag { tag: String, distinct_sites: u32 },
    SeenClue(String),
    CorroboratedTheory { subject: String, min: u32 },
    All(Vec<Knowledge>),
}

impl Knowledge {
    pub fn from_requirement(req: &KnowledgeRequirement) -> Self {
        match req {
            KnowledgeRequirement::SeenTag { tag, distinct_sites } => {
                Self::SeenTag { tag: tag.clone(), distinct_sites: *distinct_sites }
            }
            KnowledgeRequirement::SeenClue(id) => Self::SeenClue(id.clone()),
            KnowledgeRequirement::CorroboratedTheory { subject, min } => {
                Self::CorroboratedTheory { subject: subject.clone(), min: *min }
            }
            KnowledgeRequirement::All(items) => {
                Self::All(items.iter().map(Self::from_requirement).collect())
            }
        }
    }

    /// Higher wins when multiple reveals match.
    pub fn specificity(&self) -> u32 {
        match self {
            Self::All(items) => 1_000 + u32::try_from(items.len()).unwrap_or(0),
            Self::SeenClue(_) => 100,
            Self::SeenTag { distinct_sites, .. } => 50 + *distinct_sites,
            Self::CorroboratedTheory { min, .. } => 40 + *min,
        }
    }
}

pub struct KnowledgeContext<'a> {
    pub catalog: &'a ClueCatalog,
    pub seen_clues: HashSet<String>,
    pub seen_sites_by_tag: HashMap<String, HashSet<ClueSite>>,
    pub theory_authors_by_subject: BTreeMap<String, HashSet<PlayerId>>,
}

impl<'a> KnowledgeContext<'a> {
    pub fn build(
        catalog: &'a ClueCatalog,
        operator: PlayerId,
        seen_clue_rows: &[(PlayerId, String, ClueSite)],
        theory_authors: &BTreeMap<String, HashSet<PlayerId>>,
    ) -> Self {
        let mut seen_clues = HashSet::new();
        let mut seen_sites_by_tag: HashMap<String, HashSet<ClueSite>> = HashMap::new();
        for (who, clue_id, site) in seen_clue_rows {
            if *who != operator {
                continue;
            }
            seen_clues.insert(clue_id.clone());
            if let Some(clue) = catalog.clue(clue_id) {
                for tag in &clue.tags {
                    seen_sites_by_tag.entry(tag.clone()).or_default().insert(site.clone());
                }
            }
        }
        Self {
            catalog,
            seen_clues,
            seen_sites_by_tag,
            theory_authors_by_subject: theory_authors.clone(),
        }
    }

    pub fn satisfies(&self, knowledge: &Knowledge) -> bool {
        match knowledge {
            Knowledge::SeenTag { tag, distinct_sites } => self
                .seen_sites_by_tag
                .get(tag)
                .is_some_and(|sites| sites.len() >= *distinct_sites as usize),
            Knowledge::SeenClue(id) => self.seen_clues.contains(id),
            Knowledge::CorroboratedTheory { subject, min } => self
                .theory_authors_by_subject
                .get(subject)
                .is_some_and(|authors| authors.len() >= *min as usize),
            Knowledge::All(items) => items.iter().all(|item| self.satisfies(item)),
        }
    }
}

/// Pick the reveal with the highest specificity among those whose requirements are met.
pub fn best_reveal_text(reveals: &[RevealDef], ctx: &KnowledgeContext<'_>) -> Option<String> {
    let mut best: Option<(&RevealDef, u32)> = None;
    for reveal in reveals {
        let knowledge = Knowledge::from_requirement(&reveal.requires);
        if !ctx.satisfies(&knowledge) {
            continue;
        }
        let score = knowledge.specificity();
        if best.is_none_or(|(_, best_score)| score > best_score) {
            best = Some((reveal, score));
        }
    }
    best.map(|(reveal, _)| reveal.text.clone())
}

pub fn thread_final_clue_id(thread: &ThreadDef) -> Option<&str> {
    thread.clues.last().map(|clue| clue.id.as_str())
}
