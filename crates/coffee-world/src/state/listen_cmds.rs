//! Sensory bearings — no clue spoilers.

use terminus_core::{Audience, Channel, CommandOutcome, GameError, ParsedCommand, SessionId};

use crate::geography::{
    atmosphere_at, chebyshev_from_gate, frontier_epoch, structure_is_active, ugc_band_label,
    WildernessPos, CHALK_KIND, SIGN_KIND, TRAIL_KIND,
};
use crate::state::{Transition, WorldState};
use crate::subway;

const LISTEN_RADIUS_CHEB: i64 = 6;

impl WorldState {
    pub(crate) fn cmd_listen(
        &mut self,
        session_id: SessionId,
        _cmd: &ParsedCommand,
    ) -> Result<Transition, GameError> {
        let player = self.player(session_id)?;
        let location = self.location_id(session_id)?;
        let Some(pos) = WildernessPos::parse(&location) else {
            return self.listen_in_hub(session_id, player, &location);
        };
        let world_seed = frontier_epoch().world_seed;
        let atmo = atmosphere_at(world_seed, pos, self.now_ms);
        let cheb = chebyshev_from_gate(pos);
        let mut lines = vec![
            format!("You are {}.", ugc_band_label(pos)),
            format!("Roughly {cheb} tiles from the Hub gate (Chebyshev)."),
        ];
        if atmo.exit_clarity < 0.75 {
            lines.push(
                "Wind and distance make the street grid hard to read. Trust what you can verify."
                    .into(),
            );
        }
        if let Some(entrance) = subway::entrance_at(pos) {
            lines.push(format!(
                "A municipal stair nearby leads down to {} (`go {}`).",
                entrance.station_label, entrance.down_keyword
            ));
        } else if pos.is_gate() {
            lines.push(
                "West opens into the Squalor. Floodwater blocks the other cardinals from here."
                    .into(),
            );
        }
        if let Some(hint) = self.nearest_trace_bearing(pos) {
            lines.push(hint);
        }
        lines.push(
            "Use look for what is in front of you. map is only a sketch when visibility is poor."
                .into(),
        );
        let body = lines.join("\n");
        let (event, op) = self.emit(Channel::System, Audience::Player(player), None, body);
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }

    fn listen_in_hub(
        &mut self,
        session_id: SessionId,
        player: terminus_core::PlayerId,
        location: &str,
    ) -> Result<Transition, GameError> {
        let body: String = if location == crate::model::STARTER_LOCATION {
            "Inside the Coffee Shop: muffled HVAC, distant traffic through the alley door. \
The Roof Array is up; the Squalor is beyond the fence."
                .to_string()
        } else {
            "Undernet rooms carry echo and wiring hum. Step into the Squalor to listen for bearings \
in the abandoned districts."
                .to_string()
        };
        let (event, op) = self.emit(Channel::System, Audience::Player(player), None, body);
        let _ = session_id;
        Ok(Transition {
            outcome: CommandOutcome { events: vec![event], quit: false },
            ops: vec![op],
        })
    }

    fn nearest_trace_bearing(&self, pos: WildernessPos) -> Option<String> {
        let region_id = self.geography.region(pos.identity.coord_key())?.id;
        let (ax, ay) = pos.absolute();
        let mut best: Option<(i64, String)> = None;
        for item in &self.geography.structures {
            if item.region_id != region_id {
                continue;
            }
            if !matches!(item.kind.as_str(), SIGN_KIND | TRAIL_KIND | CHALK_KIND) {
                continue;
            }
            if !structure_is_active(item, self.now_ms) {
                continue;
            }
            let there = WildernessPos {
                identity: pos.identity,
                local_x: item.local_x,
                local_y: item.local_y,
            };
            let (ox, oy) = there.absolute();
            let dist = (ax - ox).abs().max((ay - oy).abs());
            if dist == 0 || dist > LISTEN_RADIUS_CHEB {
                continue;
            }
            let bearing = bearing_word(ax, ay, ox, oy);
            let label = match item.kind.as_str() {
                TRAIL_KIND => format!("A trail post \"{}\"", item.name),
                SIGN_KIND => "A roadside sign".to_string(),
                CHALK_KIND => "Chalk on a wall".to_string(),
                _ => continue,
            };
            let line = format!("{label} lies {bearing} (about {dist} tiles).");
            if best.as_ref().is_none_or(|(d, _)| dist < *d) {
                best = Some((dist, line));
            }
        }
        best.map(|(_, line)| line)
    }
}

fn bearing_word(from_x: i64, from_y: i64, to_x: i64, to_y: i64) -> &'static str {
    let dx = to_x - from_x;
    let dy = to_y - from_y;
    if dx.abs() >= dy.abs() {
        if dx > 0 {
            "east"
        } else {
            "west"
        }
    } else if dy > 0 {
        "north"
    } else {
        "south"
    }
}
