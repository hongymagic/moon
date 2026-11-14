# Public IPC extension

`workspace-state.ps1` is a standalone extension using .NET named pipes. It does not link moon's code or invoke the moon CLI. Run it with `-Once` for a single workspace summary, or without arguments to subscribe to commits:

```powershell
powershell.exe -NoProfile -File examples\workspace-state.ps1 -Once
```

The pipe is `\\.\pipe\moon-v1-S-1-5-5-X-Y`, using the actual `TokenLogonSid` returned by Windows. This identifier is shared by the ordinary and elevated tokens of the same sign-in session; their authentication LUIDs can differ. The pipe ACL uses that same logon SID, and the mandatory integrity label remains medium. No network transport is supported. Microsoft documents this SID as [unique to each logon session](https://learn.microsoft.com/en-us/windows/win32/secauthz/well-known-sids).

Each UTF-8 JSON frame ends in a newline. Requests are at most 64 KiB; responses at most 8 MiB. Each connection sends one request. A normal request receives one response and closes; subscriptions receive an initial world followed by committed worlds until disconnected. Slow subscriptions are disconnected so they cannot delay input or retain an unbounded queue.

```json
{"version":1,"id":1,"method":"query","query":"world"}
{"version":1,"id":2,"method":"command","commands":["focus left"]}
{"version":1,"id":3,"method":"subscribe"}
```

Replies contain `version`, the same `id`, `ok`, `result`, `error`, and the committed `revision`. Queries accept `world`, `windows` (desired placements), `monitors`, `config`, `transactions`, `stats`, `shortcuts`, and `overview`. Command arrays execute in order and stop on an error; earlier successful commands remain committed. The grammar is shared with bindings, rules, aliases and widget clicks.

`shortcuts` returns `{mode, paused, hook_enabled, bindings: [{chord, commands}], emergency}`.
It uses the same resolver as the live keyboard hook: modes replace normal bindings,
paused operation keeps the recovery commands, and disabled bindings are omitted.
It reflects the daemon's committed configuration even when the saved file is invalid.
`moon cheatsheet` opens this help in Moon Settings. When the companion is absent
from a standalone copy, it opens readable text help with the same bindings and session status.
`hook_enabled` is false when this session started with `--no-hook` or `--only`; the listed bindings remain configured, while the independent emergency recovery shortcut remains available.
`moon query shortcuts` also works without the Settings companion.

`overview` returns `{managed_windows, active_workspace, workspaces: [{name,
windows, active}]}`. Counts include hidden and suspended managed windows. Each
monitor can have an active workspace; `active_workspace` identifies the active
monitor's selection. The summary contains no window titles or process identities
and describes current membership, not a historical startup event. Both additions
use the existing version-1 query envelope; the request/response schema is unchanged.

`moon protocol schema` prints the generated request/response schema. The integration suite runs this extension as a separate protocol client. Windows titles are available to explicitly connected clients through the world; raw keys are never exposed.

## Guarded commands

Automation can include `expected_focus` on a command request, for example
`{"version":1,"id":2,"method":"command","commands":["close"],"expected_focus":12345}`.
The daemon rejects the entire request if its focused window differs when the
request executes. This prevents an inspection followed by `close` from acting
on a different window after focus changes. Omit the field for ordinary commands.
