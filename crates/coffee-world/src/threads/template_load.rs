//! Load procedural thread templates from TOML.

use serde::Deserialize;

use super::{ClueKind, KnowledgeRequirement, RevealDef};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TemplateLoadError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("duplicate template id '{0}'")]
    Duplicate(String),
}

#[derive(Clone, Debug)]
pub struct TemplateCatalog {
    pub templates: Vec<TemplateDef>,
}

#[derive(Clone, Debug)]
pub struct TemplateDef {
    pub id: String,
    pub tier: TemplateTier,
    pub shape: String,
    pub tags: Vec<String>,
    pub clues: Vec<TemplateClueSlot>,
    pub chapters: Vec<TemplateChapter>,
    pub reveals: Vec<TemplateReveal>,
    pub pools: TemplatePools,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateTier {
    Local,
    Regional,
    Long,
}

#[derive(Clone, Debug)]
pub struct TemplateClueSlot {
    pub slot: Option<String>,
    pub kind: ClueKind,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct TemplateChapter {
    pub tiers: Vec<String>,
    pub clues: u32,
    pub pool: String,
    pub breadcrumb: bool,
}

#[derive(Clone, Debug)]
pub struct TemplateReveal {
    pub text: String,
    pub requires: TemplateRequire,
}

#[derive(Clone, Debug)]
pub enum TemplateRequire {
    SeenTag { tag: String, distinct_sites: u32 },
    AnyOfTags { n: u32, tags: Vec<String> },
}

#[derive(Clone, Debug, Default)]
pub struct TemplatePools {
    pub entries: std::collections::HashMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RootFile {
    template: Vec<TemplateFile>,
}

#[derive(Debug, Deserialize)]
struct TemplateFile {
    id: String,
    tier: String,
    shape: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    clue: Vec<TemplateClueFile>,
    #[serde(default)]
    chapter: Vec<TemplateChapterFile>,
    #[serde(default)]
    reveal: Vec<TemplateRevealFile>,
    #[serde(default)]
    pools: std::collections::HashMap<String, PoolFile>,
}

#[derive(Debug, Deserialize)]
struct TemplateClueFile {
    #[serde(default)]
    slot: Option<String>,
    kind: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct TemplateChapterFile {
    tiers: Vec<String>,
    clues: u32,
    pool: String,
    #[serde(default)]
    breadcrumb: bool,
}

#[derive(Debug, Deserialize)]
struct TemplateRevealFile {
    text: String,
    requires: TemplateRequireFile,
}

#[derive(Debug, Deserialize)]
struct TemplateRequireFile {
    seen_tag: Option<SeenTagFile>,
    any_of: Option<AnyOfFile>,
}

#[derive(Debug, Deserialize)]
struct SeenTagFile {
    tag: String,
    distinct_sites: u32,
}

#[derive(Debug, Deserialize)]
struct AnyOfFile {
    n: u32,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PoolFile {
    values: Vec<String>,
}

pub fn load_templates_from_str(raw: &str) -> Result<TemplateCatalog, TemplateLoadError> {
    let parsed: RootFile =
        toml::from_str(raw).map_err(|e| TemplateLoadError::Parse(e.to_string()))?;
    let mut templates = Vec::new();
    let mut ids = std::collections::HashSet::new();
    for row in parsed.template {
        if !ids.insert(row.id.clone()) {
            return Err(TemplateLoadError::Duplicate(row.id));
        }
        let tier = parse_tier(&row.tier).ok_or_else(|| {
            TemplateLoadError::Parse(format!("unknown tier '{}' on {}", row.tier, row.id))
        })?;
        let clues = row
            .clue
            .into_iter()
            .map(|c| {
                let kind = parse_kind(&c.kind).ok_or_else(|| {
                    TemplateLoadError::Parse(format!("unknown kind '{}' on {}", c.kind, row.id))
                })?;
                Ok(TemplateClueSlot { slot: c.slot, kind, text: c.text })
            })
            .collect::<Result<Vec<_>, TemplateLoadError>>()?;
        let chapters = row
            .chapter
            .into_iter()
            .map(|ch| TemplateChapter {
                tiers: ch.tiers,
                clues: ch.clues,
                pool: ch.pool,
                breadcrumb: ch.breadcrumb,
            })
            .collect();
        let reveals = row
            .reveal
            .into_iter()
            .map(|r| {
                Ok(TemplateReveal { text: r.text, requires: parse_template_require(&r.requires)? })
            })
            .collect::<Result<Vec<_>, TemplateLoadError>>()?;
        let mut pools = TemplatePools::default();
        for (name, pool) in row.pools {
            pools.entries.insert(name, pool.values);
        }
        templates.push(TemplateDef {
            id: row.id,
            tier,
            shape: row.shape,
            tags: row.tags,
            clues,
            chapters,
            reveals,
            pools,
        });
    }
    Ok(TemplateCatalog { templates })
}

pub fn load_default_templates() -> TemplateCatalog {
    load_templates_from_str(include_str!("../../data/thread_templates.toml"))
        .expect("shipped thread_templates.toml must validate")
}

fn parse_tier(raw: &str) -> Option<TemplateTier> {
    match raw {
        "local" => Some(TemplateTier::Local),
        "regional" => Some(TemplateTier::Regional),
        "long" => Some(TemplateTier::Long),
        _ => None,
    }
}

fn parse_kind(raw: &str) -> Option<ClueKind> {
    match raw {
        "graffiti" => Some(ClueKind::Graffiti),
        "object" => Some(ClueKind::Object),
        "record" => Some(ClueKind::Record),
        "sign" => Some(ClueKind::Sign),
        _ => None,
    }
}

fn parse_template_require(row: &TemplateRequireFile) -> Result<TemplateRequire, TemplateLoadError> {
    let mut parts = 0;
    if row.seen_tag.is_some() {
        parts += 1;
    }
    if row.any_of.is_some() {
        parts += 1;
    }
    if parts != 1 {
        return Err(TemplateLoadError::Parse(
            "template reveal requires exactly one predicate".into(),
        ));
    }
    if let Some(tag) = &row.seen_tag {
        return Ok(TemplateRequire::SeenTag {
            tag: tag.tag.clone(),
            distinct_sites: tag.distinct_sites,
        });
    }
    if let Some(any) = &row.any_of {
        return Ok(TemplateRequire::AnyOfTags { n: any.n, tags: any.tags.clone() });
    }
    Err(TemplateLoadError::Parse("missing requirement body".into()))
}

impl TemplateRequire {
    pub fn to_knowledge_requirement(
        &self,
        slots: &std::collections::HashMap<String, String>,
        instance_clue_ids: &[String],
    ) -> KnowledgeRequirement {
        match self {
            Self::SeenTag { tag, distinct_sites } => KnowledgeRequirement::SeenTag {
                tag: fill_slots(tag, slots),
                distinct_sites: *distinct_sites,
            },
            Self::AnyOfTags { n, tags } => {
                let filled: Vec<String> = tags.iter().map(|t| fill_slots(t, slots)).collect();
                if filled.len() == 1 {
                    KnowledgeRequirement::SeenTag { tag: filled[0].clone(), distinct_sites: *n }
                } else {
                    KnowledgeRequirement::AnyOfClues { n: *n, clue_ids: instance_clue_ids.to_vec() }
                }
            }
        }
    }
}

pub fn fill_slots(text: &str, slots: &std::collections::HashMap<String, String>) -> String {
    let mut out = text.to_string();
    for (key, value) in slots {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

pub fn template_reveal_defs(
    template: &TemplateDef,
    slots: &std::collections::HashMap<String, String>,
    instance_clue_ids: &[String],
) -> Vec<RevealDef> {
    template
        .reveals
        .iter()
        .map(|row| RevealDef {
            text: fill_slots(&row.text, slots),
            requires: row.requires.to_knowledge_requirement(slots, instance_clue_ids),
        })
        .collect()
}
