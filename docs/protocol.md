# Protocol

Version 1. The TUI and the integration tests speak this protocol. SSH clients do not send these frames themselves. The gateway's in-process TUI does, after the lobby opens or after an explicit SSH password login of an existing account.

## Trust boundary

Every byte is untrusted. The codec rejects:

- frames larger than 32 KiB
- a decoder buffer above 64 KiB
- a version other than 1
- JSON that fails the message schema, including unknown fields

The hub checks the session token, rate-limits commands, and asks `WorldState` to validate the line again. A well-formed frame is not permission to change the world.

Passwords, recovery codes, emails, and resume tokens are credentials. `Debug` for client messages redacts them. The server does not log frame bodies. A recovery code appears in an `authenticated` or `account_status` frame only for the client that just created or rotated it.

## Framing

```text
| u32 big-endian length | UTF-8 JSON |
```

The JSON object is:

```json
{ "v": 1, "id": 7, "body": { "type": "command", "line": "look" } }
```

`id` 0 is a server push. Responses copy the request id. Outgoing queues are bounded. A slow client loses pushes instead of growing memory without limit.

## Client messages

| type | When |
| --- | --- |
| `hello` | First message. `client` is a short name. |
| `guest` | Create a guest. `username`, `operator_name`. No email. |
| `register` | Create a permanent account. `username`, `password`, `operator_name`, optional `email`. |
| `login` | Existing permanent account. |
| `recover` | `username` and `recovery_code`. Rotates the code on success. |
| `resume` | Bind an event stream to a token the SSH gateway already issued. |
| `set_password` | Authenticated permanent account. |
| `upgrade` | Authenticated guest. `password`, optional `email`. Same account and operator. |
| `rotate_recovery` | Authenticated. Replaces the recovery verifier. |
| `revoke_recovery` | Authenticated permanent account. Clears the verifier. |
| `account_info` | Authenticated. Current handle, kind, and whether a code or email is stored. |
| `command` | One line, parsed by the server registry. |
| `ping` | `nonce` echoed by `pong`. |

`hello` is required before anything else. `guest`, `register`, `login`, `recover`, and `resume` are refused once the link already has a token. The account commands require one.

## Server messages

| type | Meaning |
| --- | --- |
| `hello` | `version` and a short notice. |
| `authenticated` | Token, operator name, and the first `GameView`. Optional `username`, `account_kind`, and `recovery_code`. |
| `update` | Reply to a command. `quit` asks the client to close. |
| `snapshot` | Push of another session's current view. |
| `events` | Feed lines for clients that do not render full snapshots. |
| `account_status` | Handle, kind, recovery and email flags, a short message, and a new code when one was just issued. |
| `error` | `code` and a message safe to show. |
| `pong` | Reply to `ping`. |
| `goodbye` | The session was replaced or the server is closing it. |

The reference TUI treats snapshots as authoritative and ignores `events`. Both are sent so a simpler client can follow the feed.

## Sessions

A token is 64 hex characters (two UUIDs), stored only in the hub. Restarting the server drops every token. The account, operator, and location remain. A permanent account uses `login`. A guest uses `recover`.

One account has one simulation session. A new login replaces the previous token and sends `goodbye` on the old stream. Disconnect is idempotent: the SSH task and the protocol task may both request it.
