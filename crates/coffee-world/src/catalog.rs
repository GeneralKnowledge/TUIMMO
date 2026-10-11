//! Coffee.shop command catalogue. Handlers live on the world; this is only the registry.

use terminus_core::{ArgMode, CommandRegistry, CommandSpec};

pub fn coffee_registry() -> CommandRegistry {
    CommandRegistry::new(vec![
        CommandSpec {
            name: "help",
            aliases: &["?"],
            summary: "List commands or explain one",
            usage: "help [command]",
            args: ArgMode::OptionalWord,
        },
        CommandSpec {
            name: "look",
            aliases: &["l"],
            summary: "Study your current location",
            usage: "look",
            args: ArgMode::None,
        },
        CommandSpec {
            name: "map",
            aliases: &[],
            summary: "Show the Undernet map",
            usage: "map",
            args: ArgMode::None,
        },
        CommandSpec {
            name: "who",
            aliases: &[],
            summary: "List operators here or online",
            usage: "who [here|all]",
            args: ArgMode::OptionalWord,
        },
        CommandSpec {
            name: "go",
            aliases: &["move"],
            summary: "Take an exit",
            usage: "go <exit>",
            args: ArgMode::RequiredWord,
        },
        CommandSpec {
            name: "say",
            aliases: &["'"],
            summary: "Speak to people in this location",
            usage: "say <message>",
            args: ArgMode::Rest { required: true },
        },
        CommandSpec {
            name: "chat",
            aliases: &["gsay"],
            summary: "Speak to the whole Undernet",
            usage: "chat <message>",
            args: ArgMode::Rest { required: true },
        },
        CommandSpec {
            name: "status",
            aliases: &["st"],
            summary: "Show your operator status",
            usage: "status",
            args: ArgMode::None,
        },
        CommandSpec {
            name: "inventory",
            aliases: &["inv", "i"],
            summary: "List what you are carrying",
            usage: "inventory",
            args: ArgMode::None,
        },
        CommandSpec {
            name: "inspect",
            aliases: &["examine", "ex"],
            summary: "Inspect a person, object, or item",
            usage: "inspect <target>",
            args: ArgMode::RequiredWord,
        },
        CommandSpec {
            name: "quit",
            aliases: &["q"],
            summary: "Close this session",
            usage: "quit",
            args: ArgMode::None,
        },
    ])
}
