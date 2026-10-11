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

World geography is code. Editing `undernet.rs` and restarting upserts names and replaces exits. Operator rows stay.

## Adding a location

1. Add the location, its objects, and both directions of every exit in `crates/coffee-world/src/undernet.rs`. Destinations must already exist. `go` keywords are unique per room.
2. Update `assets/world/implemented.graph` in the same change. The test `code_matches_the_world_bible_graph` fails if names, exits, objects, or listed files drift.
3. Write `assets/world/locations/<id>/reference.md`. Separate implemented behaviour from layout interpretation. Follow `assets/world/style-guide.md` before adding `concept.png` or `floorplan.png`, and list those files in the graph only once they exist.
4. Keep Coffee.shop art under `assets/world/`. Do not put it in `terminus-core` or the other engine crates.
5. If the in-game `map` cannot show the new edge, say so in `assets/world/world-overview.md` instead of pretending the schematic is complete.

Proposals (a vendor, a puzzle, a street door) stay in the location's unresolved questions. They do not go in `implemented.graph`.

## Adding a command

Register it in the game's `CommandRegistry` (Coffee.shop does this in `coffee-world/src/catalog.rs`) and handle the canonical name in the simulation. Do not add a new top-level string match in the server. The client learns usage from the hints on `GameView`.

## Adding a game

Implement `terminus_core::Game`. Render with `terminus_tui::run_local` or speak `terminus-protocol` if you want the SSH gateway. Do not import `coffee-world` unless you are extending Coffee.shop.

## Logging

`RUST_LOG=debug` raises the tracing filter. Do not log protocol bodies. The default filter is `info`.
