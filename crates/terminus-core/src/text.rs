//! Small text helpers shared by games. Input is untrusted.

pub const MAX_COMMAND_CHARS: usize = 480;
pub const MAX_SPEECH_CHARS: usize = 400;

/// Drop control characters and cap length by Unicode scalar count.
pub fn clean_line(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if out.chars().count() >= max_chars {
            break;
        }
        if ch.is_control() {
            continue;
        }
        out.push(ch);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_controls_and_caps() {
        assert_eq!(clean_line("  hi\n\u{0007} there  ", 20), "hi there");
        assert_eq!(clean_line("abcdef", 3), "abc");
    }
}
