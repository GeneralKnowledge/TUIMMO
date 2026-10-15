//! Coffee.shop commands: registry metadata, session access, and handlers.

use terminus_core::{ArgMode, CommandRegistry, CommandSpec, GameError, ParsedCommand, SessionId};

use super::{Transition, WorldState};

pub(super) type Handler =
    fn(&mut WorldState, SessionId, &ParsedCommand) -> Result<Transition, GameError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandAccess {
    /// Any connected session, including spectators.
    Open,
    /// Player sessions only (movement, building, social management, …).
    Play,
    /// Player sessions that may speak on channels.
    Speak,
}

pub(crate) struct CoffeeCommand {
    pub spec: CommandSpec,
    pub access: CommandAccess,
    pub handler: Handler,
}

pub(crate) fn coffee_commands() -> Vec<CoffeeCommand> {
    vec![
        CoffeeCommand {
            spec: CommandSpec {
            name: "help",
            aliases: &["?"],
            summary: "Explain a topic, or one command",
            usage: "help [topic]",
            args: ArgMode::OptionalWord,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_help,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "welcome",
            aliases: &[],
            summary: "Show the arrival note again",
            usage: "welcome",
            args: ArgMode::None,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_welcome,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "look",
            aliases: &["l"],
            summary: "Study your current location",
            usage: "look",
            args: ArgMode::None,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_look,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "map",
            aliases: &[],
            summary: "Show the Hub map, Squalor chart, or subway diagram",
            usage: "map",
            args: ArgMode::None,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_map,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "who",
            aliases: &[],
            summary: "List operators here or online",
            usage: "who [here|all]",
            args: ArgMode::OptionalWord,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_who,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "go",
            aliases: &["move"],
            summary: "Take an exit",
            usage: "go <exit>",
            args: ArgMode::RequiredWord,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_go,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "say",
            aliases: &["'"],
            summary: "Speak to people in this location",
            usage: "say <message>",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Speak,
            handler: WorldState::cmd_say,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "chat",
            aliases: &["gsay"],
            summary: "Speak to the whole world",
            usage: "chat <message>",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Speak,
            handler: WorldState::cmd_chat,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "whisper",
            aliases: &["tell"],
            summary: "Send a private message to an online operator",
            usage: "whisper <player> <message>",
            args: ArgMode::WordThenRest { required_rest: true },
            },
            access: CommandAccess::Speak,
            handler: WorldState::cmd_whisper,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "emote",
            aliases: &["me", "em"],
            summary: "Describe an action in this location",
            usage: "emote <action>",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Speak,
            handler: WorldState::cmd_emote,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "profile",
            aliases: &[],
            summary: "View a profile or set your bio/status",
            usage: "profile [player|bio <text>|status <text>]",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_profile,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "friend",
            aliases: &[],
            summary: "Manage friendship requests",
            usage: "friend <add|accept|decline|remove> <player>",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_friend,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "friends",
            aliases: &[],
            summary: "List friends and pending requests",
            usage: "friends",
            args: ArgMode::None,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_friends,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "ignore",
            aliases: &["block"],
            summary: "Ignore a player across chat channels",
            usage: "ignore <player>",
            args: ArgMode::RequiredWord,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_ignore,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "unignore",
            aliases: &["unblock"],
            summary: "Stop ignoring a player",
            usage: "unignore <player>",
            args: ArgMode::RequiredWord,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_unignore,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "ignores",
            aliases: &[],
            summary: "List players you ignore",
            usage: "ignores",
            args: ArgMode::None,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_ignores,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "status",
            aliases: &["st"],
            summary: "Show your operator status",
            usage: "status",
            args: ArgMode::None,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_status,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "inventory",
            aliases: &["inv", "i"],
            summary: "List what you are carrying",
            usage: "inventory",
            args: ArgMode::None,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_inventory,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "inspect",
            aliases: &["examine", "ex"],
            summary: "Inspect a person, object, or item",
            usage: "inspect <target>",
            args: ArgMode::RequiredWord,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_inspect,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "journal",
            aliases: &[],
            summary: "Read or record your private field journal",
            usage: "journal [recent|read|record]",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_journal,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "theory",
            aliases: &[],
            summary: "Share an observation or theory about the delisted floor",
            usage: "theory [list|read <id>|post <observation|theory> <text>|remove <id>]",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_theory,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "waypoint",
            aliases: &[],
            summary: "Save a private name for where you are standing",
            usage: "waypoint [add|list|show|note|remove]",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_waypoint,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "mail",
            aliases: &[],
            summary: "Leave a private letter, including for someone who is offline",
            usage: "mail [list|read|send|delete]",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_mail,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "invite",
            aliases: &[],
            summary: "Show unused account invite codes you can share",
            usage: "invite [list]",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_invite,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "mark",
            aliases: &[],
            summary: "Leave a named marker on this Squalor tile",
            usage: "mark <name>",
            args: ArgMode::RequiredWord,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_mark,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "build",
            aliases: &[],
            summary: "Build a cabin or workshop on this Squalor tile",
            usage: "build <cabin|workshop> <name>",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_build,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "enter",
            aliases: &[],
            summary: "Enter a cabin or workshop on this tile",
            usage: "enter <name>",
            args: ArgMode::RequiredWord,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_enter,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "leave",
            aliases: &[],
            summary: "Leave a cabin or workshop interior",
            usage: "leave",
            args: ArgMode::None,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_leave,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "home",
            aliases: &[],
            summary: "Claim, name, move, or abandon a personal home; manage cabin controls",
            usage:
                "home [claim|name|move|abandon|inspect|access|share|allow|deny|rename|demolish] …",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_home,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "workshop",
            aliases: &[],
            summary: "Inspect a workshop or use its project board",
            usage: "workshop <inspect|board [post <text>|remove <id>]>",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_workshop,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "listen",
            aliases: &[],
            summary: "Listen for bearings — band, metro, nearby traces",
            usage: "listen",
            args: ArgMode::None,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_listen,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "chalk",
            aliases: &[],
            summary: "Leave or refresh ephemeral chalk on this tile",
            usage: "chalk <message>|refresh",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_chalk,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "traces",
            aliases: &[],
            summary: "List your active signs, trails, and chalk in this region",
            usage: "traces",
            args: ArgMode::None,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_traces,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "survey",
            aliases: &[],
            summary: "Private survey summary or publish to the Coffee Shop board",
            usage: "survey [status|publish <text>]",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_survey,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "sign",
            aliases: &[],
            summary: "Leave, read, or remove a sign",
            usage: "sign <text>|read|remove",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_sign,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "trail",
            aliases: &[],
            summary: "Leave a trail post on this tile",
            usage: "trail <name>",
            args: ArgMode::RequiredWord,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_trail,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "board",
            aliases: &["noticeboard", "notices"],
            summary: "Read or post on the Coffee Shop noticeboard",
            usage: "board [post <text>|remove <id>]",
            args: ArgMode::Rest { required: false },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_board,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "region",
            aliases: &["rsay"],
            summary: "Speak across this physical region",
            usage: "region <message>",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Speak,
            handler: WorldState::cmd_region,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "settle",
            aliases: &[],
            summary: "Found, join, or manage a settlement",
            usage: "settle <create|inspect|invite|invites|accept|…> …",
            args: ArgMode::Rest { required: true },
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_settle,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "settlements",
            aliases: &[],
            summary: "List settlements visible from here",
            usage: "settlements",
            args: ArgMode::None,
            },
            access: CommandAccess::Play,
            handler: WorldState::cmd_settlements,
        },
        CoffeeCommand {
            spec: CommandSpec {
            name: "quit",
            aliases: &["q"],
            summary: "Close this session",
            usage: "quit",
            args: ArgMode::None,
            },
            access: CommandAccess::Open,
            handler: WorldState::cmd_quit,
        },
    ]
}

pub(crate) fn coffee_registry(commands: &[CoffeeCommand]) -> CommandRegistry {
    CommandRegistry::new(commands.iter().map(|cmd| cmd.spec.clone()).collect())
}

pub(super) fn find_command<'a>(
    commands: &'a [CoffeeCommand],
    name: &str,
) -> Option<&'a CoffeeCommand> {
    commands.iter().find(|cmd| cmd.spec.name == name)
}
