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
Both were built from the current implementation. The unsigned UIAccess image is
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

## Acceptance boundary

These results do not establish signed/UIAccess installation, elevated-app
acceptance, clean-machine setup, every physical DPI/monitor/session scenario,
the controlled eight-window performance budgets, or the seven-day installed
sole-manager soak. Those remain the separate gates in the original v1 plan and
`acceptance-status.md`; ordinary promotion is not a signed installation.
