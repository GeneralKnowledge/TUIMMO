# Terminus + Coffee.shop

Terminus is a small Rust engine for multiplayer terminal games. Coffee.shop is the reference game: a persistent cyberpunk world called the Undernet. Players connect with SSH, get a full-screen terminal UI, and share one authoritative world.

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

Open a second SSH session as someone else. You can `look`, `go alley`, `say` in the room, and `chat` to the whole Undernet. `help` lists the commands.

Stop the server with Ctrl-C and start it again. Both operators are still where they were.

A local TUI, without SSH, can attach to a running server:

```bash
cargo run -p coffee-server -- play
```

Create an account without putting the password in the process list:

```bash
COFFEE_PASSWORD='a-long-secret' cargo run -p coffee-server -- account create --username ada --operator-name Ada
```

## Layout

| Crate | Role |
| --- | --- |
| `terminus-core` | Commands, sessions, views, and the `Game` trait |
| `terminus-tui` | Ratatui layout, input, and the remote client loop |
| `terminus-protocol` | Versioned length-prefixed JSON frames |
| `terminus-ssh` | SSH gateway. Auth starts a game session, never a shell |
| `coffee-world` | Undernet rules and SQLite persistence |
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
- [Undernet](docs/game-design.md)
- [Security](docs/security.md)
- [Development](docs/development.md)
- [Deployment](docs/deployment.md)

## What this version does

- SSH into a TUI, with password authentication and a generated host key
- Register or log in, and resume the same operator after a restart
- Header, location, status, scrollable feed, and a command prompt with history and completion
- `help`, `look`, `map`, `who`, `go`, `say`, `chat`, `status`, `inventory`, `inspect`, `mark`, `quit`
- Ten linked Undernet locations, including a sealed Black Archive
- A generated outskirts beyond the Roof Array: deterministic regions, cardinal movement, and a way back with `city`
- Local and global chat, join and leave notices, movement notices
- SQLite persistence for accounts, operators, positions, the hand-authored map, player markers, and a bounded message log
- Two simultaneous sessions, tested with protocol clients and an SSH client

## What this version does not do

No combat, economy, factions, crafting, quests, homesteads, settlements, or generated NPCs. The outskirts are a small deterministic wilderness, not a finished survival game. One account has one live session; a new login replaces the old link. Registration is open unless you pass `--no-registration`. Session tokens live in memory and are dropped on restart; the character, position, and anything they marked remain. See [security notes](docs/security.md) and [world architecture](docs/world-architecture.md).

## Next milestones

1. Player-built cabins and homesteads as owned structures with interiors
2. Trails, settlements, and permissions for shared building
3. Public-key authentication alongside passwords
4. Closed registration with an invite flow
