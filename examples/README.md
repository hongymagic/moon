# Public IPC extension

`workspace-state.ps1` is a standalone extension using .NET named pipes. It does not link moon's code or invoke the moon CLI. Run it with `-Once` for a single workspace summary, or without arguments to subscribe to commits:

```powershell
powershell.exe -NoProfile -File examples\workspace-state.ps1 -Once
```

The pipe is `\\.\pipe\moon-v1-HIGH-LOW`, where HIGH and LOW are the eight-digit hexadecimal halves of the token authentication LUID. The pipe ACL uses the logon SID, and the mandatory integrity label is medium. No network transport is supported.

Each UTF-8 JSON frame ends in a newline. Requests are at most 64 KiB; responses at most 8 MiB. Each connection sends one request. A normal request receives one response and closes; subscriptions receive an initial world followed by committed worlds until disconnected. Slow subscriptions are disconnected so they cannot delay input or retain an unbounded queue.

```json
{"version":1,"id":1,"method":"query","query":"world"}
{"version":1,"id":2,"method":"command","commands":["focus left"]}
{"version":1,"id":3,"method":"subscribe"}
```

Replies contain `version`, the same `id`, `ok`, `result`, `error`, and the committed `revision`. Queries accept `world`, `windows` (desired placements), `monitors`, `config`, `transactions`, and `stats`. Command arrays execute in order and stop on an error; earlier successful commands remain committed. The grammar is shared with bindings, rules, aliases and widget clicks.

`moon protocol schema` prints the generated request/response schema. The integration suite runs this extension as a separate protocol client. Windows titles are available to explicitly connected clients through the world; raw keys are never exposed.

## Guarded commands

Automation can include `expected_focus` on a command request, for example
`{"version":1,"id":2,"method":"command","commands":["close"],"expected_focus":12345}`.
The daemon rejects the entire request if its focused window differs when the
request executes. This prevents an inspection followed by `close` from acting
on a different window after focus changes. Omit the field for ordinary commands.
