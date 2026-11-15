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

## Signed Windows installation

The installed candidate includes commit `9079c6e`, which fixes internal background,
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

## Acceptance boundary

These results establish signed/UIAccess installation. They do not establish
elevated-app acceptance, clean-machine setup, every physical DPI/monitor/session scenario,
the controlled eight-window performance budgets, or the seven-day installed
sole-manager soak. Those remain the separate gates in the original v1 plan and
`acceptance-status.md`.
