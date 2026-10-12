# Development

## Toolchain

`rust-toolchain.toml` selects Rust 1.99.0 with rustfmt and clippy. Dependencies are pinned by `Cargo.lock`.

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`cargo test` covers command parsing, Undernet rules, the SQLite store, the frame codec, SSH connection limits, two simulated clients, and an SSH session that must render the game.

`examples/minimal-game` prints a scripted walk when stdout is not a terminal:

```bash
cargo run -p minimal-game -- --script
```

Confirm it stays independent of the reference game:

```bash
cargo tree -p minimal-game -i coffee-world
```

That command should report that `coffee-world` is not a dependency.

## Database

The server creates `data/coffee.sqlite` on startup and applies migrations embedded in `coffee-world`. To begin again, stop the server and remove the `data/` directory. The next start generates a new host key. Tell players the new fingerprint.

You do not run a separate migration command. Opening the store applies pending migrations.

Hand-authored geography is code. Editing `undernet.rs` and restarting upserts names and replaces the static exit rows. Operator rows stay. Procedural regions, markers, terrain modifications, discoveries (visits and field notes), and world connections are not part of that reseed. See [world architecture](world-architecture.md).

## Adding a command

Register it in the game's `CommandRegistry` (Coffee.shop does this in `coffee-world/src/catalog.rs`) and handle the canonical name in the simulation. Do not add a new top-level string match in the server. The client learns usage from the hints on `GameView`.

## Adding a game

Implement `terminus_core::Game`. Render with `terminus_tui::run_local` or speak `terminus-protocol` if you want the SSH gateway. Do not import `coffee-world` unless you are extending Coffee.shop.

## Logging

`RUST_LOG=debug` raises the tracing filter. Do not log protocol bodies. The default filter is `info`.
