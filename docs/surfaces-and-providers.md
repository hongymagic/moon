# Native surfaces and providers

The main thread owns `Surfaces`. It renders immutable `moon-core::scene` values
using one shared Direct3D 11 / Direct2D / DirectComposition device and DirectWrite
text. Each monitor has a bar and a click-through border underlay. Bar clicks only
enqueue commands. Notification-area controls use that same queue. Unchanged
scenes do not redraw. GPU device loss recreates the graphics resources once and
reports any subsequent failure to the daemon.

DirectComposition surfaces retain the composited image without an animation
loop. Only drawing updates acquire a DXGI render target. Font bytes are embedded
and loaded through a private in-memory DirectWrite collection, so icon glyphs do
not depend on the user's installed fonts. The asset source, pinned release,
checksum and MIT license are in `crates/moon-win/assets/`.

Each monitor has one layered underlay HWND and one reusable border atlas.
Long edge strips are split into slices no longer than 1,024 pixels, with vertical
slices transposed into horizontal atlas rows. Longest-first shelf packing bounds
unused space. Child visuals reference clipped regions of that shared atlas;
their transforms put every slice back into physical monitor coordinates. Atlas
capacity grows only when needed. Global gradient coordinates stay consistent
across slices, including rounded corners. A provider-only bar update does not
redraw borders. All surfaces share one main-thread Direct2D device context.
Its reusable texture-cache limit is zero; this does not limit live
DirectComposition textures or total process working set.

The default is WARP software rasterization with
`D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS`, which avoids
WARP's additional raster-worker pool for this small, infrequently updated scene.
DirectComposition still composes the visuals. Changed scenes commit once and
wait for that commit to finish processing before accepting another graphics
update, bounding queued buffers. Unchanged scenes neither draw nor wait. Explicit
`MOON_GRAPHICS_DRIVER=hardware`, `MOON_WARP_SERIAL=0`, and
`MOON_COMPOSITION_WAIT=0` overrides remain available for controlled diagnostics.
If explicitly requested hardware initialization fails, creation falls back to
WARP. The embedded font has a retained lifetime owner, avoiding DirectWrite's
otherwise unnecessary copy of its static bytes.

Actual cross-process mouse injection proved that
`WS_EX_LAYERED | WS_EX_TRANSPARENT` passes clicks through, while a disabled,
nonlayered HWND did not. Compact per-edge HWNDs did not materially improve memory
in the comparison, so the implementation retains the plan's one-underlay-HWND
design. The major additional allocation came from WARP's lazy raster-worker
pool and queued composition frames.

The AppBar reserves the configured height and removes that reservation on drop.
It re-registers when Explorer restarts. The AppBar and tray allow only their
required callback and Explorer-restart messages through the per-window message
filter when running with UIAccess. Fullscreen hides the bar and releases
its reservation. Native DWM edges are suppressed for ringed tiled windows and
colored for floating windows; original edge colors are restored on shutdown.
The recovery journal independently preserves those original colors for crashes.
The `system` backdrop maps to Windows' automatic backdrop selection; `mica` and
`acrylic` map to the main-window and transient-window backdrops respectively.

A separate provider worker samples local CPU, memory, network, battery, volume,
keyboard language and clock values no faster than once a second. Media metadata
uses pending WinRT operations and cancellation after five seconds; it never
waits for a player on the message-loop thread. Missing hardware stays absent;
failed providers expose `<provider>.error` instead of fabricated values.
`system.theme` follows Microsoft's documented `UISettings` foreground-luminance
test and supplies `dark` or `light` to both the bar palette and native floating
window border colors.

`exec` widgets run through the medium-integrity desktop shell. The capture
worker joins a kill-on-close Windows job before executing the command. Output
is bounded to 4 KiB and runtime to five seconds. Results are cached for each
widget's configured interval. Reloading a changed command invalidates its cache.

Weather makes no request unless a location is explicitly configured. It uses
Open-Meteo HTTPS with Windows certificate validation, finite request timeouts,
a bounded response and a location-specific cache (15 minutes by default).
Open-Meteo weather data is attributed by `weather.source`. No API key is needed.

## Reproducible Windows probes

Build these examples for the Windows MSVC target with `cargo xwin build -p
moon-win --examples --target x86_64-pc-windows-msvc`. They are Windows-only; Linux
builds expose a diagnostic entry point without importing Windows APIs.

- `providers_probe.exe` asserts actual local provider output, weather absent by
  default, ordinary shell command output, timeout termination and output bounds.
  `--weather` explicitly fetches at the public test coordinate 0,0, without
  transmitting a personal location.
- `surface_smoke.exe 45` renders for 45 seconds, prints resource counts and then
  releases its windows and AppBar reservation. It manages only already-open
  `testwin` windows and restores their original placement through an owning
  guard. Run `testwin.exe 2` before it to inspect rings.
- `memory_probe.exe providers` and `memory_probe.exe graphics` isolate the
  local-provider worker and initialized graphics device. Setting
  `MOON_GRAPHICS_DRIVER=hardware` selects the hardware Direct3D device for comparison.
- `daemon_performance.exe moon.exe testwin.exe` measures the actual daemon with
  eight owned windows, 128 injected resize chords, and a ten-second idle period.
  It reports whole-process memory, startup, hook and observed placement latency.
  `MOON_PERF_NO_SURFACES=1` disables the bar and borders for an isolation run.
- `input_transparency_probe.exe testwin.exe` injects clicks at all four ring edges
  into an owned window in another process and verifies the receiver counted them.

Visual tests should isolate moon from tacky-borders to attribute the displayed
rings. They should also cover real monitor scaling at 100%, 125% and 150%; a
simulated scene scale does not prove the monitor DPI behavior.

The isolated release probes on the development machine (one 3840×2160 monitor,
125% scaling, RTX 3070 Ti) measured 19,886,080 bytes for providers, 45,486,080 bytes
for hardware graphics initialization, and 22,093,824 bytes for WARP graphics
initialization. These are whole process working sets, not private-byte figures
or estimates with shared pages subtracted. They do not by themselves establish
the full application's 50 MB budget.

The current packed-atlas release profile with serial WARP and composition waits
was also measured in the full eight-window daemon: 48,893,952 bytes working set
after 128 resize chords, approximately 120 ms startup, and 2,865,152 nominal
surface bytes before, after, and following idle. Nominal surface bytes count
allocated RGBA surface capacity, not compositor copies or total memory. The
working-set result satisfies the strict 50,000,000-byte budget on this machine;
it is not a promise for other driver, monitor, or window configurations.

The latency requirement remains unmet: a subsequent full run measured external
key-to-placement median 14.149 ms and p99 55.182 ms. The documented measurement
polls native frame rectangles at one-millisecond intervals. Hardware, no-wait,
and no-surfaces comparisons must use the same immutable daemon and polling
protocol; they have not yet been completed. No latency acceptance or soak
completion follows from the memory result.

Actual visual inspection at 125% monitor scaling confirmed eight window rings,
continuous gradients, rounded corners, the bar, pills, and bundled icon glyphs.
The four-edge cross-process click test passed. An atlas packing test verifies
pixel-area preservation and nonoverlapping source and destination slices across
mixed horizontal and vertical strips. Physical 100% and 150% monitor scaling
remain to be verified; simulated geometry is not equivalent evidence.

## Sources checked on 2026-11-12

- [DirectComposition initialization](https://learn.microsoft.com/en-us/windows/win32/directcomp/initialize-directcomposition)
- [DirectComposition surface drawing lifetime and offsets](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositionsurface-begindraw)
- [Visual clipping and transforms](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositionvisual-setclip(constd2d_rect_f_))
- [Composition completion](https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-waitforcommitcompletion)
- [Direct3D device flags and WARP raster workers](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_create_device_flag)
- [Direct2D texture-cache limit](https://learn.microsoft.com/en-us/windows/win32/api/d2d1_1/nf-d2d1_1-id2d1device-setmaximumtexturememory)
- [In-memory font ownership](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nf-dwrite_3-idwriteinmemoryfontfileloader-createinmemoryfontfilereference)
- [DirectWrite factory and custom font collections](https://learn.microsoft.com/en-us/windows/win32/api/dwrite_3/nn-dwrite_3-idwritefactory3)
- [Windows AppBar registration and notifications](https://learn.microsoft.com/en-us/windows/win32/shell/application-desktop-toolbars)
- [Per-window message filtering](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-changewindowmessagefilterex)
- [Notification-area versioning and Explorer behavior](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw)
- [Media session manager API](https://learn.microsoft.com/en-us/uwp/api/windows.media.control.globalsystemmediatransportcontrolssessionmanager.requestasync)
- [Windows light/dark theme detection](https://learn.microsoft.com/windows/apps/desktop/modernize/apply-windows-themes)
- [System backdrop selection](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type)
- [WinHTTP finite timeouts](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpsettimeouts)
- [Open-Meteo current-weather parameters](https://open-meteo.com/en/docs)
- [Nerd Fonts v3.5.1 release](https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.5.1)

Exact Rust method signatures were checked against the downloaded `windows`
0.62.2, `windows-future` 0.3.2 and `windows-numerics` 0.3.1 crate sources before
cross-compilation. These compatible versions are pinned in Cargo.toml/Cargo.lock.
