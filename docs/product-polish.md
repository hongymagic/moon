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
and rounded corners when available. The native host reads Windows' Transparency
effects preference through `UISettings.AdvancedEffectsEnabled` and selects an
opaque surface when it is disabled. Contrast mode uses system colors and opaque
surfaces; reduced motion disables decorative transitions. The seven pages have
keyboard-visible focus, labeled controls, search, draft indicators, explicit save,
discard and reload, and a close confirmation for unsaved changes. **Desktop theme**
controls edit the same live configuration as the CLI, including light/dark/system
themes, border colors and gradients, bar materials and font size. The Settings
interface itself follows Windows' appearance. Its desktop preview displays both
gradient colors and updates when the system color scheme changes. Advanced
structured editors expose the remaining configuration without a second format.

Persistence preserves comments and personal overrides, validates complete batches,
checks concurrent edits, and replaces the file atomically. The precise limits are
in [settings-editing.md](settings-editing.md). Failed saves retain the draft and
focus the error after editing is enabled again; the error remains available when
changing pages. **Review errors** returns to an invalid field, opens its editor
when needed, and preserves its explanation. Resetting a section clears invalid
raw entries in that section. Keyboard saves restore focus to the edited field,
or to the current page heading if the user navigated while saving. A confirmation
dialog owns keyboard shortcuts and repeated close requests until it is dismissed.
A successful save reloads only a daemon using the same configuration path.

At narrow widths, navigation becomes a compact grid and the status sits above one
aligned row of Reload, Discard and Save actions. Short viewports use normal
document scrolling so navigation, editing and saving remain reachable. High
contrast uses the user's system Highlight/HighlightText colors for the primary
action, preventing an unreadable forced-color text backplate.

The frontend now sends a one-time local readiness notification after rendering
the initial loaded interface or its recoverable startup-error page. It sends
nothing while configuration loading is pending, and retries do not repeat the
notification. This gives the native host a meaningful initial-DOM signal for
revealing its hidden window; Wry's page-load completion event alone was too early.
The host bounds startup to 30 seconds so a failed script cannot leave an
invisible process indefinitely. Native opening checks exercise both the normal
and error pages. This handshake does not promise a compositor frame with no
intermediate blank pixels; immediate captures can still precede drawing.
Physical Windows transparency-preference transitions remain unmeasured.

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

## Current polish verification

The following checks cover the current source and frontend bundle. They do not
replace signed-install, native interaction, performance or soak acceptance.

| Check | Result and scope | Evidence |
| --- | --- | --- |
| Frontend build, formatting and TypeScript | Passed | `target/settings-polish/report.json` |
| Browser interaction suite | 31/31 passed; startup readiness, dialog ownership, save/conflict handling, invalid-field recovery, focus, themes, reflow and accessibility media preferences | `target/settings-polish/browser-results.json` |
| Portable Rust tests | 89/89 passed: 10 daemon, 73 core and 6 Settings store tests; Windows-only runtime paths are not executed | `target/polish-portable-tests.log` |
| Windows-target Clippy | Passed; compile/static-analysis check, not native execution | `target/polish-windows-clippy.log` |
| Windows native units | 22 passed; four separately interactive hook tests excluded | `target/polish-native-unit-results.txt` |
| Native desktop | 14 integration, four initial-assignment and ten carousel cases passed | `target/polish-final-{integration,startup,carousel}.json` |
| Native Settings | Eight passed with the current daemon, including save/apply, conflicts and different-file isolation | `target/polish-final-settings/report.json` |
| Setup | 32 isolated state/handler checks and 14 recovery checks passed; both WPF modes compiled; owned visual preview does not install or remove | `target/setup-polish-validation.json`, `target/setup-polish-recovery-validation.json`, `target/setup-polish-ui/report.json` |

Four focused frontend regressions were reproduced before the fixes; their failed
baseline is retained in `target/settings-polish-before/regressions.json`.
Screenshots in `target/settings-polish/` cover light, dark and emulated high
contrast, plus 320, 370 and 740 CSS-pixel widths. At 740×540, the footer actions
are checked for a common vertical position as well as viewport containment.
The 370×270 check exercises a reflow area equivalent to halving that content
viewport: it edits a field, saves by keyboard and navigates all seven pages.
It is not a physical Windows 200% DPI test. Browser media emulation likewise
does not establish native high-contrast or reduced-motion behavior.

The report records the exact source and generated-asset hashes. The current
native binary identities and results are recorded in
[acceptance evidence](acceptance-status.md#current-polish-candidate-13-november-2026).
Normal and malformed-config opening checks both reveal the expected UI and close
cleanly. Immediate native captures may precede compositor drawing; these results
do not establish a zero-blank-frame presentation, physical accessibility-setting
transitions, full accessibility conformance or release acceptance.

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
- [Windows effects preferences](https://learn.microsoft.com/en-us/windows/apps/develop/composition/composition-tailoring): `UISettings.AdvancedEffectsEnabled` reports the Transparency effects preference; custom effects need an opaque alternative when disabled.
- [WebView2 security](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/security): ordinary privileges, origin checks and narrow host operations.
- [WebView2 distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution): Evergreen sharing, runtime detection and missing-runtime handling.
- [Wry 0.57.2 API](https://docs.rs/wry/0.57.2/wry/struct.WebViewBuilder.html) and [Tao 0.37.2](https://docs.rs/tao/0.37.2/tao/): host, navigation, IPC and native window integration.
- [Wry page-load handler](https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html#method.with_on_page_load_handler): page-loading events reviewed against the pinned 0.57.2 API; the application readiness handshake additionally waits for the initial Settings DOM.
- [WAI modal dialog pattern](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/): modal focus and keyboard behavior.
- [WAI reflow guidance](https://www.w3.org/WAI/WCAG22/Understanding/reflow.html): preserve information and functionality as the content viewport narrows.
- [Forced colors](https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/%40media/forced-colors): system colors and text backplates in browser contrast modes.
- [Bun installation and upgrades](https://bun.sh/docs/installation): current stable runtime.

Validation is recorded separately from implementation: browser interaction tests,
portable document/store tests, native Settings save/conflict checks, setup UI and
isolated shell-registration tests, and desktop animation probes each establish
different properties. An unexecuted signed installation is not a passed release.
