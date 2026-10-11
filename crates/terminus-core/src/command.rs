//! Command registry: names, aliases, argument shapes, help, and completion.
//!
//! Games register commands here and dispatch on the canonical name. Parsing does
//! not execute anything.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::ParseError;
use crate::text::MAX_COMMAND_CHARS;

/// How the remainder of a line is interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgMode {
    /// No arguments.
    None,
    /// Zero or one word.
    OptionalWord,
    /// Exactly one word.
    RequiredWord,
    /// One required word, then the remainder of the line.
    WordThenRest { required_rest: bool },
    /// The rest of the line, optionally required.
    Rest { required: bool },
}

/// Definition of one command.
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub usage: &'static str,
    pub args: ArgMode,
}

/// A parsed line. `name` is the canonical command, not the alias the user typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCommand {
    pub name: String,
    pub raw: String,
    pub args: Vec<String>,
    /// Remainder of the line for [`ArgMode::Rest`]. Empty otherwise.
    pub rest: String,
}

/// Hint shipped to clients for completion and contextual help.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandHint {
    pub name: String,
    pub aliases: Vec<String>,
    pub usage: String,
    pub summary: String,
}

/// Registry of commands. Alias lookup is case-insensitive.
#[derive(Clone, Debug)]
pub struct CommandRegistry {
    commands: Vec<CommandSpec>,
    alias_index: HashMap<String, usize>,
}

impl CommandRegistry {
    /// Build a registry. Duplicate names or aliases are a programming error and
    /// are dropped in favour of the first definition, which tests should catch.
    pub fn new(commands: Vec<CommandSpec>) -> Self {
        let mut alias_index = HashMap::new();
        for (idx, spec) in commands.iter().enumerate() {
            insert_alias(&mut alias_index, spec.name, idx);
            for alias in spec.aliases {
                insert_alias(&mut alias_index, alias, idx);
            }
        }
        Self { commands, alias_index }
    }

    pub fn specs(&self) -> &[CommandSpec] {
        &self.commands
    }

    pub fn hints(&self) -> Vec<CommandHint> {
        self.commands
            .iter()
            .map(|spec| CommandHint {
                name: spec.name.to_string(),
                aliases: spec.aliases.iter().map(|a| (*a).to_string()).collect(),
                usage: spec.usage.to_string(),
                summary: spec.summary.to_string(),
            })
            .collect()
    }

    pub fn parse(&self, line: &str) -> Result<ParsedCommand, ParseError> {
        if line.chars().count() > MAX_COMMAND_CHARS {
            return Err(ParseError::TooLong);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err(ParseError::Empty);
        }

        // MUD-style shortcut: `'hello` is `say hello`.
        if let Some(rest) = trimmed.strip_prefix('\'') {
            if self.alias_index.contains_key("'") {
                return self.finish("'", trimmed, rest.trim());
            }
        }

        let (head, rest) = split_head(trimmed);
        self.finish(head, trimmed, rest)
    }

    fn finish(&self, head: &str, raw: &str, rest: &str) -> Result<ParsedCommand, ParseError> {
        let key = normalize(head);
        let idx = self
            .alias_index
            .get(&key)
            .copied()
            .ok_or_else(|| ParseError::Unknown(head.to_string()))?;
        let spec = &self.commands[idx];
        let (args, rest_out) = match spec.args {
            ArgMode::None => {
                if !rest.is_empty() {
                    return Err(bad(spec, "this command takes no arguments"));
                }
                (Vec::new(), String::new())
            }
            ArgMode::OptionalWord => {
                if rest.is_empty() {
                    (Vec::new(), String::new())
                } else {
                    let mut words = rest.split_whitespace();
                    let word = words.next().unwrap_or("").to_string();
                    if words.next().is_some() {
                        return Err(bad(spec, "too many arguments"));
                    }
                    (vec![word], String::new())
                }
            }
            ArgMode::RequiredWord => {
                let mut words = rest.split_whitespace();
                let Some(word) = words.next() else {
                    return Err(bad(spec, "missing argument"));
                };
                if words.next().is_some() {
                    return Err(bad(spec, "too many arguments"));
                }
                (vec![word.to_string()], String::new())
            }
            ArgMode::WordThenRest { required_rest } => {
                let trimmed = rest.trim();
                if trimmed.is_empty() {
                    return Err(bad(spec, "missing argument"));
                }
                let mut parts = trimmed.splitn(2, char::is_whitespace);
                let word = parts.next().unwrap_or("").to_string();
                let remainder = parts.next().unwrap_or("").trim().to_string();
                if required_rest && remainder.is_empty() {
                    return Err(bad(spec, "missing message"));
                }
                (vec![word], remainder)
            }
            ArgMode::Rest { required } => {
                if required && rest.is_empty() {
                    return Err(bad(spec, "missing message"));
                }
                (Vec::new(), rest.to_string())
            }
        };
        Ok(ParsedCommand {
            name: spec.name.to_string(),
            raw: raw.to_string(),
            args,
            rest: rest_out,
        })
    }

    pub fn help_all(&self) -> String {
        let mut lines = vec![
            "Commands".to_string(),
            "Type help <command> for usage. PageUp and PageDown scroll the feed.".to_string(),
            String::new(),
        ];
        for spec in &self.commands {
            let aliases = if spec.aliases.is_empty() {
                String::new()
            } else {
                format!(" ({})", spec.aliases.join(", "))
            };
            lines.push(format!("{:<16} {}{aliases}", spec.usage, spec.summary));
        }
        lines.join("\n")
    }

    pub fn help_topic(&self, topic: &str) -> Result<String, ParseError> {
        let key = normalize(topic);
        let idx = self
            .alias_index
            .get(&key)
            .copied()
            .ok_or_else(|| ParseError::Unknown(topic.to_string()))?;
        let spec = &self.commands[idx];
        let aliases =
            if spec.aliases.is_empty() { "none".to_string() } else { spec.aliases.join(", ") };
        Ok(format!("{}\n{}\n\nusage: {}\naliases: {aliases}", spec.name, spec.summary, spec.usage))
    }

    /// Complete the token currently being typed.
    pub fn complete(&self, line: &str) -> Vec<String> {
        let trimmed_end = line.trim_end();
        let (head, _) = split_head(trimmed_end);
        if trimmed_end.contains(char::is_whitespace) {
            return Vec::new();
        }
        let prefix = normalize(head);
        let mut matches: Vec<String> = self
            .alias_index
            .keys()
            .filter(|alias| alias.starts_with(&prefix) && *alias != "'")
            .cloned()
            .collect();
        matches.sort();
        matches.dedup();
        matches
    }
}

fn bad(spec: &CommandSpec, message: &str) -> ParseError {
    ParseError::BadArgs {
        message: format!("{message}\nusage: {}", spec.usage),
        usage: spec.usage.to_string(),
    }
}

fn insert_alias(map: &mut HashMap<String, usize>, alias: &str, idx: usize) {
    map.entry(normalize(alias)).or_insert(idx);
}

fn normalize(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn split_head(trimmed: &str) -> (&str, &str) {
    match trimmed.find(char::is_whitespace) {
        Some(i) => (&trimmed[..i], trimmed[i..].trim()),
        None => (trimmed, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> CommandRegistry {
        CommandRegistry::new(vec![
            CommandSpec {
                name: "help",
                aliases: &["?"],
                summary: "list commands",
                usage: "help [command]",
                args: ArgMode::OptionalWord,
            },
            CommandSpec {
                name: "look",
                aliases: &["l"],
                summary: "inspect the room",
                usage: "look",
                args: ArgMode::None,
            },
            CommandSpec {
                name: "say",
                aliases: &["'"],
                summary: "speak locally",
                usage: "say <message>",
                args: ArgMode::Rest { required: true },
            },
            CommandSpec {
                name: "go",
                aliases: &["move"],
                summary: "leave",
                usage: "go <exit>",
                args: ArgMode::RequiredWord,
            },
            CommandSpec {
                name: "whisper",
                aliases: &["tell"],
                summary: "private message",
                usage: "whisper <player> <message>",
                args: ArgMode::WordThenRest { required_rest: true },
            },
        ])
    }

    #[test]
    fn aliases_and_rest() {
        let reg = registry();
        let parsed = reg.parse("  L  ").unwrap();
        assert_eq!(parsed.name, "look");
        let parsed = reg.parse("'hello there").unwrap();
        assert_eq!(parsed.name, "say");
        assert_eq!(parsed.rest, "hello there");
        let parsed = reg.parse("SAY  hi   you").unwrap();
        assert_eq!(parsed.rest, "hi   you");
        let parsed = reg.parse("whisper bob hello there").unwrap();
        assert_eq!(parsed.name, "whisper");
        assert_eq!(parsed.args, vec!["bob".to_string()]);
        assert_eq!(parsed.rest, "hello there");
    }

    #[test]
    fn rejects_bad_arity() {
        let reg = registry();
        assert!(matches!(reg.parse("look around"), Err(ParseError::BadArgs { .. })));
        assert!(matches!(reg.parse("go"), Err(ParseError::BadArgs { .. })));
        assert!(matches!(reg.parse(""), Err(ParseError::Empty)));
        assert!(matches!(reg.parse("dance"), Err(ParseError::Unknown(_))));
    }

    #[test]
    fn help_and_completion() {
        let reg = registry();
        let help = reg.help_topic("?").unwrap();
        assert!(help.contains("help"));
        let matches = reg.complete("mo");
        assert_eq!(matches, vec!["move".to_string()]);
    }
}
