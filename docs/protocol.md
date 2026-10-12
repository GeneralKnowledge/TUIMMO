# Protocol

Version 1. The TUI and the integration tests speak this protocol. SSH clients do not send these frames themselves; the gateway's in-process TUI does, after password authentication.

## Trust boundary

Every byte is untrusted. The codec rejects:

- frames larger than 32 KiB
- a decoder buffer above 64 KiB
- a version other than 1
- JSON that fails the message schema, including unknown fields

The hub checks the session token, rate-limits commands, and asks `WorldState` to validate the line again. A well-formed frame is not permission to change the world.

Passwords and resume tokens are credentials. `Debug` for client messages redacts them. The server does not log frame bodies.

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
| `register` | Create an account. `username`, `password`, `operator_name`, optional `invite_code`. |
| `login` | Existing account. |
| `resume` | Bind an event stream to a token the SSH gateway already issued. |
| `command` | One line, parsed by the server registry. |
| `ping` | `nonce` echoed by `pong`. |

`hello` is required before anything else.

## Server messages

| type | Meaning |
| --- | --- |
| `hello` | `version` and a short notice. |
| `authenticated` | Token, operator name, and the first `GameView`. |
| `update` | Reply to a command. `quit` asks the client to close. |
| `snapshot` | Push of another session's current view. |
| `events` | Feed lines for clients that do not render full snapshots. |
| `error` | `code` and a message safe to show. |
| `pong` | Reply to `ping`. |
| `goodbye` | The session was replaced or the server is closing it. |

The reference TUI treats snapshots as authoritative and ignores `events`. Both are sent so a simpler client can follow the feed.

`GameView` may include `finding`, the latest inspection for that session, and `overlay`, for help, the arrival note, the map, the journal, a shared theory, a waypoint list, or a mailbox. Both are optional. Older snapshots that omit `finding` are read as empty. The place text stays in `description`. Inspection is not a new feed channel: the private system line is still recorded, and `finding` is what the terminal keeps on screen. Waypoint notes, mail bodies, and theory text travel in that session's overlay, not in a shared feed. A mail or theory notice on the feed names the id and not the body.

## Sessions

A token is 64 hex characters, stored only in the hub. Restarting the server drops every token. The account, operator, and location remain, so the next `login` restores them.

One account has one simulation session. A new login replaces the previous token and sends `goodbye` on the old stream. Disconnect is idempotent: the SSH task and the protocol task may both request it.
