# Security

This is a playable prototype. It is not a finished internet service. The notes below are the controls that exist, and the limits you should assume.

## No shell

The SSH server accepts a session channel and runs the game TUI on it. `exec`, subsystem, environment, and TCP forwarding requests are rejected. User input is a game command line. It is never passed to a shell, `std::process`, or the filesystem.

The unix protocol socket is created mode `0600`. It is how the in-process TUI and `coffee-server play` reach the hub. Do not expose it with a reverse proxy.

## Authentication

The SSH layer and the game account are separate. `none` authentication opens the encrypted lobby and is not an identity. The operating-system username is discarded. Password authentication is still advertised for a client that requests it directly; that path checks an existing permanent password and does not create an account. Stock OpenSSH will not reach it, because the `none` probe succeeds first. Public keys are rejected until a later milestone enrolls them from inside a session.

Passwords and recovery codes are hashed with Argon2id before they are stored. The raw recovery code is shown once, when it is created or rotated, and is not written to SQLite, tracing, or session records. The server does not log passwords, recovery codes, password hashes, session tokens, or the SSH private key. Client-message `Debug` redacts passwords, recovery codes, emails, and tokens.

Usernames are canonicalised to lowercase. Operator names are a separate display string. Email, when stored, is lowercase and unique, and is never the account key. This server does not send mail, so an address is not a recovery channel.

The host key is generated on first boot at `data/keys/ssh_host_ed25519` (mode `0600`) with a `.pub` file beside it. Nothing in the repository is a production host key, and there is no default administrative password.

Failed password attempts are limited per address (gateway) and per username (hub, 8 per minute). Recovery attempts are limited to 5 per handle and 30 globally inside a 10-minute window. Unknown handles, revoked codes, and wrong codes share the message `invalid username or recovery code`, after a dummy Argon2 check when the code is well formed. Commands are limited to about 8 per second with a small burst.

## Registration and sessions

Registration is open unless the server is started with `--no-registration` or `COFFEE_NO_REGISTRATION=1`. When it is closed, guest creation and permanent registration stop. Login and recovery of accounts that already exist still work. Open registration lets anyone claim a username. That is acceptable for a local prototype and a poor default for a public host.

A guest has no password the player knows. Their way back is the recovery code. Setting a password upgrades that same account id and operator id. A guest cannot revoke the code. A permanent account can. An account with neither a remembered password nor a saved recovery code cannot be restored.

A new login replaces the previous session for that character. The old terminal receives `goodbye`. This is what makes reconnect work. It also means a stolen password or recovery code disconnects the owner. Recovery rotates the code immediately, so the previous code stops working.

Tokens are two random UUIDs held in memory. A restart invalidates them. Accounts, recovery verifiers, and positions survive. The client never supplies an account id or an operator id.

## Data

State changes commit in a SQLite transaction. A failed commit rolls the in-memory operator and log back. The server does not rewrite every location on each command.

Frames are capped at 32 KiB. Session and connection counts are capped. Push queues drop when a client falls behind.

## What you should still do

- Read the host key fingerprint before accepting it (`ssh-keygen -lf data/keys/ssh_host_ed25519.pub`).
- Put SSH behind a firewall. The default bind is `0.0.0.0:2222`.
- Turn registration off once the accounts you want exist.
- Keep `data/` off world-readable backups if it contains password or recovery hashes. Hashes are still secrets.
- Treat the protocol socket as local-only.
- Tell players to store the recovery code somewhere that is not the game. It is shown once.

## Known prototype limits

- No public-key login and no second factor. Public-key enrollment is a documented extension, not an accepted method.
- Stored email is not verified and cannot reset an account.
- Guest rows are not expired or deleted.
- No audit log beyond the world message table.
- The SSH implementation is russh. Track its advisories.
- Test builds of `coffee-world` use lighter Argon2 parameters. The server binary uses the crate defaults. Hashes produced by `cargo test` inside `coffee-world` are not interchangeable with the server.
- This code has not had an external security review.
