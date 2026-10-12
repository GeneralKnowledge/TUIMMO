# Security

This is a playable prototype. It is not a finished internet service. The notes below are the controls that exist, and the limits you should assume.

## No shell

The SSH server accepts a session channel and runs the game TUI on it. `exec`, subsystem, environment, and TCP forwarding requests are rejected. User input is a game command line. It is never passed to a shell, `std::process`, or the filesystem.

The unix protocol socket is created mode `0600`. It is how the in-process TUI and `coffee-server play` reach the hub. Do not expose it with a reverse proxy.

## Authentication

Passwords are hashed with Argon2id before they are stored. The server does not log passwords, password hashes, session tokens, or the SSH private key. Client-message `Debug` redacts passwords and tokens.

Usernames are canonicalised to lowercase. Operator names are a separate display string.

The host key is generated on first boot at `data/keys/ssh_host_ed25519` (mode `0600`) with a `.pub` file beside it. Nothing in the repository is a production host key, and there is no default administrative password.

SSH offers password authentication only. Failed attempts are limited per address (gateway) and per username (hub). Each live session has its own command bucket: 20 commands may arrive at once, then the bucket refills at 8 per second, up to that same burst. A command past the limit is rejected with "You are sending commands too quickly." It is not queued, delayed, or applied, and the terminal keeps the previous view with that notice on the prompt. Another operator's bucket is separate. Disconnecting drops the bucket; the next login starts full. Auth replies are delayed so missing accounts do not return faster than a bad password. The limit is left at these numbers because a person walking and reading stays inside them; a script that sends the 21st command in the same instant is what the burst is for.

## Registration and sessions

Registration is open unless the server is started with `--no-registration` or `COFFEE_NO_REGISTRATION=1`. Open registration lets anyone claim a username. That is acceptable for a local prototype and a poor default for a public host.

A new login replaces the previous session for that account. The old terminal receives `goodbye`. This is what makes reconnect work. It also means a stolen password disconnects the owner.

Tokens are random 128-bit values held in memory. A restart invalidates them. Identity and position are what survive.

## Data

State changes commit in a SQLite transaction. A failed commit rolls the in-memory operator, geography, social graph, settlements, and log back. The server does not rewrite every location on each command.

Frames are capped at 32 KiB. Session and connection counts are capped. Push queues drop when a client falls behind.

Player text (speech, bios, signs, notices, cabin names) is length-capped and stripped of control characters before storage or display. Building and noticeboard posting have soft rate limits beyond the global command bucket.

## Privacy and abuse resistance

- Private whispers use `Audience::Player` and `Channel::Private`. Their bodies are not published on location or global feeds. Offline whispers are refused; contents are not queued.
- Regional chat uses `Audience::Region` with a server-derived region key. Clients cannot subscribe to arbitrary regions.
- `ignore`/`block` is enforced server-side across say, chat, region, emote, whisper, friend requests, and settlement invites.
- Friend lists expose online/offline only, never coordinates or account credentials.
- Cabin/workshop visitor access (`public` / `friends` / `private` + grants) and settlement share (`private` / `shared` / `communal`) are checked when entering an interior, including bare `go <name>` through a stored connection. All entry paths use the same server-side precedence.
- Players cannot grant themselves sharing rights over others' structures. Settlement membership alone never opens a private structure.
- Settlement membership is one per player. Private settlements are not listed to outsiders. Disband never deletes cabins, workshops, or signs; it clears links and reverts settlement share to private.
- Invitations are addressed to a specific invitee. `settle invites` only lists that player's pending invites. Expired invites cannot be accepted.
- Noticeboard posts (Coffee Shop, settlement, and workshop) are bounded in length, rate, retained count, and lifetime.

## What you should still do

- Read the host key fingerprint before accepting it (`ssh-keygen -lf data/keys/ssh_host_ed25519.pub`).
- Put SSH behind a firewall. The default bind is `0.0.0.0:2222`.
- Turn registration off once the accounts you want exist.
- Keep `data/` off world-readable backups if it contains password hashes. Hashes are still secrets.
- Treat the protocol socket as local-only.

## Known prototype limits

- No public-key login, second factor, or account recovery.
- No offline private-message mailbox; whispers require an online recipient.
- Display-name collisions are refused at resolve time rather than prevented at registration.
- No moderation console beyond owner-delete for notices and owner cabin controls.
- No audit log beyond the world message table (which may retain recent private lines up to the log cap).
- The SSH implementation is russh. Track its advisories.
- Test builds of `coffee-world` use lighter Argon2 parameters. The server binary uses the crate defaults. Hashes produced by `cargo test` inside `coffee-world` are not interchangeable with the server.
- This code has not had an external security review.
