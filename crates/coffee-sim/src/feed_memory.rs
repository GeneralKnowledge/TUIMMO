//! Parse player-visible feed/finding text into simulator memory (not authoritative state).

use coffee_world::WorldState;
use terminus_core::{Game, SessionId};

use crate::memory::AgentMemory;

/// Update agent memory from the same view a human player would see after a command.
pub fn absorb_command_outcome(
    world: &WorldState,
    session_id: SessionId,
    command: &str,
    memory: &mut AgentMemory,
) {
    let Ok(view) = world.view(session_id) else {
        return;
    };
    let location_key = world.location_key_for_sim(session_id).unwrap_or(view.location_name.clone());
    if let Some(finding) = &view.finding {
        absorb_inspection_text(finding, &location_key, memory);
    }
    for line in &view.feed {
        absorb_inspection_text(&line.text, &location_key, memory);
    }
    if command.starts_with("look") {
        absorb_look_text(&view.description, &location_key, memory);
    }
    if let Some(keyword) = command.strip_prefix("inspect ") {
        let keyword = keyword.split_whitespace().next().unwrap_or(keyword).to_ascii_lowercase();
        if !keyword.is_empty() {
            memory.record_inspected_keyword(&location_key, &keyword);
        }
    }
    if let Some(overlay) = &view.overlay {
        if command.starts_with("theory") {
            absorb_theory_overlay(overlay, memory);
        }
    }
}

fn absorb_inspection_text(text: &str, location: &str, memory: &mut AgentMemory) {
    if text.contains("first charted") {
        memory.record_public_trace_at(location, "charting");
    }
    if text.contains("Sign by") {
        memory.record_public_trace_at(location, "sign");
    }
    if text.contains("Trail post") {
        memory.record_public_trace_at(location, "trail");
    }
    if let Some(subject) = extract_theory_list_subject(text) {
        memory.record_theory_subject_hint(&subject);
        memory.record_public_trace_at(location, &format!("theory:{subject}"));
    }
    if text.contains("public") && text.contains("entries on `") {
        if let Some(subject) = extract_backtick_subject(text) {
            memory.record_theory_subject_hint(&subject);
        }
    }
}

fn absorb_look_text(description: &str, location: &str, memory: &mut AgentMemory) {
    if description.contains("Sign:") || description.contains("sign here") {
        memory.record_public_trace_at(location, "sign");
    }
    if description.contains("Trail post") {
        memory.record_public_trace_at(location, "trail");
    }
}

fn absorb_theory_overlay(overlay: &str, memory: &mut AgentMemory) {
    if overlay.contains("Subject:") {
        for line in overlay.lines() {
            if let Some(rest) = line.strip_prefix("Subject:") {
                let subject = rest.trim();
                if !subject.is_empty() {
                    memory.record_theory_read(subject);
                }
            }
        }
    }
}

fn extract_theory_list_subject(text: &str) -> Option<String> {
    let needle = "theory list `";
    let start = text.find(needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

fn extract_backtick_subject(text: &str) -> Option<String> {
    let start = text.find('`')? + 1;
    let rest = &text[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}
