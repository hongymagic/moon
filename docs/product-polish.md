# Product interface and Windows integration

The 2026-11-12 product request extends the original v1 scope to include settings,
guided setup, product identity, theme editing, Windows 11 materials, and carousel
navigation. The original signed-install, resilience, performance and seven-day
acceptance conditions remain tracked in [acceptance-status.md](acceptance-status.md).

## Development refinement, 15 November 2026

The isolated `codex/polish-20261115` branch adds searchable live shortcut help,
a current workspace overview, one-step draft Undo and persistent save/application
feedback. Existing automatic first-launch adoption remains the source of workspace
assignments; the overview explains those assignments rather than introducing a
second placement mechanism. [Settings editing](settings-editing.md#live-help-and-draft-recovery)
documents the behavior and recovery limits.

Rapid workspace switches now retarget the visible carousel from its current
positions, including reversals, while retaining at most three pages per monitor.
The native final layout remains independent of presentation. This refinement has
portable continuity/bounds tests, a Windows cross-build and the live workspace
roundtrip recorded below. The roundtrip observes animation activity and final
state; rapid-retargeting smoothness has not been visually validated. Previous
carousel results below cover the earlier implementation.

Installer changes preserve both an explicitly selected configuration and implicit
defaults through startup, recovery and Settings entry points. Portable transfer
authenticates the exact running image, retains a durable workspace checkpoint
before exit, and restores that same Moon build after a failed installation. It
refuses a paused, suspended or diagnostic session and checks that saved settings
match the running configuration and can be parsed by the signed replacement.
It never restores another window manager during portable recovery.
The isolated PowerShell 5.1 suites pass 20 checkpoint/configuration checks,
33 setup-state checks and 22 recovery checks without installing, requesting
elevation or changing the live daemon. Reports are under
`target/polish-20261115/` with `-final.json` names. Actual signed transfer remains
a separate Windows acceptance check. The additional 34 checks in
`portable-install.json` cover exact peer identity using private connected pipes,
settings equivalence and the actual capture/stop/recovery helpers with desktop
actions substituted. They pass after correcting JSON value comparison on
PowerShell 5.1; all 41 scripts also parse successfully.

After merging the repository review, all 123 portable Rust tests pass (96 core,
14 daemon, 13 Settings), including standalone-help routing and restart preflight. Settings passes 44
browser checks, and Windows-target Clippy passes for all affected crates. The
frontend receipt is `target/polish-20261115/settings-merge-validation.json`; screenshots
in `target/settings-polish/` include the live overview and shortcut list, dark and
high-contrast appearances, and narrow previews with no caption overlap.
Ordinary and UIAccess release images are built with 116 unchanged source inputs;
their exact identities and manifests are in
`target/polish-20261115-final/release-build.json`. One new native cleanup regression
passes using hidden owned popups, without input hooks or foreign windows.
The ordinary image has since been promoted and exercised as recorded below.
The UIAccess image has not completed installation; these build checks do not
establish signed-installation, performance or soak acceptance.

## Live ordinary checks, 15 November 2026

The ordinary daemon now runs as PID 48436 with SHA256
`512515BF2BB41FFC95776ECCEF26064076B865AE446CCE6DF9136DDC09883B8E`.
Its token is medium integrity, not elevated and without UIAccess. The promotion
preserved the four original application identities, configuration and workspace
layout. The subsequent workspace 2 → 10 roundtrip restored the original full
layout fingerprint, remembered focus and workspace recency; visible native frames
and inactive-window cloaking were verified. Four of nine query samples reported
`carousel_active=true`, and both transitions settled. This establishes activity
and final-state behavior, not visual motion quality or rapid-switch continuity.

Settings build
`53F48DA88434577EA18C99C0D81A949471E1D67A4EE9E168F79768D3EF7F2709`
passes six checks in its real Windows host using a separate owned configuration:
loading the selected file, one-step Undo without a write, validated save with
comment preservation, conflict rejection with the draft retained, reload
confirmation, and Appearance controls with their preview. Cleanup reports no
errors. The outer check confirms that the personal configuration and live desktop
layout remained unchanged, and the coordinator visually reviewed `appearance.png`.
This run did not start a second daemon or exercise live shortcut rendering or
same-file application in the native Settings window; those report fields are null.

A separate authenticated read-only query against PID 48436 returned 52 effective
bindings in default mode with hooks enabled and Moon unpaused. The overview
reported four managed windows: one each on workspaces 1 and 2, and two on active
workspace 10. The reported configuration matched the saved file. These are live
backend checks; the 44 browser checks separately cover the help interface.

A later native check also exercised the real `cheatsheet` launch into Shortcuts,
verified all 52 active bindings, mode and emergency recovery, and tested filtering,
no matches and clearing the search. General matched the live workspace counts.
The exact ordinary Settings companion closed gracefully without edits, original
foreground returned, and the same daemon, personal configuration and full layout
were preserved. All five checks passed with no cleanup errors. This separate run
is recorded in `polish-live-help-ui-20261115/result.json` and its retained copy
`target/polish-20261115-final/live-help-ui-result.json`; it does not establish live
application of changed personal settings.

The installation attempt for operation `6c9f37baec5f4b7092208e02a6b505c1`
returned Windows cancellation code 1223. Its recorded state is
`rolled_back_portable`, installation metadata is absent, and the same ordinary
PID 48436 remained healthy with four managed windows and no daemon error. No
signed UIAccess transfer or elevated-application acceptance is claimed. Physical
DPI/accessibility/resilience checks, current-candidate performance acceptance and
the installed seven-day soak remain open.

Evidence paths below are relative to
`C:\Users\dhong\Downloads\Moon-validation\`:

| Scope | Saved evidence |
| --- | --- |
| Ordinary promotion | `polish-promote-20261115-v3/result.json` |
| Native Settings, six checks | `polish-native-settings-20261115/settings-fixture/report.json` |
| Personal configuration and desktop preservation | `polish-native-settings-20261115/result.json` |
| Reviewed Appearance capture | `polish-native-settings-20261115/settings-fixture/appearance.png` |
| Live shortcuts and overview | `polish-current-readonly-20261115/result.json` |
| Workspace roundtrip and activity samples | `polish-focus-restore-20261115-v2/result.json` |
| Cancellation and remaining ordinary daemon | `polish-install-20261115/{handover-result,failure}.json` |

## Recorded ordinary candidate, 13 November 2026

The earlier session-recovery update was staged as ordinary build `47521240…` and
UIAccess build `AFBA0420…`. It guards cursor movement while suspended and resets
carousel input after a failed session recheck. Portable checks and the focused
Windows registration test passed; native desktop validation and promotion were
pending at that stage while Windows was locked. See
[candidate status](acceptance-status.md#staged-session-recovery-update-13-november-2026).

The earlier broad native-validation run used ordinary daemon
`43FD913681AD449967DEEBD2DD6BA38DAD59E37D384FD1142A30E9FC9F17E314`.
Its matching UIAccess image was
`748B8829283BA55E7367EEE2F0820D3C3205914BF584B93668A44D9215D1F496`,
with Settings build
`EC22F24B07DFAD2ABE6198B519016E26E2C119843351FE81AEAC13EE9799727C`.
The installation image has not yet been signed or installed. Build records are
`target/provider-demand-build.json` and `target/provider-install-build.json`.

The recorded 43FD promotion ran through Explorer as PID 32520 from
`Downloads\Moon-validation\moon-provider-demand-20261113`, with EC22 Settings
adjacent. All four original window/process/birth identities and workspace
assignments survived, visible frames and prior focus were verified, and the
personal configuration was unchanged. It ran at medium integrity without
UIAccess, with no previous manager running. `target/provider-demand-live.json`
records this promotion separately from pending signed-installation acceptance.

This ordinary candidate passes 77 core tests, 36 native unit tests, 14 lifecycle
cases, ten workspace-carousel cases and all 128 exact placements in the default
performance workload. Ready time is 118.223 ms, external placement median/p99
11.944/29.496 ms and own/whole-hook p99 101/886 µs.
Working-set samples are 45,543,424 / 48,816,128 / 46,981,120 bytes before input,
after input and after idle; default idle CPU is 0.156248% of one core. Both opt-in
profiling modes are disabled. These memory samples are below 50,000,000 bytes,
but earlier E9 image-residency variation prevents a universal memory-pass claim.
Placement median, hook latency and literal-zero idle CPU still miss the budgets;
placement p99 passes in this run only. Reports:
`target/provider-demand-{integration,carousel,performance}.json`.

The scoped visual review passes for all 13 images, byte-identical to the accepted
960 comparison captures: eight complete rings, five bars, and no residual pixels
in the removed-split region. This is explicit
per-target inspection ordering at 4K/125%; normal product Z-order and other DPI
settings are not covered. Reports are in
`target/provider-demand-visual/{report,visual-review,pixel-review}.json`.
The earlier E9 and 960 lowest-helper captures remain partial evidence; an
intervening window is a plausible explanation, not an identified cause.

Retained visible text layouts and provider selection avoid unused work. A matched
60-second Static pair records 0.234353% to 0.078116% one-core CPU and 429,596,270
to 99,043,820 cycles, with unchanged scene, draw, layout, commit and revision
counters. This is one controlled pair, not a universal or default-bar CPU
improvement; the default bar still requests all displayed provider families.
See `target/provider-idle-{1-baseline-static,2-candidate-static}/report.json`.

Six harmless tracked-launcher parser/behavior checks and the cleanup check pass
in `target/explorer-launch-tracked-validation.json`; they perform no installation
or UAC request. Signed privilege checks, physical resilience and seven-day
sole-manager acceptance remain open.

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
A successful save is picked up by a daemon watching the same configuration path.
Settings reports when Moon has loaded that configuration; it does not equate a
matching configuration with completed desktop effects.

At narrow widths, navigation becomes a compact grid and the status sits above one
aligned row of Undo, Reload, Discard and Save actions. Short viewports use normal
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

Workspace buttons adapt to their displayed name or icon and the configured font
size, while retaining a minimum hit area and the configured maximum width. The
larger-font Windows review now shows workspace 10 in full. Border brushes and
bar text formats are reused within bounded per-canvas storage; changing their
style replaces the relevant resource, and closing windows releases retired
border slots.

Native hover/press feedback and entrance effects use bounded DirectComposition
animations. They introduce no animation polling while idle. Contrast and reduced
motion apply to these surfaces as well as Settings. Window-manager performance
and settings-window performance are separate measurements, not an aggregated
claim that a WebView fits the daemon's memory budget.

## Earlier polish verification

The following checks cover the E050 polish baseline and unchanged frontend
bundle. Renderer and workspace-label follow-up results are listed below. These
checks do not replace signed-install, performance or soak acceptance.

| Check | Result and scope | Evidence |
| --- | --- | --- |
| Frontend build, formatting and TypeScript | Passed | `target/settings-polish/report.json` |
| Browser interaction suite | 31/31 passed; startup readiness, dialog ownership, save/conflict handling, invalid-field recovery, focus, themes, reflow and accessibility media preferences | `target/settings-polish/browser-results.json` |
| Portable Rust tests | 89/89 passed: 10 daemon, 73 core and 6 Settings store tests; Windows-only runtime paths are not executed | `target/polish-portable-tests.log` |
| Windows-target Clippy | Passed; compile/static-analysis check, not native execution | `target/polish-windows-clippy.log` |
| Windows native units | 22 passed; four separately interactive hook tests excluded | `target/polish-native-unit-results.txt` |
| Native desktop | 14 integration, four initial-assignment and ten carousel cases passed | `target/polish-final-{integration,startup,carousel}.json` |
| Native Settings | Eight passed with the E050 baseline daemon, including save/apply, conflicts and different-file isolation | `target/polish-final-settings/report.json` |
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
[acceptance evidence](acceptance-status.md#current-text-layout-and-provider-refinement-13-november-2026).
Normal and malformed-config opening checks both reveal the expected UI and close
cleanly. Immediate native captures may precede compositor drawing; these results
do not establish a zero-blank-frame presentation, physical accessibility-setting
transitions, full accessibility conformance or release acceptance.

The later D583 renderer/workspace-label candidate passes 75 core tests, 27 native
unit tests, 14 lifecycle cases and ten carousel cases. All 13 images from five
native styling stages passed review at 4K/125%, with the documented inspection
override exposing all border edges. The larger Arial font displays workspace 10
without truncation; complete gradients, rounded corners, focus colors and removal
of old borders were checked. Reports are under `target/workspace-labels-visual/`.
Separately, Paint's genuine WinUI3 colour dialog passed six ordinary-desktop
checks on E050, including floating, workspace hide/show, cancellation and retiling.
Its original desktop and document bytes were preserved. The remaining numerical
performance limits are reported explicitly in the acceptance ledger.

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
