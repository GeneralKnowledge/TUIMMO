//! Tests for thread vs flavor look labels.

use super::{look_object_label, ClueDef, ClueKind, SiteRule, WildSiteFilter};
use crate::geography::DensityBand;

#[test]
fn thread_clues_use_distinct_look_labels() {
    let thread = ClueDef {
        id: "t:0".into(),
        kind: ClueKind::Graffiti,
        surface_text: "x".into(),
        tags: vec!["starter".into()],
        journal_title: "t".into(),
        site: SiteRule::Wild(WildSiteFilter { band: DensityBand::NearHub, district: None }),
        reveals: vec![],
    };
    let flavor = ClueDef { tags: vec!["coverage".into()], ..thread.clone() };
    assert_eq!(look_object_label(&thread), "counted mark");
    assert_eq!(look_object_label(&flavor), "wall mark");
}
