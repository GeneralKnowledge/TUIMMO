//! Static Undernet geography. The database is seeded from this definition.

#[derive(Clone, Copy, Debug)]
pub struct GrantDef {
    pub flag: &'static str,
    pub item_key: &'static str,
    pub item_name: &'static str,
    pub item_detail: &'static str,
    pub first: &'static str,
    pub already: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct ObjectDef {
    pub keyword: &'static str,
    pub aliases: &'static [&'static str],
    pub name: &'static str,
    pub description: &'static str,
    pub grant: Option<GrantDef>,
}

#[derive(Clone, Copy, Debug)]
pub struct LocationDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub objects: &'static [ObjectDef],
}

#[derive(Clone, Copy, Debug)]
pub struct ExitDef {
    pub from: &'static str,
    pub keyword: &'static str,
    pub to: &'static str,
    pub label: &'static str,
    pub requires_flag: Option<&'static str>,
}

const RELAY_GRANT: GrantDef = GrantDef {
    flag: "cipher-plate",
    item_key: "cipher-plate",
    item_name: "cipher plate",
    item_detail: "A warm metal plate etched with a route cipher.",
    first: "The plate comes free in your hand. Somewhere below, a sealed hatch will answer it.",
    already: "The relay housing is empty. You already carry the plate.",
};

/// Set when this operator has read the abandoned-exchange terminal.
/// The terminal stays mounted for everyone else.
pub const READ_TERMINAL_FLAG: &str = "read-terminal";

/// The page itself. Inspection may add a journal hint; the carried scrap does not.
pub const TORN_LOG_DETAIL: &str = "The last readable line is B-4, the same short code left on the service panel. \
A later hand wrote that the room was struck from the public list. The lock was left with the relay.";

const LOG_GRANT: GrantDef = GrantDef {
    flag: READ_TERMINAL_FLAG,
    item_key: "torn-log",
    item_name: "torn log",
    item_detail: TORN_LOG_DETAIL,
    first: "The terminal coughs up one readable page. You tear it free.",
    already: "The terminal has nothing new.",
};

pub fn locations() -> &'static [LocationDef] {
    &LOCATIONS
}

pub fn exits() -> &'static [ExitDef] {
    &EXITS
}

pub fn location(id: &str) -> Option<&'static LocationDef> {
    LOCATIONS.iter().find(|loc| loc.id == id)
}

pub fn map_text(current_id: &str, archive_open: bool) -> String {
    let mark = |id: &str, label: &str| -> String {
        if id == current_id {
            format!("*{label}*")
        } else {
            label.to_string()
        }
    };
    let archive =
        if archive_open { mark("black-archive", "Black Archive") } else { "(sealed)".to_string() };
    format!(
        "\
{roof}
   |
{shop} --- {alley} --- {tunnels} --- {relay}
   |            |                         |
   |       {exchange} --- {market}        {archive}
   |            |
   |     {service}
   |            |
   |   {abandoned}

* marks where you are standing.",
        roof = mark("roof-array", "Roof Array"),
        shop = mark("coffee-shop", "Coffee Shop"),
        alley = mark("neon-alley", "Neon Alley"),
        tunnels = mark("cooling-tunnels", "Cooling Tunnels"),
        relay = mark("old-relay", "Old Relay"),
        exchange = mark("exchange", "The Exchange"),
        market = mark("stack-market", "Stack Market"),
        service = mark("service-corridor", "Service Corridor"),
        abandoned = mark("abandoned-exchange", "Abandoned Exchange"),
        archive = archive,
    )
}

const LOCATIONS: [LocationDef; 10] = [
    LocationDef {
        id: "coffee-shop",
        name: "The Coffee Shop",
        description: "Steam, cheap ceramic, and a row of terminals that still boot. \
Operators treat the shop as neutral ground. The windows look onto wet neon, and a stair behind the counter climbs toward the roof.",
        objects: &[
            ObjectDef {
                keyword: "counter",
                aliases: &["bar"],
                name: "counter",
                description: "The counter is scarred laminate. Someone has carved a tiny cup into the corner.",
                grant: None,
            },
            ObjectDef {
                keyword: "board",
                aliases: &["jobs", "noticeboard", "notices"],
                name: "noticeboard",
                description: "A cork board by the terminals. Operators pin short notices here. \
Use `board` to read them, `board post <text>` to leave one, or `board remove <id>` for your own.",
                grant: None,
            },
        ],
    },
    LocationDef {
        id: "neon-alley",
        name: "Neon Alley",
        description: "A service alley between the shop and the exchange. Signage buzzes. Rain collects in a gutter that smells like hot dust.",
        objects: &[ObjectDef {
            keyword: "poster",
            aliases: &["wall"],
            name: "poster",
            description: "A torn poster advertises a night market that closed three years ago. The date is still bright.",
            grant: None,
        }],
    },
    LocationDef {
        id: "exchange",
        name: "The Exchange",
        description: "A trading floor gone half-dark. A few screens still scroll prices nobody here can settle. People come for rumours, not fills.",
        objects: &[ObjectDef {
            keyword: "ticker",
            aliases: &["screens"],
            name: "ticker",
            description: "The ticker loops a warning: RELAY UNSTAFFED. ARCHIVE LOCKED.",
            grant: None,
        }],
    },
    LocationDef {
        id: "stack-market",
        name: "Stack Market",
        description: "Stalls built from server racks sell cables, batteries, and stories. The awnings are foil blankets.",
        objects: &[ObjectDef {
            keyword: "stall",
            aliases: &["rack"],
            name: "stall",
            description: "The nearest stall sells blank plates. The vendor is not here. A sign says CASH ONLY, THEN LEAVE.",
            grant: None,
        }],
    },
    LocationDef {
        id: "service-corridor",
        name: "Service Corridor",
        description: "A maintenance hall under the exchange. Cable trays run overhead. One hatch in the floor has been forced and then abandoned.",
        objects: &[ObjectDef {
            keyword: "panel",
            aliases: &["hatch"],
            name: "access panel",
            description: "The panel lists floors. The lowest entry is crossed out in marker. \
The stroke misses the end of the line, where B-4 is still readable.",
            grant: None,
        }],
    },
    LocationDef {
        id: "abandoned-exchange",
        name: "The Abandoned Exchange",
        description: "The lower floor. Desks are shoved aside and the carpet is ash-coloured. A single terminal still has power.",
        objects: &[ObjectDef {
            keyword: "terminal",
            aliases: &["screen", "computer"],
            name: "terminal",
            description: "Amber text waits on a login that nobody will answer. The local log is still mounted.",
            grant: Some(LOG_GRANT),
        }],
    },
    LocationDef {
        id: "cooling-tunnels",
        name: "Cooling Tunnels",
        description: "Round concrete and a constant fan-hum. Condensate drips in time. The tunnel runs from the alley toward an old communications node.",
        objects: &[ObjectDef {
            keyword: "grate",
            aliases: &["fan"],
            name: "grate",
            description: "The grate is warm. Air pushes up from somewhere that still has electricity.",
            grant: None,
        }],
    },
    LocationDef {
        id: "old-relay",
        name: "The Old Relay",
        description: "A derelict communications node. Racks stand open. One relay sled is still seated, its cipher plate latched but loose. The dish cable is scuffed.",
        objects: &[
            ObjectDef {
                keyword: "relay",
                aliases: &["sled", "plate"],
                name: "relay sled",
                description: "The sled's cipher plate is warm to the touch and almost free of its latch.",
                grant: Some(RELAY_GRANT),
            },
            ObjectDef {
                keyword: "dish",
                aliases: &["antenna"],
                name: "dead dish",
                description: "A status lamp on the dish feed is dark. The cable runs down into the floor. The jacket is scuffed where it leaves the housing.",
                grant: None,
            },
        ],
    },
    LocationDef {
        id: "black-archive",
        name: "The Black Archive",
        description: "A quiet room of sealed stacks. The air is dry and the lights are low. You should not be able to hear the city from here, but you can.",
        objects: &[ObjectDef {
            keyword: "stacks",
            aliases: &["shelves", "archive"],
            name: "stacks",
            description: "The stacks are labelled with route numbers, not titles. One gap matches the plate you carried in.",
            grant: None,
        }],
    },
    LocationDef {
        id: "roof-array",
        name: "Roof Array",
        description: "The shop's roof. Antennae and a folding chair. The city is a circuit of wet light, and a break in the fencing opens onto the outskirts. The stair back down smells like coffee. Fresh scratches mark the tape on the mast.",
        objects: &[ObjectDef {
            keyword: "antenna",
            aliases: &["array", "mast"],
            name: "antenna",
                description: "The mast is lashed with tape. It still points at the old relay, a few blocks of rooftops away. Under the newest wrap, a scratch reads: one step west of the gate. The mark repeats. `journal record` keeps a field note.",
            grant: None,
        }],
    },
];

const EXITS: [ExitDef; 20] = [
    ExitDef {
        from: "coffee-shop",
        keyword: "alley",
        to: "neon-alley",
        label: "Neon Alley",
        requires_flag: None,
    },
    ExitDef {
        from: "coffee-shop",
        keyword: "up",
        to: "roof-array",
        label: "Roof Array",
        requires_flag: None,
    },
    ExitDef {
        from: "roof-array",
        keyword: "down",
        to: "coffee-shop",
        label: "The Coffee Shop",
        requires_flag: None,
    },
    ExitDef {
        from: "neon-alley",
        keyword: "shop",
        to: "coffee-shop",
        label: "The Coffee Shop",
        requires_flag: None,
    },
    ExitDef {
        from: "neon-alley",
        keyword: "exchange",
        to: "exchange",
        label: "The Exchange",
        requires_flag: None,
    },
    ExitDef {
        from: "neon-alley",
        keyword: "down",
        to: "cooling-tunnels",
        label: "Cooling Tunnels",
        requires_flag: None,
    },
    ExitDef {
        from: "exchange",
        keyword: "alley",
        to: "neon-alley",
        label: "Neon Alley",
        requires_flag: None,
    },
    ExitDef {
        from: "exchange",
        keyword: "market",
        to: "stack-market",
        label: "Stack Market",
        requires_flag: None,
    },
    ExitDef {
        from: "exchange",
        keyword: "service",
        to: "service-corridor",
        label: "Service Corridor",
        requires_flag: None,
    },
    ExitDef {
        from: "stack-market",
        keyword: "exchange",
        to: "exchange",
        label: "The Exchange",
        requires_flag: None,
    },
    ExitDef {
        from: "stack-market",
        keyword: "service",
        to: "service-corridor",
        label: "Service Corridor",
        requires_flag: None,
    },
    ExitDef {
        from: "service-corridor",
        keyword: "exchange",
        to: "exchange",
        label: "The Exchange",
        requires_flag: None,
    },
    ExitDef {
        from: "service-corridor",
        keyword: "market",
        to: "stack-market",
        label: "Stack Market",
        requires_flag: None,
    },
    ExitDef {
        from: "service-corridor",
        keyword: "down",
        to: "abandoned-exchange",
        label: "The Abandoned Exchange",
        requires_flag: None,
    },
    ExitDef {
        from: "abandoned-exchange",
        keyword: "up",
        to: "service-corridor",
        label: "Service Corridor",
        requires_flag: None,
    },
    ExitDef {
        from: "abandoned-exchange",
        keyword: "relay",
        to: "old-relay",
        label: "The Old Relay",
        requires_flag: None,
    },
    ExitDef {
        from: "cooling-tunnels",
        keyword: "up",
        to: "neon-alley",
        label: "Neon Alley",
        requires_flag: None,
    },
    ExitDef {
        from: "cooling-tunnels",
        keyword: "relay",
        to: "old-relay",
        label: "The Old Relay",
        requires_flag: None,
    },
    ExitDef {
        from: "old-relay",
        keyword: "tunnels",
        to: "cooling-tunnels",
        label: "Cooling Tunnels",
        requires_flag: None,
    },
    ExitDef {
        from: "old-relay",
        keyword: "floor",
        to: "abandoned-exchange",
        label: "The Abandoned Exchange",
        requires_flag: None,
    },
];

pub fn archive_exit() -> ExitDef {
    ExitDef {
        from: "old-relay",
        keyword: "archive",
        to: "black-archive",
        label: "The Black Archive",
        requires_flag: Some("cipher-plate"),
    }
}

pub fn archive_return() -> ExitDef {
    ExitDef {
        from: "black-archive",
        keyword: "relay",
        to: "old-relay",
        label: "The Old Relay",
        requires_flag: None,
    }
}

/// All exits, including the locked archive door.
pub fn all_exits() -> Vec<ExitDef> {
    let mut exits = EXITS.to_vec();
    exits.push(archive_exit());
    exits.push(archive_return());
    exits
}
