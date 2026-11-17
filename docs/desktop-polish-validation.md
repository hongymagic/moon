# Desktop polish validation — 15 November 2026

The six additions in `../.claude/plans/moon-desktop-polish.md` are implemented:
workspace Overview, rules from a selected window, one-step layout Undo, saved
setups, a theme library and desktop health controls. The UI retains the native
Windows caption and snap behavior, follows system appearance and accessibility
preferences, and uses compact empty-workspace drop targets after active and
occupied workspaces.

## Automated checks

- 148 portable Rust tests passed: 104 core, 15 daemon/CLI, 29 Settings.
- 30 Settings tests passed on Windows, including bounded UTF-8 preset reads,
  path validation and atomic export behavior. Platform-specific tests account
  for the difference from the portable Settings count.
- 70 browser tests passed, including ten-workspace ordering, mouse/touch dragging,
  keyboard movement and focus, invalid-config Overview, guarded recovery,
  preset sharing, draft preservation, high contrast and reduced motion.
- Portable workspace Clippy and Windows daemon/Settings Clippy passed with
  warnings denied. Formatting, bundled asset generation and generated config,
  reference and protocol comparisons passed. All 42 PowerShell scripts parsed
  in Windows PowerShell 5.1.26100.9818.

## Interactive Windows checks

The ordinary daemon image is
`B3164E8A5361161BED160A576DC595FCAA4217CA67BC2FEEAC2A78B445423EC7`.
The final Settings image is
`5C817B710BC5925969E1939C754F1ADA36E704B4952F9A2040D7F5E4E156EDA6`.
These images belong to the original polish candidate. Its unsigned UIAccess image is
`EADF82B3895FFE67456EF12D26D0AC0D808F6D1CF6F0BAA0A0F7FA5283AA74F8`.

The retained four-window desktop passed guarded promotion with all original
window/process births, workspace assignments, native frames, visibility,
foreground, recent workspace and configuration bytes preserved. Native actions
proved rejection of stale generations, revisions and process births; moving an
inactive window from workspace 1 to 3; exact one-step Undo; and pause/resume.
Focusing the remembered window on workspace 2 activated that workspace through
the carousel, then returned to workspace 10 with the original layout, focus and
recency. An active carousel was observed and retired; this is not a visual
smoothness or frame-time measurement.

The final Settings companion passed five live Overview checks: ordinary-token
launch through the real daemon, all four accessible window entries, native search
and empty-state behavior, leaving Overview, and unchanged desktop/configuration.
The captured Windows window showed actual DWM content and app icons, including
cloaked workspace windows. A separate owned document passed six native editing
checks before the final compact-card adjustment: Save, draft Undo, conflict
handling, reload confirmation, initial loading and Appearance.

The final companion passed five native theme-library checks: curated themes,
Apply/Undo, a named custom theme, harmless Open/Save cancellation, and a complete
file export/import round trip with an explicit preview before confirmation.
Only the owned fixture sidecar and export files changed. Personal configuration
and library hashes stayed unchanged and the owned Settings window closed cleanly.
The reusable harness is `scripts/validate-settings-library.ps1`.

PowerShell's UIA client initially exposed native dialog controls without their
standard patterns. An observed `LoadDefaultProxies` null-reference error required
one narrowly gated retry of Microsoft's public provider-registration API in the
test client. Open and Save also used different filename control identifiers.
The harness now selects the unique observed filename Edit by its supported value
pattern. These were validation-client failures; no Moon runtime workaround or
system-file modification was introduced.

Evidence is retained under `target/polish-20261115-enhancements/`, the browser
reports, and Windows `Downloads/Moon-validation/desktop-polish-*` receipts.
Earlier evidence remains attached to its original immutable image hashes.

## Initial signed Windows installation

The first installed candidate includes commit `9079c6e`, which fixes internal background,
watchdog and restart launches for UIAccess through the Windows shell broker.
The installer config-validation fix is in `15f8b16`. Ordinary launches retain
their existing behavior. The focused checks passed: 16 portable daemon/CLI tests,
Windows-target Clippy with warnings denied, three native process/argument tests,
and two native standard-stream cases using private log directories.

The new unsigned UIAccess image is
`04DBD64C98B2EC7E113641050968B81E978529D79F4DB2ED074DFF0945F2F88E`.
The independently verified package contains 35 payloads and 36 ZIP entries; ZIP
SHA-256 is `3D559B4E9EC474BAC321A4D2B1843D2C92C7B36D4DB0DFCFDE773493065BE282`.
Settings and the inventory/recovery helpers retain their previously verified bytes.

Windows approval completed and product handover `ada1303b353f424ab46a341454d5f384`
succeeded at 03:41 UTC on 15 November. Moon runs from `C:\Program Files\moon`
with a valid local signature, UIAccess enabled and elevation disabled. Its signed
image is `DB793217696E0EDE4ADB44A48973DAD18FD0861B7C18455A4DC42D88B56683B2`.
The startup task uses Limited run level. An ordinary client connected, a program
launched by Moon retained medium integrity without elevation or UIAccess, and a
low-integrity client was refused.

The separate coordinator did not pass its full desktop-history comparison. A
later readback proves exact semantic layout, original window identities, visible
frames, visibility, foreground and config bytes; remembered workspace differs
from the pre-install snapshot after additional user commands. It does not prove
that installation preserved recency. Original failure evidence is retained.

A real installed lifecycle restart at 03:49 UTC produced PID 42956 (native birth
134391881647308150) from the same signed image. The original test helper failed
after restart while comparing a PowerShell ordered dictionary and then repeated
that error while writing its report. Its saved baseline and failure remain
unchanged. A separate read-only audit verified both predecessor lifetimes had
ended, the current signed UIAccess successor and its sole same-image child, and
the successor's ownership of the recovery journal. All four native frames,
semantic layout, configuration, foreground and current recency exactly match the
pre-restart baseline; the successor reports no error. Child command-line arguments
were not observable from the ordinary verifier, so watchdog-role evidence is
limited to its image, parent, lifetime and journal relationship.

At 03:54 UTC the installed daemon passed a targeted focus round trip: Terminal on
workspace 2, then Codex on workspace 10. The carousel was observed active and
retired. Exact layout, native frames and visibility, foreground, configuration and
recency were restored without a daemon error. This verifies the exercised focus
path; it does not diagnose the earlier latched warning or measure animation frame
time. Receipts are `runtimefix-restart-completion-20261115/result.json` and
`runtimefix-installed-focus-20261115/result.json` under `Downloads\Moon-validation`.

## Settings usability follow-up — 15 November 2026

The ordinary Settings image
`A540262E44804321220B8D501C1021DEC199FEBB568D87FD3435E399D21B6270`
passed five native checks using `scripts/validate-settings-usability.ps1` and a
new private TOML document. The companion ran at medium integrity (8192), without
elevation or UIAccess. The coordinator separately confirmed that the personal
configuration and live layout were unchanged; the owned Settings process closed
without cleanup errors and foreground returned to the original window.

- The sign-in preference was present and honestly unavailable for the older
  installed helper. The test did not toggle it. The optional guarded
  `-VerifyStartupToggle` check awaits installation of the updated helper.
- A real Open-Meteo lookup returned **Sydney, New South Wales, Australia**.
  Explicit selection and Save persisted latitude `-33.86785`, longitude
  `151.20732` and the display label in the fixture. Merely searching did not
  save anything. Windows location permission was not requested.
- The workspace picker selected and saved the exact bundled Code glyph
  (`U+F121`). This establishes selection and persistence, not pixel rendering.
- Moving the Moon bar item changed the saved order to Clock, Moon, Custom text
  while retaining the custom command, styling and width.
- Choosing **Keep floating** saved `set-floating` while retaining the original
  app matcher. The common path used structured controls rather than command text.

The receipt and four native PNGs are in
`Downloads/Moon-validation/usability-settings-v4-worker-20261115/settings-fixture/`;
the parent `result.json` records the independent live configuration/layout check.
The earlier stopped attempts remain preserved: the validation client had to use
actual UIA associations and Toggle/ExpandCollapse patterns for stateful controls.
One early cleanup was blocked by the open icon dialog; cleanup now cancels only
that exact owned dialog before requesting a normal close.

Visual review of the native PNGs found readable weather, bar and rule controls,
but the immediate first icon-picker capture had blank glyph areas. Its labels and
successful codepoint save alone did not prove visible icons. The bundled local
font now preloads from the document head, and the native capture waits 500 ms
after the picker becomes accessible. Browser checks confirm the font is fetched
on General before the first picker and its bytes match the bundled font.

The rebuilt ordinary Settings image
`D46551D3366DBDEC0D6C81C9B191F18E02FD16E178F20D508E3FCE30B0A1DE90`
then passed all five native checks again, with no cleanup errors and unchanged
personal configuration and live layout. Direct inspection of the new native
`icon-picker.png` confirms all 12 distinct glyphs are clearly visible, including
Code, Terminal and the icons in the bottom row; none are blank or replacement
squares. Its SHA-256 is
`A044F66B4CAFAAECCC023C8C0E1068A4EF2F572A660951F50D39A2096A50B6F3`.
Final receipts and PNGs are under
`Downloads/Moon-validation/usability-settings-v5-worker-20261115/`.
This verifies the observed settled native rendering; it does not claim a
zero-delay first frame or isolate the preload's timing effect from the capture
settle. The other PNGs include transient status/toast messages and are not
final-state visual baselines.

After the attribution and preload additions, the complete frontend suite passed
82/82 checks. Formatting, TypeScript checking, the checked-in bundle build, both
portable and Windows all-target Clippy checks, and 158 portable Rust tests also
passed. The separate native carousel lifecycle suite passed 4/4.

The one-time Windows request follows the documented foreground/UI-thread
requirements of [Geolocator.RequestAccessAsync](https://learn.microsoft.com/en-us/uwp/api/windows.devices.geolocation.geolocator.requestaccessasync?view=winrt-26100).
City choices include region/country to disambiguate results; the source credits
link to [Open-Meteo's licence](https://open-meteo.com/en/licence) and
[GeoNames](https://www.geonames.org/), as described by the
[geocoding API](https://open-meteo.com/en/docs/geocoding-api).
The picker uses the same licensed Symbols Nerd Font as Moon's bar rather than
assuming private-use codepoints are interchangeable with a system font; see
[Microsoft's icon-font guidance](https://learn.microsoft.com/en-us/windows/apps/design/iconography/segoe-fluent-icons-font).
The early font fetch uses the standard [HTML preload mechanism](https://html.spec.whatwg.org/multipage/links.html#link-type-preload).

## Usability package and approval outcome

Source commit `ad07642` is packaged in
`C:\Users\dhong\Downloads\Moon-0.1.0-2026-11-15-usability`.
The strict release has 35 payload files and a verified ZIP with SHA-256
`A24903B3167BC2F6374F058F5797B36FCEBFD90E9CD95E71EDCF4AB092C0C69E`.
The unsigned UIAccess input is
`34B41CBFC5FF878EA130C2F6834A45ED688C2BD2A1755F9F8FB933B650C6C220`;
Settings is the native-verified `D46551D…B0A1DE90` image above.

Upgrade operation `d4b39145b4c54905abd920da651ab208` reached the real Windows
approval prompt at 04:37 UTC. Windows returned cancellation; the product recorded
`cancelled`, and the independent coordinator retained its failure receipt.
The new package was **not installed by that attempt**. A fresh read-only audit at 04:40 UTC
confirmed the existing PID 42956 still manages the same four windows without a
daemon error, using unchanged personal configuration SHA-256
`055E3A6AC39BE61B9740810E21AAB6F44F8F19168356862D8EE5A9B3A175B832`.
No alternate desktop manager was launched. At that point the installed startup
toggle and 128-shortcut stress run were still pending; their prepared harnesses
were not evidence of execution. Receipts are retained beneath
`target/usability-20261115` and `Downloads\Moon-validation\usability-upgrade-worker-20261115`.

The unchanged package was subsequently installed successfully. Operation
`ece5416c88324ee1a21a4bf82d632eab` completed at **08:18:27 UTC** on 15 November,
with installer exit code 0, product status `succeeded`, and a passing independent
coordinator result. Source is `ad07642`. The new daemon is PID **35052**, native
birth **134392043063996486**, running from `C:\Program Files\moon\moon.exe` with
UIAccess enabled, elevation disabled and integrity RID 12288 in session 1.
It reports four managed windows, no last error, and no suspended session.

| Installed signed image | SHA-256 |
|---|---|
| Moon | `D3ABC1DE88325EDAF72983AB1BB34C44451627202C40E96F93271FE986A1168F` |
| Settings | `55C97E722E6E2A3DEDE5577AC2F6EE6B37ACDE44DF7E7C7863CF369A48237632` |

An independent file comparison matched each signed executable to the package's
unsigned input after accounting only for the PE checksum, certificate-directory
entry and appended signature. The installed support scripts match the package
text followed by their Authenticode blocks. The coordinator verified the daemon's
valid signature, unchanged semantic layout and selected configuration, unchanged
installation identity, and exit of the old PID 42956 lifetime. Its baseline config
hash was `055E3A6A…5B832`; this is an upgrade-time comparison, not a claim that later
user edits cannot change the file. The layout comparison covers planned geometry,
membership and focus; it does not independently compare native window/process
births, actual frames or recent-workspace history.

Startup remained enabled. The task's owner, group and all previous access entries
were retained; the only added entry grants the installation owner `0x1201BF`
(read, write and execute), without granting delete, permission-changing or
ownership-changing rights. No other principal received a new grant. Evidence is
`Downloads/Moon-validation/usability-upgrade-ready-worker-20261115/`
`{before,installer-process,installer-status,result,completion}.json`.

At **08:23:45 UTC**, an ordinary worker (integrity 8192, no elevation or UIAccess)
used the signed installed helper to read the startup preference, disable it, then
restore it to enabled. All three states were confirmed. PID 35052 remained the
same authenticated daemon with no error, and configuration bytes were unchanged
across that check. Its receipt is
`Downloads/Moon-validation/usability-installed-startup-backend-20261115/result.json`.
This proves the installed backend works without elevation. It does not exercise
sign-out/sign-in or reboot behavior.

The installed Settings image then passed all five native usability checks,
completing at **08:33:02 UTC**. The startup switch was activated by two guarded
clicks on its verified control: enabled → disabled → enabled. Both the visible
UI state and independent Task Scheduler COM readback confirmed each transition.
The same run passed the live Sydney lookup/explicit selection, bundled icon,
bar-order and structured floating-rule checks using its owned fixture document.
The coordinating agent inspected the installed icon-picker screenshot and
confirmed all 12 glyphs rendered correctly.
Settings ran at ordinary integrity 8192 without elevation or UIAccess, closed
without cleanup errors, and returned foreground to HWND 393448.

The coordinator verified unchanged personal configuration and live layout across
the run; only the test's owned configuration was edited. Its personal baseline
hash was `3BF5AA31799578B17DA42C992EE2C827D9E9F4D7866C711E604BF28ACE79625A`.
PID 35052 remained healthy with no last error and five managed windows at this
later observation. Receipts are
`Downloads/Moon-validation/usability-installed-settings-click-20261115/`
`{result,completion}.json` and `settings-fixture/report.json`.

Earlier validation attempts remain recorded. Calling UIA Toggle did not activate
the WebView switch; the subsequent keyboard approach stopped because UIA
SetFocus did not establish the required control focus, before sending input.
These were harness/provider limitations, not established product defects. The
successful guarded-click run does not claim that either UIA pattern works or
that keyboard accessibility was verified by those failed attempts.

Windows canceled the separate high-integrity shortcut-test approval at
**08:31:09 UTC**, before the elevated child or test body ran. Its receipt is
`Downloads/Moon-validation/usability-installed-workspace-burst-high-20261115/completion.json`.
The revised 128-shortcut burst remains unexecuted pending fresh user readiness;
neither the canceled prompt nor the Settings checks establish that result.

## Acceptance boundary

These results establish signed/UIAccess installation. They do not establish
elevated-app acceptance, clean-machine setup, every physical DPI/monitor/session scenario,
the controlled eight-window performance budgets, or the seven-day installed
sole-manager soak. Those remain the separate gates in the original v1 plan and
`acceptance-status.md`.
