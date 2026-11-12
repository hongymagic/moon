# Windows canvas motion: implementation constraints

Research checked against current Microsoft documentation on 12 November 2026.
The selected implementation is the owned DWM presentation described below.
Its native fixture results are recorded in [carousel validation](validation-carousel.md);
those results do not establish every application's compatibility or the full
daemon's animation performance.

Each workspace retains its tiled and floating window rectangles. Switching
workspaces translates every outgoing window together and every incoming window
together, within that monitor. Focusing a hidden window activates its containing
workspace. Focusing another window on an already visible workspace does not move
the canvas. `workspace-animation` controls the optional presentation; it does not
select a different layout. Fullscreen windows and native virtual desktops keep
their existing semantics.

## Actual windows and visual previews

| Approach | What it provides | Constraint |
|---|---|---|
| Repeated `SetWindowPos` | Moves the actual application HWND; native input, menus and accessibility continue to refer to that window | Final native coordinates arrive after the animation, and asynchronous application message processing can make windows advance unevenly |
| DWM thumbnails in an owned overlay | Live 2-D application representations with explicit source/destination rectangles and opacity; final HWND placement can happen immediately behind the overlay | A thumbnail is presentation, not a replacement interactive application HWND; hidden/cloaked/protected-source behavior requires an actual probe |
| DirectComposition transform on a foreign HWND | No general documented operation for ordinary application windows | Composition targets belong to the caller, and `CreateSurfaceFromHwnd` specifically needs a layered source |
| Reparent external applications into one canvas | A parent can clip child contents | Changes application window relationships and styles; cross-process `SetParent` can reset the child process's DPI awareness |

Microsoft documents [asynchronous window placement](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos),
[live DWM thumbnail relationships](https://learn.microsoft.com/en-us/windows/win32/dwm/thumbnail-ovw),
[thumbnail source/destination rectangles](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ns-dwmapi-dwm_thumbnail_properties),
[caller-owned composition targets](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondesktopdevice-createtargetforhwnd),
[layered-window composition surfaces](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-createsurfacefromhwnd)
and the [cross-process DPI consequences of reparenting](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setparent).
The distinction between native motion and an interactive proxy is an inference
from those contracts; the thumbnail API supplies no input-forwarding contract.

## Selected presentation

`moon-win::carousel::Carousel` prepares one opaque, nonactivating popup per affected
monitor after the native-state journal is durable. It registers live DWM
thumbnails for the outgoing and incoming workspace groups, draws the old
workspace and calls `DwmFlush` before the daemon commits the new native positions
and cloak state. The final application HWNDs then remain stationary while a
200 ms cubic ease-out translates the two workspace canvases.
Source crops preserve the visible frame and gaps; the owned popup clips everything
to the monitor work area, below the bar. Incoming and outgoing workspaces may be
empty. Thumbnail source dimensions and frame crops are refreshed on every draw
so an asynchronously resized incoming tile does not retain its old source crop.

The initial flush deliberately waits for the caller's queued presentation. It is
not a guarantee that all applications have completed their drawing. Animated
mode therefore includes a compositor wait before final native placement and must
be measured separately from snap placement. Neither its 200 ms duration nor its
logical commit time can be relabeled as satisfying the original placement
latency budget. See Microsoft's [DwmFlush contract](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmflush).

The daemon pumps at 16 ms only while a visual transition exists. Rapid repeat
settles the native state and removes the previous presentation; it does not start
from a stale thumbnail. Once visuals and a possible consumed-button release are
finished, there is no carousel mouse hook or fast idle timer.

Mouse movement cancels the presentation. The first mouse press that begins within
a visible overlay dismisses it and consumes that press and matching release.
The next click reaches the actual application. No click is replayed or forwarded
to a representation that may not match the native target. An already-held mouse
button suppresses animation, checked both before preparing and immediately before
showing. This uses the current-down bit documented by
[GetAsyncKeyState](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getasynckeystate),
not its unreliable pressed-since-query bit.

## Monitor edges and recovery

Moving a real top-level HWND beyond one monitor can put it on a neighboring
monitor. Negative coordinates are legitimate desktop coordinates, not a reliable
hiding mechanism. Microsoft's [multi-monitor guidance](https://learn.microsoft.com/en-us/windows/win32/gdi/multiple-monitor-applications-on-different-systems)
explicitly warns about that assumption.

`SetWindowRgn` can clip a partially visible window to its assigned viewport, but
that is a mutation of application-owned state. It sends window-position
notifications and transfers ownership of the supplied region handle to Windows.
`GetWindowRgn` can copy an existing region, though its error result means either
no region or failure. A production clipping implementation therefore needs
careful capture, fail-closed admission and durable restoration, including shaped
windows; it is not a free compositor crop. See
[SetWindowRgn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowrgn)
and [GetWindowRgn](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrgn).

The native cross-process probe confirmed that both shell-cloaked and app-cloaked
GDI sources continue updating in DWM thumbnails on this Windows build. An ordinary
redirected popup also supplies the required opaque background. A no-redirection
destination displayed thumbnails but did not display its GDI background, so it
was not selected. This is observed compatibility, not a promise that every
application supplies usable protected or minimized content.

Both implementations must settle or abort on monitor changes, fullscreen,
lock/secure desktop, dragging, menus, window destruction, or native virtual-desktop
changes. Preserve existing HWND/process ownership and current-desktop checks;
journal original native state before the first mutation. The watchdog restores
real windows after process death; owned overlays disappear with their process.
Never move a user to another native virtual desktop automatically, consistent with
[Microsoft's virtual-desktop guidance](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager).
Honor the system's [client-area animation preference](https://learn.microsoft.com/en-us/windows/win32/winauto/client-area-animation),
and keep instant placement available for reduced motion and high contrast.
