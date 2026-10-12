# Terminus + Coffee.shop

Terminus is a small Rust engine for multiplayer terminal games. Coffee.shop is the reference game: a persistent post-disaster megacity (the Undernet) whose inhabited **Hub** is surrounded by the abandoned urban **Squalor**. Players connect with SSH, get a full-screen terminal UI, and share one authoritative world.

The engine does not know about operators, locations, or the Undernet. `examples/minimal-game` builds a two-room game on Terminus alone.

## Play

```bash
cargo run -p coffee-server -- serve
```

The server listens on `0.0.0.0:2222` and keeps its database, host key, and local protocol socket under `data/`.

From another terminal:

```bash
ssh -p 2222 -o StrictHostKeyChecking=accept-new <username>@127.0.0.1
```

There is no operating-system shell. The first login creates an account when registration is open (the default). The password must be 8–128 characters. Usernames are 3–16 characters: a letter, then letters, digits, or `_`. A second login with the same name returns the same operator, including their last location.

Stop the server with Ctrl-C and start it again. Operators are still where they were.

A local TUI, without SSH, can attach to a running server:

```bash
cargo run -p coffee-server -- play
```

Create an account without putting the password in the process list:

```bash
COFFEE_PASSWORD='a-long-secret' cargo run -p coffee-server -- account create --username ada --operator-name Ada
```

## First session

You arrive in the Undernet, in the Coffee Shop. The world stays when you leave, and other operators share it. You can explore alone. A new operator sees a short note; the same account does not get that page on every reconnect.

1. `look` — the room, the exits, and what is here.
2. `go alley` — Neon Alley, next door. `shop` comes back. `up` is the Roof Array, and `down` returns to the shop.
3. `inspect counter` — read something in the room. The noticed panel keeps the last reading.
4. `help` — short topics: `help around`, `help people`, `help explore`, `help stuck`, or `help look` for one command. `help commands` is the full list.
5. `say hello` speaks in this room, if anyone is here. `chat hello` reaches operators who are online, anywhere. Neither is required. `who` shows who is actually here.
6. `journal` is a private notebook. `waypoint add shop` remembers where you are standing. Both are optional, and only you can read them.
7. `welcome` shows the arrival note again. `look` clears the notes page.
8. `quit` leaves. The same account returns you to the place you left.

`map` shows the city. Nothing in these commands is a required task. From the Roof Array you can enter the Squalor (`outskirts` or `squalor`), find a subway stair (`go metro`), `build cabin`, leave a `sign`, found a `settle`, and speak with `region`.

## Layout

| Crate | Role |
| --- | --- |
| `terminus-core` | Commands, sessions, views, and the `Game` trait |
| `terminus-tui` | Ratatui layout, input, and the remote client loop |
| `terminus-protocol` | Versioned length-prefixed JSON frames |
| `terminus-ssh` | SSH gateway. Auth starts a game session, never a shell |
| `coffee-world` | Hub, Squalor, subway, and SQLite persistence |
| `coffee-server` | Hub, protocol listener, and `coffee-server` binary |
| `examples/minimal-game` | Engine demo with no Coffee.shop dependency |

The simulation is synchronous and single-owner. The server task applies one action at a time, then commits the rows that changed. SSH and the TUI never touch the world directly.

## Develop

The toolchain is pinned in `rust-toolchain.toml` (Rust 1.99).

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p minimal-game -- --script
```

`cargo tree -p minimal-game -i coffee-world` should fail: the example must not depend on the game.

Docs:

- [Architecture](docs/architecture.md)
- [World architecture](docs/world-architecture.md)
- [Protocol](docs/protocol.md)
- [Undernet / Hub & Squalor](docs/game-design.md)
- [Setting and subway](docs/setting-and-subway.md)
- [Security](docs/security.md)
- [Development](docs/development.md)
- [Deployment](docs/deployment.md)

## What this version does

- SSH into a TUI, with password authentication and a generated host key
- Register or log in, and resume the same operator after a restart
- A short arrival note for a new operator at the Coffee Shop, and `help` topics (`around`, `people`, `explore`, `commands`, `stuck`). `welcome` shows the note again
- Header, location, a noticed panel for the latest inspection, status, scrollable feed, and a command prompt with history and completion
- Social Undernet: local `say`, global `chat`, regional `region`/`rsay`, private `whisper`, persistent `mail`, `emote`, profiles, mutual friends, and ignore/block
- Homesteads: `build cabin`, enter/leave interiors, visitor access controls, signs, trail posts
- Settlements: named membership groups anchored to outskirts tiles, invites, claim/unclaim of structures, discovery without a global directory
- Coffee Shop and settlement noticeboards with bounded, expiring posts
- Ten linked Hub locations, including a sealed Black Archive, plus a two-station subway vertical slice (one hop ≈ eight surface tiles)
- Generated Squalor beyond the Roof Array: abandoned urban districts, deterministic regions, cardinal movement, and a way back with `city`
- Private field journal (`journal`, `journal recent`, `journal read`, `journal record`) for authored discoveries, including one optional outskirts survey that does not open the Black Archive
- Shared theories (`theory list`, `read`, `post observation`, `post theory`, `remove`) about the delisted floor. They persist, they are not copied from the private journal, and they do not unlock anything
- Personal waypoints (`waypoint add`, `list`, `show`, `note`, `remove`) for places an operator has stood, with private notes and return hints
- SQLite persistence for accounts, operators, social graph, settlements, positions, structures, connections, notices, waypoints, private mail, shared theories, and a bounded message log
- Multiplayer coverage with protocol clients, including restart persistence for friends/cabins/settlements/notices

## What this version does not do

No combat, economy, guild ranks, crafting, quests, or generated NPCs. Shared theories are not a forum: there are no replies, votes, or a moderator console. Waypoints do not teleport, share a map, or reveal unexplored ground. Settlements are membership groups, not map rectangles, and there is no settlement-wide remote chat. `mail` is a private letter that waits for an offline operator; `whisper` is still live and is not stored. The Squalor is a small deterministic urban wilderness, not a finished survival game. The subway slice is two stations, not a full metro. One account has one live session; a new login replaces the old link. Registration is open unless you pass `--no-registration`. Session tokens live in memory and are dropped on restart; the character, relationships, cabin, settlement, signs, and mailbox remain. See [security notes](docs/security.md) and [world architecture](docs/world-architecture.md).

## Next milestones

1. Shared build rights / workshops inside settlements
2. Public-key authentication alongside passwords
3. Closed registration with an invite flow
