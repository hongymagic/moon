# Settings editing contract

The settings window edits the user's TOML overlay through
`moon_core::settings`. It does not serialize the entire effective configuration
back over personal settings. `Snapshot::load` returns the exact optional source
text, effective configuration, personal overrides, compiled defaults and editor
schema. An absent file and an empty file have different concurrency tokens.

Configurations are limited to 1 MiB, including the exact final saved bytes.
The edit protocol uses explicit paths:

```json
{"op":"set","path":["bar","height"],"value":44}
{"op":"reset","path":["bindings","alt+h"]}
{"op":"set","path":["workspaces",0,"icon"],"value":"⌂"}
```

`prepare_update(current, expected, edits)` checks exact source equality, rejects
an invalid original configuration, applies the complete batch, and validates the
result before returning any replacement text. A batch may change interdependent
values together. Errors include a stable code, readable message, path where
available, source line/column and recovery help. Configuration migration remains
an explicit CLI operation; settings never silently migrate or repair a bad file.

Tables and bindings inherit defaults by key. Other lists replace their default
list, except rules, which append personal entries to built-in rules. Rule indices
therefore refer only to `snapshot.overrides.rules`; built-in rules are displayed
separately. Editing a row of an inherited replacement list materializes that list
while preserving its other rows. Reset removes an override: resetting an entire
list restores its defaults, while resetting an optional field of an explicit row
removes that field. Removing a row requires setting the resulting list. Direct
null values require Reset; nullable properties inside complete JSON row objects
are omitted from TOML.

Edits preserve untouched formatting and comments. Workspace/monitor names and
widget IDs keep comments attached to their rows during reordering. No-op saves
retain the original bytes, and uniformly CRLF documents retain CRLF line endings.
The pinned `toml_edit` library documents limitations around dotted-key ordering
and missing final newlines; edited documents may acquire a final newline.

`moon_settings::store::Store` owns filesystem effects. Saving holds a sidecar OS
file lock shared by cooperating settings instances, reads the current source,
prepares the validated change, creates a unique temporary file in the same
directory, copies existing file metadata/streams, writes and syncs the temporary
file, then rereads the exact source before replacing the destination with a
rename. It never truncates the live configuration. Invalid changes and observed
external edits leave that file untouched. The sidecar remains on disk so waiting
processes keep locking the same file identity. Read-only files are rejected.

The final check and rename are separate OS operations: an editor that ignores
the advisory lock can race between them. A synced temporary file is not a promise
of power-loss durability for the final directory entry or every filesystem.
On Windows 11, Rust's copy operation uses CopyFileEx, retaining NTFS streams and
security properties; its rename replaces an existing destination using the
platform implementation. Unix permission bits are preserved as well.

The portable tests exercise comments, resets, inherited lists, personal rules,
nullable row fields, batch validation, conflicts, concurrent saves, temporary
cleanup and permission preservation. Browser tests exercise the bundled UI with
only its native IPC bridge simulated; they do not establish native WebView2 or
live daemon behavior. Their defaults fixture was generated from
`moon --config /dev/null config show`, without reading personal preferences.

Current official references checked on 12 November 2026:

- [toml_edit documentation](https://docs.rs/toml_edit/latest/toml_edit/) and
  [registry version metadata](https://crates.io/api/v1/crates/toml_edit): pinned
  `0.25.19+spec-1.1.0`, latest stable at the check, published 6 November 2026.
- [Rust File locking and sync](https://doc.rust-lang.org/std/fs/struct.File.html),
  [copy](https://doc.rust-lang.org/std/fs/fn.copy.html), and
  [rename](https://doc.rust-lang.org/std/fs/fn.rename.html), Rust 1.99.1.
- [Microsoft CopyFile security and attributes](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-copyfilew).
- [Playwright fixtures](https://playwright.dev/docs/test-fixtures) and
  [retrying DOM assertions](https://playwright.dev/docs/test-assertions).

The `settings` command uses the same parser in bindings, aliases, widget clicks
and the CLI/IPC command flow. It produces a Settings control effect; the daemon
opens the companion window through its normal application-launch path. The
built-in process rule keeps `moon-settings` outside managed tiling.

## Live help and draft recovery

`moon cheatsheet` opens the Shortcuts page of the Settings companion. Its
**Available now** list comes from the running daemon's committed binding mode,
including its pause state. The same resolver supplies the keyboard hook, so
disabled bindings and empty aliases do not appear as available actions. Searching
matches keys and actions; the emergency recovery shortcut remains visible.
Help still opens when the saved configuration is invalid. A standalone `moon.exe`
without the companion retains its text help; a broken companion produces an
actionable error rather than silently switching applications.

The native launcher accepts only the named `general`, `overview` and `shortcuts` pages.
It passes a typed page value to the local interface without changing navigation
or origin rules. Live help and the General workspace overview refresh when their
page opens or Settings regains focus, with an explicit Refresh action as well.
Neither requires an idle polling loop. The overview reports current workspace
counts without transmitting window titles or claiming to reconstruct past
first-launch assignments.

The dedicated **Overview** page uses a separate committed desktop catalog with
window titles, application identities and planned workspace geometry. It searches
across workspaces and supports focus, moves, float/tile, layout Undo and rules
created from a selected window. Rule explanations describe matches against
current metadata, not the reason for a historical placement. Actions validate
the daemon generation, revision and window process birth before applying.
The compact General overview continues to transmit counts only.

Live thumbnails are DWM resources owned by the Settings window. Up to 32 fully
visible slots are registered on demand, released when leaving Overview or losing
foreground, and refreshed after geometry changes. Protected or unavailable
content retains its app label and geometry. App icons use bounded native icon
requests; the host releases its copied icons and bitmaps after conversion.
The daemon retains no thumbnail or WebView resources. Internal pointer dragging
works alongside the host's external-file drop restrictions; every move also has
a keyboard-accessible action.

Theme and profile persistence and native file sharing are described in
[the desktop library](desktop-library.md). Health controls on About expose pause,
reload and confirmed restoration, with unavailable actions disabled while the
Windows session is suspended. Preset application and rule creation remain draft
changes until saved; window actions act on the desktop immediately.

**Undo** restores one draft checkpoint, including invalid input, and never writes
the saved file. Native text-field undo remains available. Saving, discarding or
reloading clears the checkpoint. The Appearance preview represents the draft;
it does not apply unsaved colors to the desktop.

After saving, a persistent status separates storage from application. A status
check compares the expected source against the file, confirms the running
configuration path, and compares the active configuration against the saved
effective configuration. Equality confirms that Moon loaded the settings, not
that every native window accepted its new placement. A stopped daemon, different
configuration, rejected reload, changed file, paused or suspended session, and
reported native error remain explicit. An older check cannot replace the status
of a newer save. **Check desktop** retries without saving again.

The live help follows Microsoft's [keyboard shortcut discovery guidance](https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-accelerators),
checked on 15 November 2026. The existing Bun, TypeScript, Wry and Tao versions
remain pinned; this work introduces no new runtime dependency.

## Native window and material lifecycle

The Windows host retains the standard caption and snap behavior. Its WebView
is transparent over a full-client DWM main-window backdrop, with opaque CSS
fallbacks when the backdrop is unavailable or high contrast is enabled. The
host explicitly clears its background to zero-valued black: Microsoft's
extended-frame contract requires zero alpha under the frame. Leaving these
pixels uninitialized produced a visible rectangle at the initial window size
after a window manager enlarged the window; the explicit background corrected
the actual desktop capture at 1886 × 2048 pixels, with 125% monitor scaling.

A same-thread window subclass observes sent preference broadcasts as well as
resize, DPI, theme, system-color, and composition messages. It coalesces them
into the ordinary event loop before refreshing the material and fallback CSS.
It removes itself on destruction or owner drop. WebView2's existing parent
subclass continues to resize the browser controller; the material observer
does not replace its procedure or introduce an animation loop. IPC envelopes
are bounded to 4 MiB, allowing a 1 MiB source plus edits and JSON escaping;
the store independently enforces the final saved-file limit.

On 12 November 2026, `scripts/validate-settings-ui.ps1 -MoonBinary ...`
passed all eight native checks on the current SID-compatible candidate. These cover
an isolated daemon with no managed windows, loading the owned configuration,
saving while preserving comments, confirming both the UI's applied message and
the daemon's queried inner gap of 24, rejecting concurrent edits without losing
the draft, cancelling and confirming reload, opening the Appearance preview, and
saving a different configuration without changing the running daemon or revision.

The tested SHA-256 values were:

- Moon: `DBADD0C08FA4C78AA1EF567520B593B7D3F68297BD3724C641CEDB082ECBCCAE`.
- Moon Settings: `6A633A06B872765EA30AEDCBC9839F856FD3EA70BA5D855AC1BD85CCC8670E92`.

The report and Appearance screenshot are retained in
`target/sid-settings-ui/{report.json,appearance.png}`. The report records no cleanup
errors and no change to the user's configuration; its isolated daemon managed
zero windows. The ordinary live daemon resumed afterward. Earlier candidate
evidence remains in `target/polish-settings-ui` and
`target/polish-settings-ui-pre-static-ea798913`. These checks establish the current
Settings-to-normal-daemon behavior; launching Settings from the installed
UIAccess daemon and verifying its ordinary token still require the signed
installation. They do not establish a performance budget or future build's
behavior.

- [DWM frame pixels and alpha](https://learn.microsoft.com/en-us/windows/win32/dwm/customframe)
- [Frame extension and composition changes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmextendframeintoclientarea)
- [Same-thread subclass ownership](https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-setwindowsubclass)
- [Mica fallbacks and content layering](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)

Native theme-library checks can be run with `scripts/validate-settings-library.ps1`
using an explicit Settings binary, its SHA-256 and a fresh output directory.
The harness changes only its owned configuration/library fixture and export files.
See [desktop polish validation](desktop-polish-validation.md) for measured scope.
