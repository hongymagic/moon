# Moon desktop polish

Authorized 15 November 2026: implement all six additions proposed in the current
conversation, preserving the existing Windows desktop until a validated candidate
is ready. This extends the v1 plan; signed installation and elapsed-time acceptance
remain separate requirements in `moon-v1.md`.

## User experience

1. Overview: searchable windows across workspaces, app icons, live native preview
   slots, focus through the existing carousel, and equivalent pointer/keyboard
   window moves. A dedicated shortcut opens Overview without changing Alt+Space.
2. Rules from a window: choose workspace/floating/ignore, inspect matching windows
   and currently matching rules, then stage the change through Settings Save.
   Current rule matches are not presented as historical placement provenance.
3. Layout Undo: one bounded checkpoint for deliberate layout edits, independent
   of Settings draft Undo; stale windows, configuration or topology invalidate it.
4. Profiles: named, local desktop presets applied to the current draft. Existing
   monitor reconnection logic remains the runtime authority.
5. Themes: coordinated built-in and personal themes; bounded versioned sharing
   documents contain appearance settings only, never executable configuration.
6. Health and recovery: explain live state and expose guarded pause, reload,
   layout Undo and confirmed window restoration through existing daemon controls.

## Implementation boundaries

- Core/daemon owns the committed desktop catalog, native window identity checks,
  layout Undo and typed actions. IDs and birth/revision tokens cross JavaScript
  boundaries as decimal strings. Configuration changes use the existing store.
- The ordinary Settings host owns optional DWM previews and app icons. It holds
  at most 32 visible thumbnails, releases them on navigation/foreground loss, and
  verifies source identity. No screen capture files or polling timer are needed.
- Profiles/themes use a versioned local sidecar with an optimistic source token,
  bounded data and atomic writes. Applying a library entry produces draft edits;
  it never silently overwrites the user's configuration.
- The frontend remains Bun/TypeScript with existing components, keyboard focus,
  high contrast and reduced-motion behavior. Drag actions have button equivalents.
- The visual language follows Windows 11: Segoe UI Variable, sentence-case page
  titles, restrained Mica beneath readable content surfaces, Moon's neutral/copper
  palette, clear focus indicators and quiet transitions. Settings follows Windows
  appearance; desktop theme presets coordinate the bar and window borders.
- Preset files use the native Windows Open/Save dialogs. Import previews precede
  library changes, cancellation is harmless, and export confirms replacement.
- Workspace dragging uses pointer capture because Wry's Windows native drop
  handler disables browser HTML drag-and-drop. Keyboard Move remains equivalent.

## Validation

Exercise actual behavior and failure boundaries: stale target identities,
workspace focus, undo restoration/invalidation, safe library imports, conflicting
writes, unrelated configuration preservation, keyboard/search/drag alternatives,
draft persistence and native resource cleanup. Build ordinary and UIAccess images
from immutable inputs; preserve the previous package. Native promotion must retain
the user's window identities, workspace geometry, saved configuration and focus.
Never infer signed installation, physical hardware tests or a seven-day soak from
portable tests or a successful cross-build.

## Official references checked

- [DWM thumbnails](https://learn.microsoft.com/en-us/windows/win32/dwm/thumbnail-ovw)
- [Thumbnail ownership](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmregisterthumbnail)
- [Keyboard interaction](https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-interactions)
- [Windows motion](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/motion)
- [Windows typography](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- [Mica layering and fallback](https://learn.microsoft.com/en-us/windows/apps/design/style/mica)
- [Windows geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry)
- [Common file dialogs](https://learn.microsoft.com/en-us/windows/win32/shell/common-file-dialog)
- [Window icons](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-geticon)
- [DIB synchronization](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-createdibsection)
- [Wry 0.57.2](https://docs.rs/wry/0.57.2/wry/)
- [Base64 0.23.1](https://docs.rs/base64/0.23.1/base64/), already in the lockfile,
  is now an explicit Windows Settings dependency for native icon transport.
