# Public IPC extension

`workspace-state.ps1` is a standalone extension using .NET named pipes. It does not link moon's code or invoke the moon CLI. Run it with `-Once` for a single workspace summary, or without arguments to subscribe to commits:

```powershell
powershell.exe -NoProfile -File examples\workspace-state.ps1 -Once
```

The pipe is `\\.\pipe\moon-v1-S-1-5-5-X-Y`, using the actual `TokenLogonSid` returned by Windows. This identifier is shared by the ordinary and elevated tokens of the same sign-in session; their authentication LUIDs can differ. The pipe ACL uses that same logon SID, and the mandatory integrity label remains medium. No network transport is supported. Microsoft documents this SID as [unique to each logon session](https://learn.microsoft.com/en-us/windows/win32/secauthz/well-known-sids).

The base pipe and slots with suffixes `-1` through `-15` share the same protocol. Try another slot when one is busy. The example requires a maximum of one server instance per name and checks the connected server's executable before sending its request. Use `-DaemonPath` for a development executable outside the default `moon-dev` directory. Clients should use identification-only impersonation and check the server identity independently of the reply contents. [Windows named-pipe access rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)

Setup uses the `lifecycle` method with an `action` of `restart` or `exit` to control an authenticated daemon. These actions work independently of the user's command aliases.

Each UTF-8 JSON frame ends in a newline. Requests are at most 64 KiB; responses at most 8 MiB. Each connection sends one request. A normal request receives one response and closes; subscriptions receive an initial world followed by committed worlds until disconnected. Slow subscriptions are disconnected so they cannot delay input or retain an unbounded queue.

```json
{"version":1,"id":1,"method":"query","query":"world"}
{"version":1,"id":2,"method":"command","commands":["focus left"]}
{"version":1,"id":3,"method":"subscribe"}
```

Replies contain `version`, the same `id`, `ok`, `result`, `error`, and the committed `revision`. Queries accept `world`, `windows` (desired placements), `monitors`, `config`, `transactions`, `stats`, `restart-config`, `shortcuts`, `overview`, `desktop`, and `undo`. Command arrays execute in order and stop on an error; earlier successful commands remain committed. The grammar is shared with bindings, rules, aliases and widget clicks.

`restart-config` is a read-only setup preflight. It parses the current file with the daemon’s original explicit/default selection and returns the same Config JSON shape as `world.config`; it does not apply the file. Invalid or missing explicitly selected files return an error. Sessions started with `--only`, `--no-hook` or `--no-surfaces` are refused because setup does not preserve those diagnostic options. Setup compares this result with its captured committed config and separately rechecks file identity before shutdown.

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
The daemon rejects the entire request unless both its committed focused window
and the current native foreground window match when the request executes. This prevents an inspection followed by `close` from acting
on a different window after focus changes. Omit the field for ordinary commands.

## Desktop overview and layout Undo

`moon overview` (also `window-search`, default Alt+Ctrl+Space) opens the searchable
Overview page in Moon Settings. `query desktop` returns committed managed windows,
workspace monitor bounds, planned window rectangles, state flags, and current rule
matches. Hidden-workspace rectangles describe their intended layout, rather than
the off-screen location used to hide a native window. Rule matches use current
process/class/title metadata; they are not a historical assignment log. Ignored
and unmanaged windows are not included.

The catalog contains `generation`, `revision`, `paused`, `session_suspended`,
`active_workspace`, `undo`, `total_windows`, `truncated`, `workspaces`, and `windows`.
Each window has `id`, `process_id`, `process_birth`, `title`, `process`, `class`,
`workspace`, `monitor_id`, `rect`, `state`, and `rules`. Identity fields and the
catalog revision are decimal strings to preserve their exact values in JavaScript.
The catalog is bounded to 1 MiB; `truncated` reports omitted windows or shortened
display text/rule explanations. Workspace names retain their exact spelling.

Copy the catalog's generation, revision, and selected window identity into a typed
request. These capabilities do not accept arbitrary commands:

```json
{"version":1,"id":4,"method":"window","generation":"100:134391580679147114","revision":"42","target":{"id":"12345","process_id":"200","process_birth":"134391580679147115"},"action":{"type":"focus"}}
{"version":1,"id":5,"method":"window","generation":"100:134391580679147114","revision":"42","target":{"id":"12345","process_id":"200","process_birth":"134391580679147115"},"action":{"type":"move-to-workspace","workspace":"2","follow":false}}
{"version":1,"id":6,"method":"undo","generation":"100:134391580679147114","revision":"43"}
{"version":1,"id":7,"method":"control","generation":"100:134391580679147114","revision":"44","action":"pause"}
```

Window actions are `focus`, `float`, `tile`, and `move-to-workspace` with an explicit
`follow` boolean. Focus activates the window's containing workspace. The CLI also
supports `moon window ID focus|float|tile` and
`moon window ID move-to-workspace NAME [--follow]`; it obtains the current guard
tokens before sending the typed request. Controls are `pause` (toggle), `reload`,
and `restore` (restore original windows and stop Moon). Successful typed requests
return `null`; refresh the catalog afterwards. Stale generation/revision or native
window/process lifetime changes reject the request. Paused, minimized, off-desktop,
and suspended-session windows cannot be changed through these actions. Controls
remain available while paused but require an interactive Windows session.

`query undo` returns `{available, label, reason}`. `moon undo` (default Alt+Ctrl+Z)
restores the immediately preceding deliberate move, resize, split, float/tile,
fullscreen, or monitor/workspace-assignment change. It restores layout and focus,
not application contents, and retains no persistent history. Managed-window
availability, configuration, monitor, or native-fullscreen changes invalidate the
checkpoint; closing or launching an application also clears it. Opening an ignored
Settings window does not. Every affected native process lifetime is checked again
before applying Undo, and the checkpoint is consumed after one successful commit.
