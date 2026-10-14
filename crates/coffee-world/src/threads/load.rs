//! Load and validate thread definitions from TOML.

use std::collections::HashSet;

use serde::Deserialize;

use crate::geography::{DensityBand, DistrictKind};

use super::{
    ClueCatalog, ClueDef, ClueKind, KnowledgeRequirement, RevealDef, SiteRule, ThreadDef,
    WildSiteFilter,
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoadError {
    #[error("parse error: {0}")]
    Parse(String),
    #[error("duplicate thread id '{0}'")]
    DuplicateThread(String),
    #[error("duplicate clue id '{0}'")]
    DuplicateClue(String),
    #[error("thread '{thread}' clue '{clue}': {detail}")]
    InvalidClue { thread: String, clue: String, detail: String },
    #[error("unknown clue reference '{0}'")]
    UnknownClue(String),
    #[error("unknown tag reference '{0}'")]
    UnknownTag(String),
}

#[derive(Debug, Deserialize)]
struct RootFile {
    threads: Vec<ThreadFile>,
}

#[derive(Debug, Deserialize)]
struct ThreadFile {
    id: String,
    #[serde(default)]
    tags: Vec<String>,
    clues: Vec<ClueFile>,
}

#[derive(Debug, Deserialize)]
struct ClueFile {
    id: String,
    kind: String,
    surface_text: String,
    #[serde(default)]
    tags: Vec<String>,
    journal_title: Option<String>,
    site: SiteFile,
    #[serde(default)]
    reveals: Vec<RevealFile>,
}

#[derive(Debug, Deserialize)]
struct SiteFile {
    wild: Option<WildSiteFile>,
    #[serde(rename = "static")]
    static_location: Option<String>,
    metro_entrance: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WildSiteFile {
    band: String,
    district: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RevealFile {
    text: String,
    requires: RequireFile,
}

#[derive(Debug, Deserialize)]
struct RequireFile {
    seen_tag: Option<SeenTagFile>,
    seen_clue: Option<String>,
    corroborated_theory: Option<CorroboratedFile>,
    used_subway: Option<bool>,
    all: Option<Vec<RequireFile>>,
}

#[derive(Debug, Deserialize)]
struct SeenTagFile {
    tag: String,
    distinct_sites: u32,
}

#[derive(Debug, Deserialize)]
struct CorroboratedFile {
    subject: String,
    min: u32,
}

pub fn load_threads_from_str(raw: &str) -> Result<ClueCatalog, LoadError> {
    let parsed: RootFile = toml::from_str(raw).map_err(|err| LoadError::Parse(err.to_string()))?;
    let mut threads = Vec::new();
    let mut thread_ids = HashSet::new();
    for thread in parsed.threads {
        if !thread_ids.insert(thread.id.clone()) {
            return Err(LoadError::DuplicateThread(thread.id));
        }
        let mut clues = Vec::new();
        for clue in thread.clues {
            let kind = parse_kind(&clue.kind).ok_or_else(|| LoadError::InvalidClue {
                thread: thread.id.clone(),
                clue: clue.id.clone(),
                detail: format!("unknown kind '{}'", clue.kind),
            })?;
            if clue.surface_text.trim().is_empty() {
                return Err(LoadError::InvalidClue {
                    thread: thread.id.clone(),
                    clue: clue.id.clone(),
                    detail: "surface_text is empty".into(),
                });
            }
            let site = parse_site(&thread.id, &clue.id, &clue.site)?;
            let reveals = clue
                .reveals
                .into_iter()
                .map(|row| parse_reveal(&row))
                .collect::<Result<Vec<_>, _>>()?;
            let title = clue
                .journal_title
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| clue.id.clone());
            clues.push(ClueDef {
                id: clue.id,
                kind,
                surface_text: clue.surface_text,
                tags: clue.tags,
                journal_title: title,
                site,
                reveals,
            });
        }
        threads.push(ThreadDef { id: thread.id, tags: thread.tags, clues });
    }
    let catalog = ClueCatalog { threads };
    validate_references(&catalog)?;
    Ok(catalog)
}

pub fn load_default_threads() -> ClueCatalog {
    load_threads_from_str(include_str!("../../data/threads.toml"))
        .expect("shipped threads.toml must validate")
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

fn parse_band(raw: &str) -> Result<DensityBand, LoadError> {
    match raw {
        "near_hub" => Ok(DensityBand::NearHub),
        "fringe" => Ok(DensityBand::Fringe),
        "deep" => Ok(DensityBand::Deep),
        other => Err(LoadError::Parse(format!("unknown band '{other}'"))),
    }
}

fn parse_district(raw: &str) -> Result<DistrictKind, LoadError> {
    match raw {
        "residential" => Ok(DistrictKind::Residential),
        "commercial" => Ok(DistrictKind::Commercial),
        "civic" => Ok(DistrictKind::Civic),
        other => Err(LoadError::Parse(format!("unknown district '{other}'"))),
    }
}

fn parse_site(thread: &str, clue: &str, site: &SiteFile) -> Result<SiteRule, LoadError> {
    let mut chosen = 0;
    if site.wild.is_some() {
        chosen += 1;
    }
    if site.static_location.is_some() {
        chosen += 1;
    }
    if site.metro_entrance.is_some() {
        chosen += 1;
    }
    if chosen != 1 {
        return Err(LoadError::InvalidClue {
            thread: thread.into(),
            clue: clue.into(),
            detail: "site must set exactly one of wild, static, or metro_entrance".into(),
        });
    }
    if let Some(wild) = &site.wild {
        let band = parse_band(&wild.band).map_err(|err| LoadError::InvalidClue {
            thread: thread.into(),
            clue: clue.into(),
            detail: err.to_string(),
        })?;
        let district = wild.district.as_deref().map(parse_district).transpose().map_err(|err| {
            LoadError::InvalidClue {
                thread: thread.into(),
                clue: clue.into(),
                detail: err.to_string(),
            }
        })?;
        return Ok(SiteRule::Wild(WildSiteFilter { band, district }));
    }
    if let Some(location) = &site.static_location {
        return Ok(SiteRule::Static { location: location.clone() });
    }
    Ok(SiteRule::MetroEntrance { name: site.metro_entrance.clone().unwrap_or_default() })
}

fn parse_reveal(row: &RevealFile) -> Result<RevealDef, LoadError> {
    Ok(RevealDef { text: row.text.clone(), requires: parse_require(&row.requires)? })
}

fn parse_require(row: &RequireFile) -> Result<KnowledgeRequirement, LoadError> {
    let mut parts = 0;
    if row.seen_tag.is_some() {
        parts += 1;
    }
    if row.seen_clue.is_some() {
        parts += 1;
    }
    if row.corroborated_theory.is_some() {
        parts += 1;
    }
    if row.used_subway == Some(true) {
        parts += 1;
    }
    if row.all.is_some() {
        parts += 1;
    }
    if parts != 1 {
        return Err(LoadError::Parse(
            "each requirement must set exactly one of seen_tag, seen_clue, corroborated_theory, used_subway, or all"
                .into(),
        ));
    }
    if let Some(tag) = &row.seen_tag {
        return Ok(KnowledgeRequirement::SeenTag {
            tag: tag.tag.clone(),
            distinct_sites: tag.distinct_sites,
        });
    }
    if let Some(id) = &row.seen_clue {
        return Ok(KnowledgeRequirement::SeenClue(id.clone()));
    }
    if let Some(theory) = &row.corroborated_theory {
        return Ok(KnowledgeRequirement::CorroboratedTheory {
            subject: theory.subject.clone(),
            min: theory.min,
        });
    }
    if row.used_subway == Some(true) {
        return Ok(KnowledgeRequirement::UsedSubway);
    }
    let items = row
        .all
        .as_ref()
        .ok_or_else(|| LoadError::Parse("missing requirement body".into()))?
        .iter()
        .map(parse_require)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(KnowledgeRequirement::All(items))
}

fn validate_references(catalog: &ClueCatalog) -> Result<(), LoadError> {
    let clue_ids: HashSet<&str> = catalog.clues().map(|c| c.id.as_str()).collect();
    let mut tag_pool: HashSet<String> = HashSet::new();
    for thread in &catalog.threads {
        for tag in &thread.tags {
            tag_pool.insert(tag.clone());
        }
        for clue in &thread.clues {
            for tag in &clue.tags {
                tag_pool.insert(tag.clone());
            }
        }
    }
    for thread in &catalog.threads {
        for clue in &thread.clues {
            for reveal in &clue.reveals {
                validate_require(&reveal.requires, &clue_ids, &tag_pool)?;
            }
        }
    }
    Ok(())
}

fn validate_require(
    req: &KnowledgeRequirement,
    clue_ids: &HashSet<&str>,
    tag_pool: &HashSet<String>,
) -> Result<(), LoadError> {
    match req {
        KnowledgeRequirement::SeenClue(id) => {
            if !clue_ids.contains(id.as_str()) {
                return Err(LoadError::UnknownClue(id.clone()));
            }
        }
        KnowledgeRequirement::SeenTag { tag, .. } => {
            if !tag_pool.contains(tag) {
                return Err(LoadError::UnknownTag(tag.clone()));
            }
        }
        KnowledgeRequirement::All(items) => {
            for item in items {
                validate_require(item, clue_ids, tag_pool)?;
            }
        }
        KnowledgeRequirement::CorroboratedTheory { .. } => {}
        KnowledgeRequirement::UsedSubway => {}
        KnowledgeRequirement::AnyOfClues { clue_ids: need, .. } => {
            for id in need {
                if !clue_ids.contains(id.as_str()) {
                    return Err(LoadError::UnknownClue(id.clone()));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_catalog_loads() {
        let _ = load_default_threads();
    }

    #[test]
    fn rejects_bad_clue_reference() {
        let raw = r#"
[[threads]]
id = "bad"
tags = ["x"]

[[threads.clues]]
id = "only"
kind = "sign"
surface_text = "text"
tags = ["x"]
journal_title = "t"

[threads.clues.site]
static = "roof-array"

[[threads.clues.reveals]]
text = "nope"

[threads.clues.reveals.requires]
seen_clue = "missing-id"
"#;
        let err = load_threads_from_str(raw).unwrap_err();
        assert!(matches!(err, LoadError::UnknownClue(_)));
    }
}
