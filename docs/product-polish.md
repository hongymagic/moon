# Product interface and Windows integration

The 2026-11-12 product request extends the original v1 scope to include settings,
guided setup, product identity, theme editing, Windows 11 materials, and carousel
navigation. The original signed-install, resilience, performance and seven-day
acceptance conditions remain tracked in [acceptance-status.md](acceptance-status.md).

## Settings

`moon-settings.exe` is a separate ordinary-permission process. It refuses an
elevated or UIAccess token. The daemon launches it through Explorer's ordinary
shell, and the Start shortcut targets it directly. Closing Settings releases its
WebView process; the daemon never hosts a browser. The frontend is bundled,
served through an in-process resource protocol, and cannot navigate to remote
pages, create popups, download files, or make network requests. Host requests
have a checked origin, a bounded message size, and a fixed operation list.

The interface uses native window chrome for resizing, snapping, system menus,
and caption buttons. Windows 11 system backdrop and corner preferences add Mica
and rounded corners when available. Contrast mode uses system colors and opaque
surfaces; reduced motion disables decorative transitions. The seven pages have
keyboard-visible focus, labeled controls, search, draft indicators, explicit save,
discard and reload, and a close confirmation for unsaved changes. Main theme
controls edit the same live configuration as the CLI, including light/dark/system
themes, border colors and gradients, bar materials and font size. Advanced
structured editors expose the remaining configuration without a second format.

Persistence preserves comments and personal overrides, validates complete batches,
checks concurrent edits, and replaces the file atomically. The precise limits are
in [settings-editing.md](settings-editing.md). Failed saves retain the draft.
A successful save reloads only a daemon using the same configuration path.

## Workspace motion

Workspaces form the carousel. Focusing a window in another workspace activates
that workspace and slides its whole canvas beneath the fixed monitor viewport.
Windows keep their tiled or floating geometry. Changing focus inside the same
workspace does not slide the canvas. `workspace-animation = false`, Windows'
reduced-motion setting, or high contrast selects immediate transitions.

The native presentation uses short-lived, clipped DWM thumbnails of the existing
windows. It does not reparent application windows, change their regions, forward
clicks, or move them off the monitor. The real layout commits independently of
the effect; rapid changes and interaction settle on that layout. The complete
contract and measured evidence are in [workspace motion](carousel-layout.md) and
[native presentation validation](validation-carousel.md).

## Brand and interaction

The crescent mark has vector source and deterministic icon resources at fifteen
sizes from 16 to 256 pixels. Product and version resources are embedded in both
executables. The tray uses light/dark variants; it reflects Pause/Resume and opens
Settings. Bar actions have labels, tooltips, pointer feedback, and release-inside
activation. A drag outside an action does not trigger it.

Native hover/press feedback and entrance effects use bounded DirectComposition
animations. They introduce no animation polling while idle. Contrast and reduced
motion apply to these surfaces as well as Settings. Window-manager performance
and settings-window performance are separate measurements, not an aggregated
claim that a WebView fits the daemon's memory budget.

## Dependencies and current references

Verified on 2026-11-12: Bun 1.4.4, TypeScript 7.0.2, Playwright 1.65.0, Wry 0.57.2,
Tao 0.37.2, windows-rs 0.62.2, and toml_edit 0.25.19. Exact versions and lockfiles
are committed. Wry's Windows backend uses webview2-com 0.39.1 and the shared
Evergreen Runtime. Windows 11 normally includes the runtime; setup checks for it
and provides Microsoft's download link when absent.

Windows builds statically link the C runtime using Rust's documented
`crt-static` target feature. This removes a separate Visual C++ redistributable
from first-launch prerequisites. Package validation checks the actual executable
imports; the WebView2 Evergreen Runtime remains a checked Settings prerequisite.
The implementation follows [Rust's runtime-linkage contract](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes).

The shipping dependency audit targets Windows x64. Cargo's cross-platform lockfile
also records Wry/Tao's unused Linux GTK dependencies; a lockfile-wide audit reports
RUSTSEC-2024-0370 (proc-macro-error) and RUSTSEC-2024-0429 (glib), neither present in
the Windows dependency graph. No advisory is suppressed. The portable Linux tests
do not compile the Windows-only Settings host or its GTK dependencies.

Primary references:

- [Windows system backdrop types](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type): Mica MAINWINDOW and minimum build 22621.
- [DWM window attributes](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute): native corners, theme and backdrop attributes.
- [Windows motion](https://learn.microsoft.com/en-us/windows/apps/design/motion/): bounded, purposeful feedback and system preferences.
- [WebView2 security](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security): ordinary privileges, origin checks and narrow host operations.
- [WebView2 distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution): Evergreen sharing, runtime detection and missing-runtime handling.
- [Wry 0.57.2 API](https://docs.rs/wry/0.57.2/wry/struct.WebViewBuilder.html) and [Tao 0.37.2](https://docs.rs/tao/0.37.2/tao/): host, navigation, IPC and native window integration.
- [Bun installation and upgrades](https://bun.sh/docs/installation): current stable runtime.

Validation is recorded separately from implementation: browser interaction tests,
portable document/store tests, native Settings save/conflict checks, setup UI and
isolated shell-registration tests, and desktop animation probes each establish
different properties. An unexecuted signed installation is not a passed release.
