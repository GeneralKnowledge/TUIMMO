//! Distinguish coverage flavor from thread clues (authored + procedural).

use super::ClueDef;

/// Coverage grid fillers tagged `coverage` — inspectable flavor, not thread progression.
pub fn is_coverage_flavor(clue: &ClueDef) -> bool {
    clue.tags.iter().any(|t| t == "coverage")
}

/// Authored threads, procedural templates, and starter threads — not coverage flavor.
pub fn is_thread_clue(clue: &ClueDef) -> bool {
    !is_coverage_flavor(clue)
}

/// Player-visible noun phrase in `look` (see [`super::ClueKind::look_label_for`]).
pub fn look_object_label(clue: &ClueDef) -> String {
    clue.kind.look_label_for(is_thread_clue(clue)).to_string()
}
