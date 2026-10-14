//! Deterministic sensory degradation by region and time bucket (derived only).

use super::generate::thread_placement_hash;
use super::identity::WildernessPos;
use super::ugc::chebyshev_from_gate;

const DAY_MS: u64 = 86_400_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    /// 0 = exits feel unreliable; 1 = crisp orientation prose.
    pub exit_clarity: f32,
    /// When true, wilderness map blurs non-adjacent tiles.
    pub map_degraded: bool,
}

impl Atmosphere {
    pub const fn clear() -> Self {
        Self { exit_clarity: 1.0, map_degraded: false }
    }
}

pub fn atmosphere_at(world_seed: u64, pos: WildernessPos, now_ms: u64) -> Atmosphere {
    let day_bucket = now_ms / DAY_MS;
    let cheb = chebyshev_from_gate(pos);
    let h = thread_placement_hash(
        world_seed,
        "atmosphere",
        &format!("{}:{}:{}", pos.identity.origin_x, pos.identity.origin_y, day_bucket),
        pos.local_x as i32,
        pos.local_y as i32,
    );
    let noise = (h % 1000) as f32 / 1000.0;
    let band_penalty = if cheb <= 20 {
        0.05
    } else if cheb <= 48 {
        0.15 + noise * 0.25
    } else {
        0.35 + noise * 0.45
    };
    let exit_clarity = (1.0 - band_penalty).clamp(0.35, 1.0);
    let map_degraded = cheb > 20 || (cheb > 8 && noise > 0.55);
    Atmosphere { exit_clarity, map_degraded }
}

pub fn exit_clarity_note(atmo: Atmosphere) -> Option<&'static str> {
    if atmo.exit_clarity >= 0.85 {
        return None;
    }
    if atmo.exit_clarity >= 0.6 {
        Some("The street layout feels uncertain — listen before you commit to a direction.")
    } else {
        Some("Distant exits are hard to read. listen may help more than guessing.")
    }
}

pub fn fogged_map_symbol(base: char, here: WildernessPos, target: WildernessPos, atmo: Atmosphere) -> char {
    if !atmo.map_degraded {
        return base;
    }
    let dx = (i16::from(target.local_x) - i16::from(here.local_x)).abs();
    let dy = (i16::from(target.local_y) - i16::from(here.local_y)).abs();
    if (dx <= 1 && dy <= 1) || base == '~' || base == '^' {
        base
    } else {
        '?'
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::gate_position;

    #[test]
    fn atmosphere_is_deterministic() {
        let pos = gate_position();
        let a = atmosphere_at(42, pos, 1_700_000_000_000);
        let b = atmosphere_at(42, pos, 1_700_000_000_000);
        assert_eq!(a, b);
    }

    #[test]
    fn gate_stays_mostly_clear() {
        let pos = gate_position();
        let atmo = atmosphere_at(99, pos, 0);
        assert!(atmo.exit_clarity >= 0.9);
    }
}
