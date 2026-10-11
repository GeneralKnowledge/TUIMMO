# Deployment

One machine, one process, one SQLite file. There is no Redis, no extra database, and no web frontend.

## Build

```bash
cargo build --release -p coffee-server
```

The binary is `target/release/coffee-server`.

## Run

```bash
export COFFEE_DATA_DIR=/var/lib/coffee
export COFFEE_SSH_LISTEN=0.0.0.0:2222
export COFFEE_NO_REGISTRATION=1   # after the accounts exist
./coffee-server serve
```

First boot creates:

- `/var/lib/coffee/coffee.sqlite` — accounts and world state
- `/var/lib/coffee/keys/ssh_host_ed25519` — private host key, mode `0600`
- `/var/lib/coffee/keys/ssh_host_ed25519.pub` — public host key
- `/var/lib/coffee/game.sock` — local protocol socket, mode `0600`

Print the fingerprint and keep it:

```bash
ssh-keygen -lf /var/lib/coffee/keys/ssh_host_ed25519.pub
```

## Players

```bash
ssh -p 2222 -o StrictHostKeyChecking=accept-new your.host
```

Compare the fingerprint with the one you printed. After the first connection, leave host-key checking at its default so a changed key is a loud failure.

The SSH username is ignored. The welcome screen is where a player enters as a guest, logs in, registers, or recovers. There is no host shell and no SSH password prompt for a normal OpenSSH client. Game passwords and recovery codes are typed inside the TUI, and password fields are not echoed.

An existing database gains guest, email, and recovery columns the first time this version opens it (`0003_accounts.sql`). Previous accounts become permanent accounts and keep their operators, positions, and passwords. Copy `coffee.sqlite` before the first start of a new version if you want a rollback point. The migration adds columns and an email index. It does not rebuild player tables.

`--no-registration` or `COFFEE_NO_REGISTRATION=1` stops new guests and new permanent accounts. People who already have an account can still log in or recover.

## Firewall

Allow TCP 2222 from the networks that should play. Do not publish `game.sock`. The directory should be readable only by the service user.

## Restart

Stop the process (Ctrl-C, or a service stop that sends SIGINT). Start it again with the same `COFFEE_DATA_DIR`. Operators return to their saved rooms. In-memory session tokens do not survive. A permanent account logs in with its password. A guest returns through recovery, which issues a new code.

Copy `coffee.sqlite` (and the `-wal` file if it exists) only while the server is stopped, or use SQLite's backup command. Keep the host key with the data directory if you want clients to recognise the server.

## Process supervision

A systemd unit can be `ExecStart` on the release binary, `Restart=on-failure`, and `User=` set to a dedicated account that owns `COFFEE_DATA_DIR`. The working directory does not matter when `COFFEE_DATA_DIR` is absolute.

## Limits

`--max-sessions` caps simultaneous SSH connections (default 64). Each address is also limited to 4 connections. Raise these only when you know the host can afford the Argon2 logins.
