# Workspace carousel

Moon keeps every workspace tiled. Switching workspaces slides the outgoing and
incoming workspace canvases together, including their tiled and floating windows.
Selecting a window on another workspace activates that workspace. Focusing a
window on an already visible workspace changes focus without rearranging its
tiles or starting a workspace transition.

`workspace-animation = true` is the default. Set it to `false` in the configuration
or turn off workspace animation in Moon Settings for immediate switches. Windows
reduced-motion and high-contrast preferences also disable animated presentation.
There is no separate window-per-page layout.

A workspace change during motion continues from the currently displayed positions.
Reversing direction returns to an already visible workspace without first snapping
to the previous destination. Only the visible pages and one incoming page are
retained, with a maximum of three pages per monitor; there is no queue of obsolete
transitions. The 200 ms easing duration is unchanged. Native focus and final window
placement commit immediately, independently of the visual interpolation.

Pointer input, display changes and invalid source windows cancel presentation.
The animation preference is rechecked during an active transition so disabling
Windows animations also ends existing motion. Idle loops do not query this
preference. Microsoft documents
[the client-area animation setting](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)
and [updating existing DWM thumbnail relationships](https://learn.microsoft.com/en-us/windows/win32/dwm/thumbnail-ovw).

Workspace membership, split direction, tile proportions, floating rectangles and
fullscreen state remain ordinary window-manager state. The core Plan describes
final window rectangles and visibility. Native presentation groups the windows
of each workspace for the transition; it does not reparent application windows or
change their tile positions to create pages. Other monitors retain their active
workspaces when one monitor switches.

Reducer regression tests cover activating a hidden workspace by focusing one of
its windows, preserving all tiled siblings and tree order, changing focus within
visible workspaces on two monitors, floating/fullscreen state, and retaining
resized tiling through animation-setting reloads and restart serialization. These
checks establish state and final geometry, not native animation timing or visual
clipping; those require the separate Windows integration checks.

Portable interruption checks cover a third destination, reversal, bounded retention
over repeated changes and final committed focus/layout. The hidden-popup cleanup
regression passed on Windows on 15 November 2026, covering cancellation, removed
monitors, disabled motion, expiry, stale sources, subsequent reuse and destruction.
It installs no input hook and never shows a popup. The updated rapid-interruption
visual probe still requires execution; earlier captures validate the previous
cancellation behavior, not this refinement.

## Live workspace roundtrip, 15 November 2026

Ordinary daemon
`512515BF2BB41FFC95776ECCEF26064076B865AE446CCE6DF9136DDC09883B8E`
completed a guarded workspace 2 → 10 roundtrip as PID 48436. The check retained the
same daemon lifetime, verified the four original application/window identities,
and used public workspace commands without changing animation preferences or
personal configuration. Each leg waited for the expected layout, native foreground,
visible window frames, inactive-window cloaking and settled presentation.

Four of nine bounded query samples reported `carousel_active=true`: two during
each transition, followed by an inactive sample at each settled destination.
The complete final layout fingerprint matched the original capture. Foreground
and remembered focus returned to window 393448, and `recent_workspace` returned
to ID 1 (workspace 2). The roundtrip also repaired an inactive workspace's missing
remembered focus and null recency left in the prior coordinator snapshot.

The first check stopped before sending commands because it expected the original
recency at startup; the current promoted snapshot instead had null recency. The
corrected check accepted that observed initial state while retaining the original
final-state requirements. This was a validation precondition correction, not a
change to Moon's production behavior.

The passing receipt is
`C:\Users\dhong\Downloads\Moon-validation\polish-focus-restore-20261115-v2\result.json`
(and its retained copy `target/polish-20261115-final/workspace-roundtrip-result.json`).
It explicitly records `carousel_visual_proof=false`. These samples demonstrate
that the live presentation activated and settled with preserved final geometry;
they do not establish smoothness, clipping quality, rapid reversals, physical
reduced-motion transitions or timing/performance acceptance. Signed UIAccess and
elevated-window coverage also remain separate checks.
