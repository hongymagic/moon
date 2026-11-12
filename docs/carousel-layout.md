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
