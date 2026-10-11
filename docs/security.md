# Security

This is a playable prototype. It is not a finished internet service. The notes below are the controls that exist, and the limits you should assume.

## No shell

The SSH server accepts a session channel and runs the game TUI on it. `exec`, subsystem, environment, and TCP forwarding requests are rejected. User input is a game command line. It is never passed to a shell, `std::process`, or the filesystem.

The unix protocol socket is created mode `0600`. It is how the in-process TUI and `coffee-server play` reach the hub. Do not expose it with a reverse proxy.

## Authentication

Passwords are hashed with Argon2id before they are stored. The server does not log passwords, password hashes, session tokens, or the SSH private key. Client-message `Debug` redacts passwords and tokens.

Usernames are canonicalised to lowercase. Operator names are a separate display string.

The host key is generated on first boot at `data/keys/ssh_host_ed25519` (mode `0600`) with a `.pub` file beside it. Nothing in the repository is a production host key, and there is no default administrative password.

SSH offers password authentication only. Failed attempts are limited per address (gateway) and per username (hub). Commands are limited to about 8 per second with a small burst. Auth replies are delayed so missing accounts do not return faster than a bad password.

## Registration and sessions

Registration is open unless the server is started with `--no-registration` or `COFFEE_NO_REGISTRATION=1`. Open registration lets anyone claim a username. That is acceptable for a local prototype and a poor default for a public host.

A new login replaces the previous session for that account. The old terminal receives `goodbye`. This is what makes reconnect work. It also means a stolen password disconnects the owner.

Tokens are random 128-bit values held in memory. A restart invalidates them. Identity and position are what survive.

## Data

State changes commit in a SQLite transaction. A failed commit rolls the in-memory operator, geography, social graph, and log back. The server does not rewrite every location on each command.

Frames are capped at 32 KiB. Session and connection counts are capped. Push queues drop when a client falls behind.

Player text (speech, bios, signs, notices, cabin names) is length-capped and stripped of control characters before storage or display. Building and noticeboard posting have soft rate limits beyond the global command bucket.

## Privacy and abuse resistance

- Private whispers use `Audience::Player` and `Channel::Private`. Their bodies are not published on location or global feeds. Offline whispers are refused; contents are not queued.
- `ignore`/`block` is enforced server-side across say, chat, emote, whisper, and friend requests.
- Friend lists expose online/offline only, never coordinates or account credentials.
- Cabin access (`public` / `friends` / `private` + grants) is checked when entering an interior, including bare `go <cabin>` through a stored connection.
- Noticeboard posts are bounded in length, rate, retained count, and lifetime.

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
