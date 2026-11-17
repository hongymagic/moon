# Desktop profiles and themes

Open **Appearance → Theme gallery** to try **Linen**, **Slate** or **Forest**, or
save the appearance in your current draft with **Save your theme**. Each built-in
theme includes light and dark palettes and follows the Windows colour setting.
Themes change colours, font, bar style and border style. They preserve whether
your bar and borders are shown, your widgets and their actions, workspace
assignments, and custom palette entries not supplied by the theme.

Open **Workspaces → Saved setups** and choose **Save this setup** to keep a named
profile such as Desk, Laptop or Presentation. A profile contains workspace names
and monitor assignments, monitor overrides, spacing, appearance, bar and border
visibility, workspace motion, cursor jumping and focus-following preferences.
It does not capture open apps, window titles, window positions, commands, hooks,
rules, shortcuts, aliases, weather location or configuration file paths.

**Apply** adds the selected item's changes to your draft. Review the result or
use draft **Undo**, then choose **Save changes** to update the desktop. Saving or
renaming a library item alone does not change the configuration. Profiles are
manual presets; they do not automatically switch when a monitor is connected.
Applying checks the merged draft: for example, a profile cannot remove a
workspace still named by one of your personal rules, and a theme cannot require
borders wider than your current gaps.

Names are unique within each kind, ignoring case. Replacing a saved item requires
the explicit replacement checkbox. Built-in themes are read-only; save a copy
under a different name to customise one. Removing a library item leaves the
current draft and saved desktop settings intact.

## Import and export

**Import theme** or **Import profile** opens the Windows file picker. Choose a
UTF-8 JSON file, review its name and type, and confirm **Import to library**.
Importing does not apply the item to the desktop. **Paste theme data** and
**Paste profile data** provide the same validation for copied JSON.

**Export** opens the Windows Save dialog with a suggested `.moon-theme.json` or
`.moon-profile.json` name. Windows asks before replacing an existing file.
**Copy data** is available for sharing without a file. Canceling either file
dialog leaves the library, draft and configuration unchanged.

The file dialogs accept JSON paths on a local drive, not URLs, device paths or
UNC paths. Export resolves an existing file alias before replacing its target,
and verifies that the resolved destination is also a local JSON path. Reads are
bounded to 64 KiB and must be UTF-8; an optional UTF-8 byte-order mark is accepted
when importing a file. Export writes and syncs a temporary file alongside the
selected destination and checks its previous content before atomic replacement.
An observed concurrent edit stops the save. As with ordinary Settings saves,
this is not an operating-system compare-and-swap against noncooperating editors.

## Exchange format, version 1

Only these top-level fields are accepted:

```json
{
  "version": 1,
  "kind": "theme",
  "name": "Blue hour",
  "values": {
    "theme": "system",
    "palette": {"base": "#161b22", "text": "#e6edf3", "accent": "#89b4fa"},
    "light-palette": {"base": "#f4f7fa", "text": "#203346", "accent": "#386889"},
    "bar": {
      "height": 40,
      "font": "Segoe UI",
      "font-size": 14.0,
      "background": "base",
      "foreground": "text",
      "opacity": 1.0,
      "backdrop": "none"
    },
    "borders": {
      "width": 4,
      "radius": 8,
      "focused": "accent",
      "focused-end": "accent",
      "unfocused": "#64748b"
    }
  }
}
```

`kind` is `theme` or `profile`. Theme values have exactly the fields shown above;
`theme` is `system`, `light` or `dark`. Style values use Moon's normal configuration
validation. Palette maps merge by key when applied. `bar.widgets`, enabled
switches and every command-bearing field are excluded from a theme.

For a profile, `values` has these required fields:

| Field | Value |
| --- | --- |
| `appearance` | The complete theme values object above |
| `bar-enabled`, `borders-enabled` | Booleans |
| `gaps` | `{ "inner": number, "outer": number }` |
| `workspaces` | Workspace configuration rows: name and optional icon, colour, monitor and gaps |
| `monitors` | Monitor configuration rows: name and optional gaps, `bar-enabled` and `bar-height` |
| `workspace-animation`, `cursor-jump`, `focus-follows-cursor` | Booleans |

The exact workspace colour key is `color`, as in the configuration. Unknown
fields at every typed level and unsupported versions are rejected. Imports are
data only: they do not run commands or open applications. Names contain 1–64
Unicode characters, without surrounding whitespace, controls, `/`, `\` or `:`.
Each palette has at most 128 names; colour keys use up to 64 ASCII letters,
digits, hyphens or underscores. Font values are family names, not file paths.
Each exchange document is limited to 64 KiB, including its final exported bytes.

## Storage and validation

Each selected configuration has its own adjacent `.moon-library.json` sidecar;
for example, `config.toml.moon-library.json`. Existing configuration aliases
resolve to the same sidecar. Reading an absent library does not create files.
There are at most 64 user items and the whole sidecar is bounded to 1 MiB.

Mutations use the last exact optional sidecar source as their concurrency token,
the same bounded OS-lock and synced atomic-save helpers as Settings, and an
additional content check before replacement. A stale source, corrupt library,
duplicate name or invalid import is refused without overwriting saved items.
Applying produces ordinary Settings edits, preserving unrelated TOML comments
and values through the existing [editing contract](settings-editing.md).

The portable Settings suite passed 29 checks, including 12 library tests covering
unsafe and oversized imports, name and source conflicts, concurrent writers,
aliases, visibility, recipient compatibility and preservation of unrelated
comments, commands and widgets. Evidence:
`target/polish-20261115-enhancements/settings-library-tests.log`. This is backend
validation; it does not claim that the native file dialogs have been exercised
on the Windows desktop.

Implementation references: Rust's current [file locking and synchronization
API](https://doc.rust-lang.org/std/fs/struct.File.html),
[rename semantics](https://doc.rust-lang.org/std/fs/fn.rename.html), Serde's
[strict field handling](https://serde.rs/container-attrs.html), and Microsoft's
[Open](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ifileopendialog),
[Save](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ifilesavedialog)
and [dialog options](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/ne-shobjidl_core-_fileopendialogoptions)
documentation. The implementation reuses the repository's pinned Rust and
Windows SDK bindings and adds no dependency.
