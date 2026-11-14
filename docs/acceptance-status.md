# Acceptance evidence

This ledger maps the checks in `.claude/plans/moon-v1.md` to observed evidence.
It was reviewed on 15 November 2026 (Sydney). Implemented event handling and passing
simulated tests do not establish that the corresponding physical Windows event
has been exercised.

## Staged combined review and polish, 15 November 2026

Commit `b2c9fc9` combines the completed repository review (`75e5468`, an exact
45-file snapshot of the review working tree) with the polish implementation.
The original review files were independently hash-checked and remain unchanged
in their checkout. This candidate adds current shortcut help and workspace counts,
draft Undo, truthful saved/loaded feedback, continuous carousel retargeting, and
the reviewed IPC, reload, configuration, provider and workspace fixes.

The final staged ordinary image, including the read-only restart preflight, is
`512515BF2BB41FFC95776ECCEF26064076B865AE446CCE6DF9136DDC09883B8E`;
the UIAccess image is
`2B0AA7420F2CF206376BBB01606B0C46726211E6D9561FA4AE12FE2A26FD2CFF`;
Settings is
`53F48DA88434577EA18C99C0D81A949471E1D67A4EE9E168F79768D3EF7F2709`.
All 116 build inputs remained unchanged during compilation. Manifest, icon and
static-runtime import checks are recorded in
`target/polish-20261115-final/release-build.json`; copied Windows images are verified
in `target/polish-20261115-final/windows-stage.json`. The earlier combined-build
artifacts and their receipts remain unchanged.

The combined source passes 123 portable Rust tests (96 core, 14 daemon, 13
Settings), 44 Settings browser checks, Windows-target Clippy, and formatting.
The merged installer foundation passes 20 snapshot/configuration checks, 33
setup-state checks and 22 recovery checks under PowerShell 5.1. These use private
fixtures, not a live installation. All 34 portable-transfer checks pass, including
actual private connected-pipe identity checks and mocked capture/stop/recovery
failure paths. The effective-config comparison preserves array order and scalar
values; a regression caught by the first run was corrected and the suite rerun.
All 41 PowerShell scripts parse under Windows PowerShell 5.1.
The native carousel cleanup regression also
passes on the exact new unit executable using hidden owned popups only; it
does not exercise visible animation, keyboard hooks or the user's windows.
Reports are under `target/polish-20261115/` with prefixes `merged-core-checks`,
`settings-merge-validation`, `combined-` and `native-hidden-popup-`.
Final installer reports are `upgrade-snapshot-final.json`,
`install-recovery-final.json`, `setup-state-final.json` and `portable-install.json`.
The package regression suite also passes on the final executable inputs; it
reads executable resources without launching them, checks archive integrity and
failure cleanup, and confirms the original images are unchanged. Its report is
`target/polish-20261115-final/package-validation.json`.

The candidate is built and copied to Windows, not promoted, signed or installed.
At the 19:09 UTC readiness check, Windows was still locked. The old 43FD daemon
has not been stopped during this work. Its shared-pipe implementation cannot
authenticate a live upgrade with the new client; migration must first end that
specific old instance using the documented compatibility path. The new portable
handoff does not relax the single-instance IPC requirement.

Visible carousel and Settings validation, signed upgrade/rollback, physical
resilience, performance budgets and the seven-day installed soak remain separate
acceptance conditions. Earlier results below do not establish them for this build.

## Staged session-recovery update, 13 November 2026

The cursor and carousel session-recovery fix is committed in `70c7d98`. The
ordinary candidate is
`475212403C1DBE8AF5BBD8F8ABA78AC8543523481F4D53AFE37746B3071D2067`;
the UIAccess installation candidate is
`AFBA0420B38E0EA05084138C7F0E48D5E2E77ACBE2B34BF1100FD43287866C81`.
Both are staged separately from the running 43FD build. The 93 portable tests,
both workspace Clippy checks and the focused Windows registration regression
pass. The focused test makes no native calls; it does not replace physical
lock/unlock or the remaining native acceptance checks.

The actual session remains locked, with the four original application identities,
workspace assignments and configuration preserved. Neither new candidate has
been promoted, signed or installed. Build and test evidence is in
`target/session-lock-{build,stage,native-unit}.json` and
`target/session-install-build.json`; [platform evidence](validation-platform.md)
describes the recovery changes. The measurements below still refer to the last
fully exercised ordinary candidate, 43FD.

## Current text-layout and provider refinement, 13 November 2026

The latest validated ordinary daemon is
`43FD913681AD449967DEEBD2DD6BA38DAD59E37D384FD1142A30E9FC9F17E314`,
with Settings
`EC22F24B07DFAD2ABE6198B519016E26E2C119843351FE81AEAC13EE9799727C`.
Its recorded Explorer promotion ran as the sole manager, PID 32520, from
`Downloads\Moon-validation\moon-provider-demand-20261113\moon.exe`.
All four original application identities and workspace assignments are preserved;
visible native frames match their placements, hidden windows remain hidden,
prior focus returned and the personal configuration is unchanged. The token is
medium integrity without UIAccess. Evidence: `target/provider-demand-live.json`.

The corresponding UIAccess installation image is
`748B8829283BA55E7367EEE2F0820D3C3205914BF584B93668A44D9215D1F496`.
Its embedded manifest is `asInvoker`, `uiAccess=true`; Settings remains
`uiAccess=false`. All four staged installation inputs are hash-verified in
`target/provider-install-build.json`. This image is built and staged, not yet
signed or installed.

Bar canvases now retain only their current visible text layouts. Provider
sampling follows the configured templates and direct custom-command IDs,
preserving existing native-value fallback and override behavior. Unused media
requests are canceled and released. The default bar still requires all of its
displayed native provider families. These changes retain the existing one-second
maintenance cadence.

This exact candidate passes 77 core tests, 36 native Windows unit tests, the
local-provider/command probe, 14 lifecycle cases and ten workspace-carousel
cases. Four interactive hook tests are explicitly ignored in the native unit
run; previous separate hook evidence is unchanged. All 128 placement trials
complete exactly without a daemon error. All 13 reviewed images are byte-identical
to the accepted 960 captures. The visual result covers explicit per-target
inspection ordering at 4K/125%, not other physical DPI settings or normal Z-order.
Evidence: `target/provider-demand-build.json`, `target/provider-demand-native-units.log`,
`target/provider-demand-providers.json`, `target/provider-demand-{integration,carousel,performance}.json`
and `target/provider-demand-visual/{report,visual-review,pixel-review}.json`.

A matched 60-second static-screen pair records CPU use falling from 0.234353% to
0.078116% of one core and cycles from 429,596,270 to 99,043,820. Both intervals
have zero revisions, scene changes, draws, layout creations and commits, with
clean fixture teardown and restored personal windows. This single pair establishes
an improvement in that controlled run, not literal-zero CPU or a universal
performance guarantee. Reports: `target/provider-idle-{1-baseline-static,2-candidate-static}/report.json`.

The default workload records external placement median/p99 of 11.944/29.496 ms:
the median fails the 10 ms target; p99 meets 30 ms in this run. Own-hook p99 is
101 microseconds and remains above its budget. Working-set samples before input,
after input and after idle are 45,543,424 / 48,816,128 / 46,981,120 bytes; these
samples do not establish a universal peak-memory pass. Signed installation,
physical resilience, remaining performance budgets and the seven-day soak remain
open. The [detailed measurements](validation-performance.md#provider-demand-selection-native-validation-and-matched-static-pair)
preserve scope and comparison limits.

## Earlier renderer refinement, 13 November 2026

The earlier ordinary daemon was
`9608872A54F93B28221A918D48E2CB1B487884D10A2BCC1A19ECF4945D4FCE94`;
its UIAccess installation candidate was
`9ECFCE8BF68424B9DB46D344D4938445076FB1F886C9DD799D644C8815DC7040`.
That build used Settings
`EC22F24B07DFAD2ABE6198B519016E26E2C119843351FE81AEAC13EE9799727C`.
The UIAccess image was built and staged, without signing or installation. Build
identities are in `target/factory-{final,install,validation}-build.json`.

The ordinary 960 daemon ran through Explorer as the sole manager,
PID 10844, from `Downloads\Moon-validation\moon-factory-final-20261113\moon.exe`,
with the matching EC22 Settings companion beside it. Live verification preserved
all four original HWND/PID/process-birth identities and workspace assignments;
visible native frames matched the plan and prior focus was restored. The personal
configuration hash was unchanged. The token was medium integrity (8192), without
UIAccess, and no previous window manager was running. Evidence:
`target/factory-final-live.json`. This ordinary startup does not complete signed
installation or the seven-day soak.

That ordinary daemon passed all 14 lifecycle integration cases and ten
workspace-carousel cases: `target/factory-final-{integration,carousel}.json`.
The default 128-chord workload has 128 exact placements and no daemon error;
ready time is 120.659 ms, external median/p99 12.348/44.500 ms and own-hook p99
104 µs. Working-set samples before/after input/after idle are
45,961,216 / 49,188,864 / 47,472,640 bytes, with idle CPU 0.4687147% of one core.
Both opt-in profiling modes are disabled. These three memory samples are below
50,000,000 bytes; the earlier E9 run's image-residency variation and higher total
remain evidence against a universal memory pass. Latency and literal-zero CPU
requirements remain unmet. See `target/factory-final-performance.json` and the
[budget table](#performance-budgets).

The final **scoped visual review passes**: all 13 PNGs were inspected and their
hashes verified, all eight ring captures have continuous edges and corners,
all five bar hashes match D583, and the removed-split region has zero residual
pixels. Each image's ordering checks held before and after capture on attempt
one. Evidence: `target/factory-final-target-visual/{report,visual-review,pixel-review}.json`.
This covers explicit per-target inspection ordering on one 4K display at 125%,
not normal product Z-order or other physical DPI settings. The earlier E9 and
960 captures with the underlay below the lowest helper remain preserved as
partial results in `target/surface-pools-visual/` and `target/factory-final-visual/`.
The new complete initial ring is consistent with earlier fixture occlusion;
the identity of any intervening ignored window was not proved.

The tracked Explorer installation launcher passes six harmless parser/behavior
checks plus its cleanup check, including argument/stream preservation, exit-code
forwarding, expiration and foreign-owner rejection. These checks perform no
installation or UAC request: `target/explorer-launch-tracked-validation.json`.
Signed installation, physical resilience and the seven-day soak remain open.

### Earlier workspace-label candidate

The previous corrected ordinary daemon is
`D5831ED9F4C2DD5676D79EF1CD511D4EC379E582C5C4FFF7DADD6BBE5D006F4A`;
its UIAccess installation candidate is
`5EF1FB070BC48A5FC924728769B0DB572217F44958260B1D73B6EC8A849B23A6`.
Settings remains `EC22F24B07DFAD2ABE6198B519016E26E2C119843351FE81AEAC13EE9799727C`.
The installation candidate is built, not yet signed or installed.

Workspace buttons now account for their displayed label/icon and font size,
within the configured width limit. The Windows review caught and corrected
workspace 10 becoming `1…` at Arial 18. Borders reuse one brush per current ring,
and each bar canvas reuses its text format. Theme/font changes invalidate the
appropriate resources and removed rings release their cached brush.

This exact ordinary candidate passes 75 core tests, 27 Windows native unit
tests, 14 lifecycle integration cases and ten carousel cases. Five visual stages
cover initial styling, resize/focus, theme/font reload, window removal and style
restoration. All 13 captures were reviewed at 4K/125%; the explicit underlay
inspection option exposed complete gradients and corners without changing user
windows. This pixel review does not establish normal Z-order or other physical
DPI behavior. The unchanged hook implementation additionally passed its four
interactive cases on the preceding `8B7FC844…` binary.

Reports: `target/workspace-label-{normal,uiaccess}-build.json`,
`target/workspace-labels-native-units.txt`,
`target/workspace-labels-{integration,carousel,performance}.json` and
`target/workspace-labels-visual/{report,visual-review,pixel-review}.json`.
The latest performance budget results are below. Installation, physical
resilience and seven-day acceptance remain incomplete.

For this earlier D583 run, readiness was 134.267 ms, external placement
median/p99 12.183/43.941 ms, own/whole-hook p99 100/743 µs and idle CPU
approximately 0.3125% of one core. Working-set samples were
46,780,416 / 51,515,392 / 49,446,912 bytes before input, after input and after
idle; the after-input sample exceeded the memory budget. These historical
results remain distinct from the later 960 and 43FD measurements above.

At that verification, the D583 ordinary daemon was running as the sole manager
with its matching Settings companion available. All four existing application identities and
workspace assignments were preserved, visible native frames match the plan,
hidden windows remain hidden, original focus returned, and the personal config
hash is unchanged. No previous manager was started. Live verification:
`target/workspace-labels-live.json`.

## Polish baseline, 13 November 2026

The ordinary Windows daemon is
`E050F021CFA3643CAF33B0EE007525FD1676575D688A73706FC14A8979512BE5`;
Settings is
`EC22F24B07DFAD2ABE6198B519016E26E2C119843351FE81AEAC13EE9799727C`.
The corresponding UIAccess installation image is
`B7188D60E72E40ECB662E72E2D344E9EE840FEF8333BC5058941E3F9EFA75E25`.
Build identities are recorded in `target/polish-native-build.json` and
`target/polish-uiaccess-build.json`.

| Check | Observed result |
| --- | --- |
| Settings frontend | 31 browser scenarios pass, including initial normal/error-page readiness, dialog ownership, invalid-field recovery, keyboard save/focus, themes and narrow/short viewport reflow. |
| Portable and native units | 89 portable tests and 22 Windows native unit tests pass. Four interactive hook tests are explicitly ignored in this run; their earlier separate evidence is unchanged. |
| Native desktop behavior | 14 integration, four startup-assignment and ten carousel checks pass on this daemon. Carousel checks include focusing a hidden workspace, same-workspace focus without a slide, rapid switches and restoring original frames. |
| Native Settings | Eight checks pass with this exact Settings/daemon pair: save/apply, comment preservation, conflict retention, reload, Appearance and different-file isolation. The personal configuration is unchanged and cleanup reports no errors. |
| Setup | 32 isolated state/handler checks and 14 recovery checks pass. Install/Uninstall WPF views compile. Visual preview invokes neither Install nor Remove and requests no UAC prompt. |
| Build | Formatting, frontend type checking/build, and Windows-target Clippy with warnings denied pass. |

Reports: `target/polish-final-{integration,startup,carousel,desktop}.json`,
`target/polish-final-settings/report.json`, `target/polish-native-unit-results.txt`,
`target/settings-polish/report.json`, `target/setup-polish-validation.json`,
`target/setup-polish-recovery-validation.json` and `target/setup-polish-ui/report.json`.

During this baseline run the ordinary daemon was the sole window manager. Actual visits to
workspaces 1, 2 and 10 verified Arc, Windows Terminal, and ChatGPT plus Claude,
respectively, with exact visible native frames and correct hidden-window state.
The visits returned to workspace 10. See
`target/polish-final-workspace-visits.json`. No previous manager was restarted.

Settings now reveals after its initial page or error UI has been built. Both
native opening scenarios pass and close cleanly using owned temporary files.
Immediate `PrintWindow` captures can still precede WebView compositor drawing;
these checks do not prove a zero-blank-frame presentation. Evidence is retained
in `target/settings-opening-ready-after/` and
`target/settings-opening-invalid-after/`.

This candidate has not completed a signed installation. The physical resilience
matrix, performance budgets and seven-day soak below remain open; this polish
run does not supersede their measured results.

### Follow-up rendering and host validation

The subsequent resource-reuse candidate `B2ABB417…` creates eight gradient
brushes and one text format across the 128-chord workload, rather than 1,032
brushes. Both exact-placement runs pass, but the measured memory and latency
budgets remain unmet. Matched no-input controls also establish that the initial
startup snapshot is not a warmed memory baseline. Detailed candidate identities,
residency categories and measurement limits are retained in
[performance evidence](validation-performance.md#performance-investigation-13-november-2026).
The optional input-profiling candidate `8B7FC844…` passes 27 native unit tests;
all four interactive hook cases also pass separately with profiling enabled,
including all 52 default routes and preserving keyboard layout and key ownership.

The official Windows Enterprise evaluation ISO is downloaded and its SHA-256
matches Microsoft's published checksum. This prepares clean-machine testing;
it is not a completed VM or clean-first-run result. The ordinary host token
cannot enumerate or create Hyper-V guests. `target/acceptance-vm-download.json`
records the verified media and `target/acceptance-vm-capacity.json` records host
capacity. No guest count is inferred from the permission failure.

## Earlier SID-fix candidate, 12 November 2026

The earlier normal-permission daemon is
`DBADD0C08FA4C78AA1EF567520B593B7D3F68297BD3724C641CEDB082ECBCCAE`;
the matching Settings image is
`6A633A06B872765EA30AEDCBC9839F856FD3EA70BA5D855AC1BD85CCC8670E92`.
The UIAccess installation candidate is
`521E8BD9E926AF8108A722564452DF3970E5A935D496A7EF22D37AE865C1E219`.
Both applications and the installer now derive the IPC endpoint from the logon
SID. The measured linked UAC tokens had different authentication LUIDs but the
same logon SID; the earlier endpoint derivation separated those clients.

Four native IPC checks, including the linked-token identity check, and all 14
normal-daemon integration scenarios pass on this source/build. Evidence:
`target/sid-ipc-native-tests.txt` and
`target/live-sid-integration-results.json`. The matching Settings image also
passes all eight native checks with the normal daemon: save/apply, conflict
retention, reload confirmation, Appearance and different-file isolation. The
user's configuration was unchanged and cleanup recorded no errors. Report:
`target/sid-settings-ui/report.json`.

The ordinary live startup then adopted all four expected application windows:
Arc in workspace 1, Windows Terminal in workspace 2, and ChatGPT plus Claude in
workspace 10. At the settled observation, workspace 10 was active, its two
visible frames matched their placements, and Arc/Terminal were shell-cloaked.
The daemon remained medium integrity without UIAccess. See
`target/live-workspace-after.json` and `target/live-workspace-settled.json`.
Actual visits to workspaces 1, 2 and 10 then verified each expected visible set
and exact native frames, returning to 10; see `target/live-workspace-visits.json`.
Four isolated startup checks also pass for pre-existing windows' individual
workspace rules, native reveal/hide and original-frame restoration:
`target/startup-assignment-native-results.json`.

Installation is authorized. Earlier accepted installation attempts failed;
the diagnostic retry recorded rollback, and the retained diagnostic logs report
a signed-daemon readiness timeout. The latest SID-fix attempt was canceled at the Windows consent prompt;
the corrected launcher preserved cancellation code 1223 and handover recorded
`rolled_back_stopped`. Evidence is in
`target/live-install-sid-cancellation-fixed.log` and
`target/live-install-sid-cancellation-fixed-state.json`. This UIAccess candidate
has not completed installation or signed runtime
verification. [Installation evidence](installation.md#live-installation-attempts-and-sid-fix)
records the distinct outcomes. The requested no-fallback mode keeps the previous
desktop processes stopped during rollback. Startup reliability, signed privilege
checks, the physical resilience matrix and the seven-day soak remain open.

## Earlier product polish evidence, 12 November 2026

The recorded normal-permission Windows candidate is
`075E96CD3C7C95E24D04F07EC9BCFF7D53F74F60E2F6AC53B23B179C7DFBB134`.
Settings is
`7901563646D88335A3882DC0604691F5868D5F4BEA625D9C0D1479368A7BE050`.
The corresponding UIAccess installation image is
`9B85EBEEFD8404AC8E1FCB2F5395883D81CF1ADD8E7CCFE79488F7DD503B1546`;
its application evidence below predates the subsequent installation attempts.
All four distribution executables use the static C runtime. Setup checks the
separate WebView2 Evergreen prerequisite for Settings.

| Area | Recorded result |
|---|---|
| Configuration and persistence | 86 portable tests pass: 70 core, 10 CLI and six store cases. Windows adds five store cases and 18 native unit checks; four interactive hook cases are explicitly excluded from this run and retain their earlier separate evidence. |
| Settings interaction | 13 browser scenarios and eight live Windows checks pass. The native run proves comment-preserving save, applying the expected effective configuration, conflict retention, reload confirmation, Appearance rendering and refusing to reload a daemon using another file. Report: `target/polish-settings-ui/report.json`. |
| Native layout and recovery | All 14 integration scenarios passed on the recorded static-runtime candidate, including guarded close, restart, watchdog recovery and failed-startup recovery. Report: `target/polish-static-integration-results.json`. |
| Workspace motion | Workspaces are the carousel pages. A focused window activates its containing workspace; focus within that workspace does not animate. Ten native integration cases and nine grouped-thumbnail probe cases cover the behavior described in [carousel validation](validation-carousel.md). |
| Icons and setup | Two binary-resource checks and 60 isolated installer/registration/handover checks pass. Setup preview checks do not invoke Install or Remove. Multi-size icons and product identity are embedded in both application images. |
| Distribution | The packaging validator passes 14 isolated cases, including deterministic archive readback, input tampering, permission declarations, external-runtime rejection and partial-publication cleanup. [Packaging](packaging.md) describes strict validation of the final inputs. |
| Build and dependencies | Portable and Windows Clippy pass with warnings denied; frontend type checking, generated assets and formatting pass. Windows-target dependency policy checks pass. The cross-platform lockfile's two unused Linux dependency advisories are documented in [product polish](product-polish.md). |

These measurements do not establish signed-install, elevated-window, physical
resilience or seven-day-soak acceptance. Rows below retain their candidate
identity; they are not promoted to measurements of the SID-fix binary.

## Milestone checks

| Check | Observed evidence | Acceptance still requiring a measured result |
|---|---|---|
| S1: enumeration | Native probe finds all eight owned helpers. The same-desktop Glaze comparison accounts for every Glaze-managed HWND and explains ignored/shell-cloaked differences; four already-stranded ordinary windows were recovered by exact identity. | Covered on this Windows build; see platform evidence. |
| S2: cloak | Native shell cloak becomes `DWMWA_CLOAKED = 2`; uncloak returns zero. Integration verifies workspace visibility and forced-death restoration. | Covered on this Windows build. |
| S3: hotkeys | Alt+H, Alt+Shift+1 and Alt+Tab each match ten trials with the helper retaining foreground. Final ordered masking passes 22 renewal checks and 256 trigger checks across 128 burst chords with both release orders. Consumed chords preserve HKL and leak no shortcut or character events. All 52 default routes preserve foreground, HKL and native menu state. Key-pair ownership survives modifier changes and renewal; ordinary F24 still reaches other hooks. | Repeat after actual lock/unlock and verify no menu or task-switcher activation. |
| S4: focus | [100/100 hook-driven foreground changes](validation-applications.md) across three distinct medium-integrity processes: testwin, Windows About and Paint. World focus and the actual foreground HWND agree for every transition, including the final key-pair-corrected candidate 1A7F5020…. | Covered on the recorded candidate; elevated-window focus remains S8. |
| S5: placement | All 400 repeated batches have eight exact DWM frames. Async settled median/p99: 9.186/12.178 ms; deferred: 7.587/21.096 ms. The async path retains faster submission and better tail latency. | Full key-to-placement distribution and a comparative visual flicker assessment are separate checks. |
| S6: border underlay | Eight attributed rings have continuous corners and gradients at physical 125%; four physical clicks across the actual underlay reached a separate receiver process exactly once each. | Complete physical 100% and 150% visual checks. |
| S7: text | Native DirectWrite bar and Nerd Font glyphs render on the development display; font data is embedded. | Exercise a Windows installation without a system Nerd Font and retain the visual result. |
| M1: core and sandbox | Core invariants, reducer benchmark and all fourteen native integration scenarios pass on normal release candidate 1A7F5020…, including guarded close rejection, exact restart state, crash recovery and injected startup failure. | Covered for this normal release candidate; repeat on the installed signed candidate. |
| M2: config and workspaces | All 46 active GlazeWM chords have equivalent moon defaults; none are missing. Real rule fixtures, command parsing, live reload, cloak, restart, public IPC extension and parser fuzzers pass. Sixteen ordinary native regressions include terminated-process liveness, bounded IPC, atomic-replacement sharing, modifier state and border ownership. All 52 defaults route exact command payloads through the live hook without executing daemon effects. Five portable writer tests cover durable barriers, coalescing, error handling and shutdown. A source/data-flow review and 231 saved evidence files found no raw keyboard-event logging or pressed-state dumps; [scope and limits](validation-platform.md#input-logging-evidence) are recorded. | Exercise every binding's resulting behavior with moon as the sole manager. |
| M3: bar | Pure scene tests, provider probes, native rendering, centered pills, custom theme and default glyphs pass. | Record side-by-side visual parity and the full-process idle/performance measurements. |
| M4: borders | Pure ring geometry and physical 125% rendering pass; floating native tint is implemented. | Complete physical 100% and 150% scaling and multi-monitor visual checks. |
| M5: installation | The initial cancellation had verified full rollback. Later accepted attempts failed; the diagnostic retry recorded rollback and a readiness timeout. The latest SID-fix consent cancellation retained code 1223 and recorded `rolled_back_stopped`. [Attempt evidence](installation.md#live-installation-attempts-and-sid-fix) keeps these outcomes separate. | Complete installation and signed runtime verification of the current SID-fix candidate, then verify a running upgrade/rollback. Installation authorization exists; Windows consent is still required for an elevated attempt. |
| S8: privilege boundary | Medium-integrity IPC succeeds; low-integrity IPC is rejected. | With the installed signed binary, exercise elevated foreground hotkeys/placement and confirm launched applications are medium integrity without UIAccess. |
| M5: clean first run | No-config loading and native sandbox adoption pass. | Run the copied executable on a clean Hyper-V Windows VM with no config and confirm tiling. |
| M5: seven-day soak | No seven-day observation has elapsed. | Seven consecutive days as the sole manager, no lost windows, recorded uptime and papercuts, no fallback to GlazeWM. |

## Physical resilience matrix

For each row, retain the before/after workspace tree, active workspace, visible
native rectangles, cloak state and daemon process/revision. The running daemon
must stay usable and every owned test window must be recoverable afterward.

| Scenario | Current measured status | Completion evidence |
|---|---|---|
| Sleep and wake | Not exercised. | Suspend and resume the machine; hooks renew, windows retain membership, bar and placements recover. |
| Lock and unlock | Not exercised. | Lock and unlock the session; no placement or focus during the lock, hotkeys work afterward. |
| Explorer restart | Passed on final candidate 1A7F5020… after excluding shell/desktop handles from fullscreen detection. The same daemon retained four helpers, complete layout semantics and exact frames; AppBar reservation and tray registration returned. All five visible user applications survived. Cleanup restored original frames/work area. See `target/explorer-restart-results.json` and [platform evidence](validation-platform.md#actual-explorer-restart). | Covered on this Windows build. Tray API recovery is verified; visible-pixel icon position is not asserted. |
| Display added/removed | Core display-removal tests pass; physical hotplug not exercised. | Attach/remove a monitor with visible and hidden workspaces; all windows land on a surviving display. |
| Resolution and DPI changes | Geometry tests include 125%; physical changes not exercised. | Change resolution and scale, checking AppBar reservation, physical frame bounds and ring/text alignment. |
| UAC prompt | Actual consent prompts were entered during accepted and canceled installation attempts; controlled before/after hook and focus checks were not recorded. | Verify no unwanted focus and that normal bindings resume after leaving the secure desktop. |
| RDP session | Session-scoped journals implemented; RDP not exercised. | Connect/disconnect RDP, verify separate session ownership/recovery and restored local display geometry. |
| Native virtual desktop switch | Suspension and persistent-tree tests pass. The supported-API harness and read-only type/identity preflight are ready; three guarded attempts stopped before helpers or desktop shortcuts, most recently because the foreground was LockApp. [Preflight evidence](validation-platform.md#native-virtual-desktop-acceptance-preflight) records the corrected PowerShell/CIM checks and frozen source. Physical switching remains unexercised. | Run in an observed unlocked session with managed windows on both desktops; verify no forced switch back, off-desktop uncloak or membership loss and restore the original state. |
| WSLg windows | Host `.wslconfig` explicitly sets `guiApplications=false`; the distro has no X11/Wayland display. WSLg 1.0.79 and `xmessage` are installed, but enabling GUI support requires restarting WSL, including this build environment. | Tile, focus, hide, restore and close a real WSLg application. |
| WinUI 3 windows | Paint 11.2609.401.0 passed exact tile/float placement, hook-driven focus, hide/show and restoration on candidate 1A7F5020…. Its genuine Edit colours dialog subsequently passed six checks on E050F021…, including floating, workspace hide/show with the same runtime identity, cancellation and exact retiling. Loaded WinUI3 and the medium token were measured. [Application evidence](validation-applications.md) records the runs and preserved original desktop. | Application-window and transient-dialog behavior are covered on the recorded candidates. |
| Fullscreen games | Native borderless helper transition passes. | Exercise an actual fullscreen game and return to tiling without a persistent bar/border or manual-fullscreen flag change. |
| Multi-monitor actions | Pure monitor/workspace movement tests pass; physical multi-monitor not exercised. | Move windows and workspaces between real displays, including differing DPI; verify cursor jump and per-monitor overrides. |

## Performance budgets

These are the latest measured budgets for ordinary candidate `43FD9136…`.
Earlier candidate measurements remain in [performance evidence](validation-performance.md).
No signed-installation performance result is inferred from this ordinary run.

| Budget | Evidence available |
|---|---|
| Hook callback p99 under 50 µs | 101 µs p99 for Moon's own work and 886 µs for the whole callback. Both exceed the budget. The earlier 9 µs sample predates the key-pair correctness fix. |
| Eight-window key-to-placement median under 10 ms, p99 under 30 ms | 128/128 exact placements pass. Independent median/p99 are 11.944/29.496 ms. The median fails; p99 passes in this run only. |
| 50-window reducer plus layout under 100 µs | Passed: 22.59–22.93 µs in the recorded Criterion run. |
| Idle CPU 0%, no timer faster than 1 Hz | The ten-second default-bar sample is 0.156248% of one core. A separate 60-second Static interval is 0.078116%, with unchanged scene/layout/revision counters. Providers run at 1 Hz and unchanged scenes do not redraw; animation uses a bounded active-only timer. Neither result establishes literal zero CPU. |
| Working set under 50 MB, bar and borders on one 4K monitor | 45,543,424 bytes before the workload, 48,816,128 after it and 46,981,120 after idle. All three samples are below 50,000,000 bytes in this run. Earlier E9 repeats exceeded the budget with different executable-image residency; a universal bound or continuous peak below the limit is not established. |
| Ready in under 300 ms | 118.223 ms; passed on this ordinary candidate. The signed installation still needs its own result. |

The separate Static pair compares D973 with 43FD: 0.234353% versus 0.078116%
one-core CPU, and 429,596,270 versus 99,043,820 cycles. Both qualified intervals
have zero scene, draw, layout, commit and revision changes. This single sequential
pair uses quantized CPU-time counters and different text from the default bar;
it is not a universal or default-bar CPU improvement. Evidence:
`target/provider-idle-{1-baseline-static,2-candidate-static}/report.json`.

The default-workload results use normal-permission candidate `43FD9136…`, eight owned helpers,
the default WARP renderer, and a 3840×2160 display at 125% scaling, with the bar
and borders enabled. Raw records are in
`target/provider-demand-performance.json`. Optional input dispatch profiling and
target-window callback profiling were both disabled; the probe's aggregate
readback timing remains included. One device, two composition surface factories,
one border atlas, eight gradient brushes and one text format served
the workload, with no draw failure or device recreation. A previous, separate four-window workspace
sample observed all six full-width animations below the 50-pixel bar, with
47,575,040 bytes peak working set. Its command-to-overlay-disappearance interval
was 251.64–269.55 ms; this includes command handling and external sampling, not
just the nominal 200 ms easing. It does not replace the eight-window budget test.

Detailed results are in [native integration](validation-integration.md),
[platform verification](validation-platform.md),
[placement comparison](validation-placement.md), and
[surfaces/providers](surfaces-and-providers.md). Update this ledger when those
documents gain measured results; never infer a pass from implementation alone.
