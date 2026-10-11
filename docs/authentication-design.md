# Authentication and onboarding

## What the code did before this change

SSH and the game were the same login.

1. The gateway (`terminus-ssh`) offered only password authentication. `auth_password` called `Hub::login` with the SSH username and the SSH password.
2. The full-screen TUI started only after that call returned a session token. The gateway then resumed the token over the local protocol socket. `exec`, subsystems, environment changes, and forwarding were already rejected.
3. `Hub` is the only place that attaches a session to an operator. A new login for the same character replaces the previous session and sends `goodbye`. Disconnect clears the in-memory token. A restart drops every token and reloads operators from SQLite.
4. Unknown SSH usernames were registered on the spot when registration was open. The operating-system username therefore became the game handle.
5. Accounts were `(id, username, password_hash, created_at)`. Passwords are Argon2id. There was no guest state, no recovery credential, and no email column.
6. Migrations run when the store opens. `0001_init.sql` created the account tables. `0002_world.sql` added procedural geography. Startup seeding still upserts the hand-authored map and does not touch accounts.
7. The protocol already had `hello`, `register`, `login`, `resume`, and `command`. The local `coffee-server play` client showed a username/password form. `Debug` redacts passwords and tokens.

The host key is still created once at `data/keys/ssh_host_ed25519` and reused.

## SSH constraint

Stock OpenSSH, including `ssh play.example.com`, always sends the local operating-system username. That name is not a game account. The same client also sends an initial `none` authentication probe before public-key or password methods.

russh 0.64 can accept that probe in `auth_none`. If the server accepts it, OpenSSH never asks for a password and opens a session channel. Accepting `none` is not a proof of identity. It only means the encrypted SSH channel is up and the host key was checked.

This server accepts `none` as a lobby and ignores the SSH username. The game account is chosen in the TUI, and the hub is still the only component that attaches an operator.

Password authentication remains for a client that asks for it directly (the integration test does). It checks an existing game password and does not create an account. Stock OpenSSH will not reach that method, because `none` succeeds first. Do not document a password on the SSH command line as the way to play.

Public-key login is not accepted. A future milestone can enroll a key from inside an authenticated session, then accept `auth_publickey` only for enrolled keys. Offering a key today is not an account.

## Player command

```bash
ssh -p 2222 -o StrictHostKeyChecking=accept-new play.example.com
```

Use the real host and the port you bound (`0.0.0.0:2222` by default). The local username OpenSSH sends is discarded. The welcome screen follows. There is still no operating-system shell.

`coffee-server play` opens the same welcome screen over the local protocol socket.

## Account model

The account id is a UUID. The operator id is a separate UUID. Neither is an email address, a display name, or an SSH username.

`0003_accounts.sql` adds columns. It does not rebuild the player tables.

| Column | Meaning |
| --- | --- |
| `kind` | `guest` or `permanent`. Existing rows become `permanent`. |
| `email` | Optional. Never a primary key. Unique when present. |
| `recovery_hash` | Argon2id verifier. Null when no recovery code is active. |
| `recovery_rotated_at` | Last time the verifier was written or cleared. |
| `password_changed_at` | Last time the password verifier was written. |

`password_hash` stays non-null so the original table constraint remains. A guest's password hash is a throwaway Argon2id hash of a random secret the player never sees. Password login against a guest fails the same way a wrong password fails.

## Flows

**Guest.** The player picks a handle and an operator name. No email. The hub inserts the account and the starter operator in one transaction, attaches one session, and returns a recovery code. The code is shown once. Enter continues into the Coffee Shop. Disconnecting before submit writes nothing.

**Returning.** A guest has no password. They come back with option 4 and the recovery code. The same operator id, location, inventory, and flags load. The old code is rotated; the new one is shown once. They can set a password from that screen, which upgrades the guest, or enter the world and keep the new code.

**Permanent registration.** Handle, password, confirmation, operator name, optional email. A recovery code is issued. Email may be left blank. The database does not require it.

**Login.** Handle and password, checked with the existing Argon2id verifier. Missing accounts run a dummy verification. The SSH password path uses this same hub method and no longer auto-registers.

**Upgrade.** From the account panel (`account` at the prompt) a guest sets a password and may set an email. The account id and operator id do not change. A failed update rolls back; the guest row stays a guest.

**Rotate and revoke.** Rotation replaces the verifier and shows the new code once. Revoke clears the verifier. A guest cannot revoke, because the code is their only way back. A permanent account with no recovery code and no remembered password cannot be restored. Stored email is not a mail channel; this server does not send mail.

## Recovery code

24 characters from a 32-symbol alphabet (`23456789ABCDEFGHJKLMNPQRSTUVWXYZ`), shown as six groups of four. 256 modulo 32 is zero, so each symbol is unbiased. That is 120 bits, drawn from the operating-system generator (`rand::rngs::SysRng`).

The raw code is returned to the client that just created or rotated it. It is not written to SQLite, the message log, or tracing. The column stores only the Argon2id hash of the canonical form (uppercase, hyphenated). Typed input may omit hyphens and spaces. Wrong codes, unknown handles, and revoked codes all answer `invalid username or recovery code`. Recovery attempts are limited to 5 per handle and 30 globally, each inside a 10-minute window.

## Sessions

Tokens stay in hub memory (two UUIDs, 256 bits of token text). Restart invalidates tokens and does not invalidate accounts, recovery verifiers, or characters. One character still has one live session; a new attach replaces the old link.

The client never chooses an account id or an operator id. The hub loads them from the authenticated row.

## Registration switch

`--no-registration` or `COFFEE_NO_REGISTRATION=1` refuses guest creation and permanent registration. Login and recovery of existing accounts still work.

`account create` still writes a permanent account from `COFFEE_PASSWORD`. It prints a recovery code to the terminal once and does not log it.
