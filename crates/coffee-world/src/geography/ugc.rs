//! Player trace placement policy and look presentation budget.

use terminus_core::PlayerId;

use super::density::{density_band, DensityBand};
use super::identity::{WildernessPos, GATE_LOCAL_X, GATE_LOCAL_Y};
use super::records::{Structure, CHALK_KIND, MARKER_KIND, SIGN_KIND, TRAIL_KIND};

/// Chebyshev distance from the Hub gate (same scale as starter-walk and density bands).
pub const NEAR_HUB_MAX_CHEB: i64 = 20;
pub const FRINGE_MAX_CHEB: i64 = 48;

pub const CHALK_NEAR_HUB_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub const CHALK_FRINGE_TTL_MS: u64 = 14 * 24 * 60 * 60 * 1000;

pub const MAX_TRACE_LINES_ON_LOOK: usize = 2;
pub const MAX_SIGNS_PER_OPERATOR_REGION: u32 = 3;
pub const MAX_TRAILS_PER_OPERATOR_REGION: u32 = 1;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

pub fn chebyshev_from_gate(pos: WildernessPos) -> i64 {
    let (ax, ay) = pos.absolute();
    let gx = i64::from(GATE_LOCAL_X);
    let gy = i64::from(GATE_LOCAL_Y);
    (ax - gx).abs().max((ay - gy).abs())
}

pub fn permanent_traces_allowed(pos: WildernessPos) -> bool {
    chebyshev_from_gate(pos) > NEAR_HUB_MAX_CHEB
}

pub fn chalk_ttl_ms(pos: WildernessPos) -> u64 {
    if chebyshev_from_gate(pos) <= NEAR_HUB_MAX_CHEB {
        CHALK_NEAR_HUB_TTL_MS
    } else {
        CHALK_FRINGE_TTL_MS
    }
}

pub fn structure_is_active(item: &Structure, now_ms: u64) -> bool {
    match item.expires_at {
        Some(expires) => expires > now_ms,
        None => true,
    }
}

pub fn reject_permanent_trace_reason(pos: WildernessPos) -> Option<&'static str> {
    if pos.is_gate() {
        return Some("Keep the city gate clear.");
    }
    if !permanent_traces_allowed(pos) {
        return Some(
            "Near the Hub, only chalk marks are allowed — they fade. Use chalk, or move farther out for signs and trail posts.",
        );
    }
    None
}

pub fn validate_chalk_text(raw: &str) -> Result<String, &'static str> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("Chalk needs a short message.");
    }
    if text.len() > 120 {
        return Err("Chalk messages stay under 120 characters.");
    }
    Ok(text.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TracePriority {
    Trail = 0,
    OwnChalk = 1,
    Sign = 2,
    ForeignChalk = 3,
    Marker = 4,
    Other = 5,
}

fn trace_priority(kind: &str, owner: Option<PlayerId>, viewer: Option<PlayerId>) -> TracePriority {
    match kind {
        TRAIL_KIND => TracePriority::Trail,
        CHALK_KIND if owner == viewer => TracePriority::OwnChalk,
        SIGN_KIND => TracePriority::Sign,
        CHALK_KIND => TracePriority::ForeignChalk,
        MARKER_KIND => TracePriority::Marker,
        _ => TracePriority::Other,
    }
}

pub fn age_suffix(now_ms: u64, created_at: u64) -> String {
    let elapsed = now_ms.saturating_sub(created_at);
    let days = elapsed / DAY_MS;
    if days == 0 {
        "today".to_string()
    } else if days == 1 {
        "1d ago".to_string()
    } else {
        format!("{days}d ago")
    }
}

/// Sort and cap trace structures for look prose. Expired chalk is omitted.
pub fn filter_traces_for_tile<'a>(
    structures: impl IntoIterator<Item = &'a Structure>,
    now_ms: u64,
    viewer: Option<PlayerId>,
) -> (Vec<&'a Structure>, u32) {
    let mut active: Vec<&Structure> = structures
        .into_iter()
        .filter(|item| {
            matches!(item.kind.as_str(), SIGN_KIND | TRAIL_KIND | MARKER_KIND | CHALK_KIND)
                && structure_is_active(item, now_ms)
        })
        .collect();
    active.sort_by(|a, b| {
        let pa = trace_priority(&a.kind, a.owner_id, viewer);
        let pb = trace_priority(&b.kind, b.owner_id, viewer);
        pa.cmp(&pb).then_with(|| b.created_at.cmp(&a.created_at))
    });
    let hidden = active.len().saturating_sub(MAX_TRACE_LINES_ON_LOOK) as u32;
    active.truncate(MAX_TRACE_LINES_ON_LOOK);
    (active, hidden)
}

pub fn count_operator_structures_in_region(
    structures: &[Structure],
    region_id: uuid::Uuid,
    owner: PlayerId,
    kind: &str,
    now_ms: u64,
) -> u32 {
    u32::try_from(
        structures
            .iter()
            .filter(|item| {
                item.region_id == region_id
                    && item.owner_id == Some(owner)
                    && item.kind == kind
                    && structure_is_active(item, now_ms)
            })
            .count(),
    )
    .unwrap_or(u32::MAX)
}

pub fn band_label(pos: WildernessPos) -> &'static str {
    let (ax, ay) = pos.absolute();
    match density_band(ax, ay) {
        DensityBand::NearHub => "near the Hub",
        DensityBand::Fringe => "the fringe",
        DensityBand::Deep => "deep Squalor",
    }
}

#[cfg(test)]
mod tests {
    use super::super::identity::gate_position;
    use super::*;
    use uuid::Uuid;

    #[test]
    fn near_hub_blocks_permanent_traces() {
        let gate = gate_position();
        assert!(!permanent_traces_allowed(gate));
        let west = WildernessPos {
            identity: gate.identity,
            local_x: gate.local_x.saturating_sub(1),
            local_y: gate.local_y,
        };
        assert!(!permanent_traces_allowed(west));
    }

    #[test]
    fn look_budget_hides_extra_traces() {
        let region = Uuid::new_v4();
        let owner = PlayerId::new();
        let now = 1_000_000_u64;
        let mut rows = Vec::new();
        for i in 0..4 {
            rows.push(Structure {
                id: Uuid::new_v4(),
                region_id: region,
                local_x: 1,
                local_y: 1,
                kind: SIGN_KIND.into(),
                name: format!("sign{i}"),
                owner_id: Some(owner),
                access: "public".into(),
                interior_place: None,
                settlement_id: None,
                share: "private".into(),
                created_at: now.saturating_sub(u64::from(i as u32) * DAY_MS),
                expires_at: None,
            });
        }
        let (shown, hidden) = filter_traces_for_tile(&rows, now, Some(owner));
        assert_eq!(shown.len(), MAX_TRACE_LINES_ON_LOOK);
        assert_eq!(hidden, 2);
    }
}
