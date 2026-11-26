//! Main-thread DirectComposition surfaces. All visible contents come from core scenes.
//! A shared D3D/D2D device serves every monitor; unchanged scenes are never redrawn.
use moon_core::{
    geometry::{Point, Rect},
    ids::{MonitorId, WindowId},
    scene::{self, BarItem, BarScene, BorderScene, Scene},
    world::World,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HMODULE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Direct2D::{Common::*, *},
            Direct3D::*,
            Direct3D11::*,
            DirectComposition::*,
            DirectWrite::*,
            Dwm::*,
            Dxgi::{Common::*, *},
            Gdi::{
                COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW,
                COLOR_WINDOWTEXT, GetSysColor, HBRUSH, ValidateRect,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            Controls::*,
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            Input::KeyboardAndMouse::{
                ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
            },
            Shell::*,
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, Interface, PCWSTR, PWSTR, Result, w},
};
use windows_numerics::{Matrix3x2, Vector2};

const APPBAR_MESSAGE: u32 = WM_APP + 41;
const TRAY_MESSAGE: u32 = WM_APP + 42;
const FONT: &[u8] = include_bytes!("../assets/SymbolsNerdFont-Regular.ttf");

/// Sizes observed on temporary BeginDraw surfaces, not allocation sizes or
/// simultaneously live buffers. DirectComposition may return shared backing.
#[derive(Clone, Default, serde::Serialize)]
pub struct SurfaceSizeObservation {
    width: u32,
    height: u32,
    sample_count: u32,
    observations: u64,
}

#[derive(Clone, Default, serde::Serialize)]
pub struct SurfaceDrawDiagnostics {
    begin_draw_calls: u64,
    begin_draw_failures: u64,
    descriptor_failures: u64,
    max_requested_pixels: u64,
    max_observed_backing_pixels: u64,
    sizes: Vec<SurfaceSizeObservation>,
    unlisted_size_observations: u64,
}
impl SurfaceDrawDiagnostics {
    fn observe(&mut self, width: u32, height: u32, sample_count: u32) {
        const SIZE_LIMIT: usize = 16;
        self.max_observed_backing_pixels =
            self.max_observed_backing_pixels.max(u64::from(width) * u64::from(height));
        if let Some(size) = self.sizes.iter_mut().find(|size| {
            size.width == width && size.height == height && size.sample_count == sample_count
        }) {
            size.observations = size.observations.saturating_add(1);
        } else if self.sizes.len() < SIZE_LIMIT {
            self.sizes.push(SurfaceSizeObservation {
                width,
                height,
                sample_count,
                observations: 1,
            });
        } else {
            self.unlisted_size_observations = self.unlisted_size_observations.saturating_add(1);
        }
    }
}

/// Bounded lifetime counters and a current retained-layout count. Size observations
/// cannot be summed into resident memory or a live-buffer count; no graphics
/// pointers or contents are retained by these diagnostics.
#[derive(Clone, Default, serde::Serialize)]
pub struct RendererDiagnostics {
    render_calls: u64,
    changed_scenes: u64,
    commits: u64,
    devices_created: u64,
    surface_factories_created: u64,
    device_recreation_attempts: u64,
    bar_canvases_created: u64,
    border_canvases_created: u64,
    border_atlases_created: u64,
    text_formats_created: u64,
    text_layouts_created: u64,
    text_layouts_retained: usize,
    gradient_brushes_created: u64,
    bar: SurfaceDrawDiagnostics,
    border: SurfaceDrawDiagnostics,
    feedback: SurfaceDrawDiagnostics,
}
#[derive(Clone, Copy)]
enum DrawKind {
    Bar,
    Border,
    Feedback,
}
impl RendererDiagnostics {
    fn draw(&mut self, kind: DrawKind) -> &mut SurfaceDrawDiagnostics {
        match kind {
            DrawKind::Bar => &mut self.bar,
            DrawKind::Border => &mut self.border,
            DrawKind::Feedback => &mut self.feedback,
        }
    }
}

struct Graphics {
    backend: &'static str,
    context: ID2D1DeviceContext,
    composition: IDCompositionDevice,
    bar_surfaces: IDCompositionSurfaceFactory,
    border_surfaces: IDCompositionSurfaceFactory,
    write: IDWriteFactory5,
    symbols: IDWriteFontCollection,
    loader: IDWriteInMemoryFontFileLoader,
    diagnostics: Rc<RefCell<RendererDiagnostics>>,
}

impl Graphics {
    fn new(diagnostics: Rc<RefCell<RendererDiagnostics>>) -> Result<Self> {
        unsafe {
            let mut device = None;
            let create = |driver, device: &mut Option<ID3D11Device>| {
                let flags = if driver == D3D_DRIVER_TYPE_WARP
                    && std::env::var("MOON_WARP_SERIAL").as_deref() != Ok("0")
                {
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT
                        | D3D11_CREATE_DEVICE_PREVENT_INTERNAL_THREADING_OPTIMIZATIONS
                } else {
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT
                };
                D3D11CreateDevice(
                    None,
                    driver,
                    HMODULE::default(),
                    flags,
                    None,
                    D3D11_SDK_VERSION,
                    Some(device),
                    None,
                    None,
                )
            };
            let prefer_hardware =
                std::env::var("MOON_GRAPHICS_DRIVER").as_deref() == Ok("hardware");
            let backend =
                if prefer_hardware && create(D3D_DRIVER_TYPE_HARDWARE, &mut device).is_ok() {
                    "hardware"
                } else {
                    create(D3D_DRIVER_TYPE_WARP, &mut device)?;
                    "warp"
                };
            let d3d = device.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
            let dxgi_device: IDXGIDevice = d3d.cast()?;
            let factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d = factory.CreateDevice(&dxgi_device)?;
            // Bound reusable image/texture caches; live render targets remain owned by DComp.
            d2d.SetMaximumTextureMemory(0);
            let context = d2d.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            // Every scene uses physical pixels; all draws share this main-thread context.
            context.SetDpi(96.0, 96.0);
            context.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            // Surface allocations are pooled per factory. Keep the wide, shallow
            // bar separate from the taller border atlas, sharing the same DXGI
            // device and composition transaction. No implicit third pool is needed.
            // https://learn.microsoft.com/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice2-createsurfacefactory
            let composition: IDCompositionDevice =
                DCompositionCreateDevice2(None::<&windows::core::IUnknown>)?;
            let composition2: IDCompositionDevice2 = composition.cast()?;
            let bar_surfaces = composition2.CreateSurfaceFactory(&dxgi_device)?;
            let border_surfaces = composition2.CreateSurfaceFactory(&dxgi_device)?;
            let write: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let loader = write.CreateInMemoryFontFileLoader()?;
            write.RegisterFontFileLoader(&loader)?;
            // A non-null owner avoids DirectWrite copying the embedded font. The
            // static bytes outlive this device reference and every font collection.
            let font = loader.CreateInMemoryFontFileReference(
                &write,
                FONT.as_ptr().cast(),
                FONT.len() as u32,
                Some(&d3d.cast::<windows::core::IUnknown>()?),
            )?;
            let factory3: IDWriteFactory3 = write.cast()?;
            let reference =
                factory3.CreateFontFaceReference(&font, 0, DWRITE_FONT_SIMULATIONS_NONE)?;
            let builder = factory3.CreateFontSetBuilder()?;
            builder.AddFontFaceReference2(&reference)?;
            let symbols =
                factory3.CreateFontCollectionFromFontSet(&builder.CreateFontSet()?)?.cast()?;
            let mut counters = diagnostics.borrow_mut();
            counters.devices_created = counters.devices_created.saturating_add(1);
            counters.surface_factories_created =
                counters.surface_factories_created.saturating_add(2);
            drop(counters);
            Ok(Self {
                backend,
                context,
                composition,
                bar_surfaces,
                border_surfaces,
                write,
                symbols,
                loader,
                diagnostics,
            })
        }
    }
}
impl Drop for Graphics {
    fn drop(&mut self) {
        unsafe {
            let _ = self.write.UnregisterFontFileLoader(&self.loader);
        }
    }
}

/// Current Windows accessibility choices. Query on startup and WM_SETTINGCHANGE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessibilityPreferences {
    pub motion_allowed: bool,
    pub high_contrast: bool,
    pub transparency_allowed: bool,
}
pub fn system_accessibility() -> AccessibilityPreferences {
    unsafe {
        let mut animations = BOOL(0);
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut animations as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let mut contrast =
            HIGHCONTRASTW { cbSize: size_of::<HIGHCONTRASTW>() as u32, ..Default::default() };
        let _ = SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&mut contrast as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let high_contrast = contrast.dwFlags.0 & HCF_HIGHCONTRASTON.0 != 0;
        let transparency_allowed = !high_contrast
            && windows::UI::ViewManagement::UISettings::new()
                .and_then(|settings| settings.AdvancedEffectsEnabled())
                .unwrap_or(false);
        AccessibilityPreferences {
            motion_allowed: animations.as_bool() && !high_contrast,
            high_contrast,
            transparency_allowed,
        }
    }
}

#[derive(Clone, PartialEq)]
struct Preferences {
    animations: bool,
    contrast: bool,
    transparency: bool,
    window: String,
    text: String,
    highlight: String,
    highlight_text: String,
    button: String,
    button_text: String,
}
impl Preferences {
    fn read() -> Self {
        unsafe {
            let accessibility = system_accessibility();
            let color = |index| format!("#{:06x}", rgb_to_colorref(GetSysColor(index)));
            Self {
                animations: accessibility.motion_allowed,
                contrast: accessibility.high_contrast,
                transparency: accessibility.transparency_allowed,
                window: color(COLOR_WINDOW),
                text: color(COLOR_WINDOWTEXT),
                highlight: color(COLOR_HIGHLIGHT),
                highlight_text: color(COLOR_HIGHLIGHTTEXT),
                button: color(COLOR_BTNFACE),
                button_text: color(COLOR_BTNTEXT),
            }
        }
    }
    fn apply(&self, scene: &mut Scene) {
        if !self.transparency {
            for bar in &mut scene.bars {
                bar.opacity = 1.0;
                bar.backdrop = "none".into();
            }
        }
        if !self.contrast {
            return;
        }
        for bar in &mut scene.bars {
            bar.background = self.window.clone();
            bar.opacity = 1.0;
            bar.backdrop = "none".into();
            for item in &mut bar.items {
                let (foreground, background) = if item.selected {
                    (&self.highlight_text, &self.highlight)
                } else if item.command.is_some() {
                    (&self.button_text, &self.button)
                } else {
                    (&self.text, &self.window)
                };
                item.foreground = foreground.clone();
                item.background = Some(background.clone());
            }
        }
        for border in &mut scene.borders {
            let color = if border.focused { &self.highlight } else { &self.text };
            border.colors = [color.clone(), color.clone()];
        }
    }
}

struct BarTarget {
    rect: Rect,
    label: String,
    command: Option<String>,
}
fn bar_targets(bar: &BarScene) -> Vec<BarTarget> {
    let mut targets: Vec<BarTarget> = Vec::new();
    for item in bar.items.iter().filter(|item| !item.rect.is_empty()) {
        let rect = item.rect.translated(-bar.rect.x, -bar.rect.y);
        // Icon and text segments form one visible control. Sharing an action
        // alone is insufficient: separate labeled controls remain separate.
        if let Some(previous) = targets.last_mut()
            && !item.label.is_empty()
            && previous.label == item.label
            && previous.command == item.command
            && previous.rect.y == rect.y
            && previous.rect.height == rect.height
            && (previous.rect.right() - rect.x).abs() <= 1
        {
            previous.rect.width = rect.right() - previous.rect.x;
        } else {
            targets.push(BarTarget {
                rect,
                label: item.label.clone(),
                command: item.command.clone(),
            });
        }
    }
    targets
}

#[derive(Default)]
struct WindowData {
    hit_regions: Vec<(Rect, String)>,
    commands: Arc<Mutex<Vec<String>>>,
    click_through: bool,
    appbar: bool,
    appbar_dirty: bool,
    appbar_registered: bool,
    taskbar_created: u32,
    tray: bool,
    hovered: Option<Rect>,
    pressed: Option<(Rect, String)>,
    interaction_dirty: bool,
    preferences_dirty: bool,
    paused: bool,
    game_active: bool,
    game_identity: Option<(String, moon_core::gaming::GameSource)>,
    tray_icon: HICON,
    tray_tip: String,
}

impl WindowData {
    fn enqueue(&self, command: String) {
        if let Ok(mut commands) = self.commands.lock() {
            commands.push(command);
        }
    }

    fn press(&mut self, point: Point) {
        self.pressed = self.hit_regions.iter().find(|(rect, _)| rect.contains(point)).cloned();
    }
    fn release(&mut self, point: Point) -> Option<String> {
        let (rect, command) = self.pressed.take()?;
        (rect.contains(point)
            && self.hit_regions.iter().any(|hit| hit.0 == rect && hit.1 == command))
        .then_some(command)
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<WindowData>;
        if !pointer.is_null() {
            let state = &*pointer;
            // USER32 and Explorer can synchronously reenter this procedure.
            // Copy message data out before calling them so nested callbacks can borrow it.
            let (is_appbar, is_tray) = {
                let data = state.borrow();
                (data.appbar, data.tray)
            };
            let tray_after_restart = {
                let mut data = state.borrow_mut();
                if message == data.taskbar_created {
                    if is_appbar {
                        data.appbar_dirty = true;
                        data.appbar_registered = false;
                    }
                    if is_tray {
                        data.preferences_dirty = true;
                        Some(tray_info(hwnd, &data))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(icon) = tray_after_restart {
                add_tray(icon);
            }
            if message == TRAY_MESSAGE && is_tray {
                let notification = lparam.0 as u32 & 0xffff;
                if notification == NIN_SELECT || notification == (NIN_SELECT | NINF_KEY) {
                    state.borrow().enqueue("settings".into());
                }
                if notification == WM_CONTEXTMENU || notification == WM_RBUTTONUP {
                    let menu = CreatePopupMenu();
                    if let Ok(menu) = menu {
                        let paused = state.borrow().paused;
                        let game = state.borrow().game_active;
                        for (id, label) in [
                            (6, "Settings…"),
                            (1, if paused { "Resume tiling" } else { "Pause tiling" }),
                            (
                                7,
                                if game {
                                    "End game mode"
                                } else {
                                    "Use game mode for the last app"
                                },
                            ),
                            (2, "Reload configuration"),
                            (3, "Open configuration"),
                            (4, "Keybindings"),
                            (5, "Quit moon"),
                        ] {
                            let label = wide(label);
                            let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(label.as_ptr()));
                        }
                        let mut point = windows::Win32::Foundation::POINT::default();
                        if let Ok(rect) = Shell_NotifyIconGetRect(&NOTIFYICONIDENTIFIER {
                            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
                            hWnd: hwnd,
                            uID: 1,
                            ..Default::default()
                        }) {
                            point.x = rect.left;
                            point.y = rect.bottom;
                        } else {
                            let _ = GetCursorPos(&mut point);
                        }
                        let _ = SetForegroundWindow(hwnd);
                        let selected = TrackPopupMenu(
                            menu,
                            TPM_RETURNCMD | TPM_RIGHTBUTTON,
                            point.x,
                            point.y,
                            Some(0),
                            hwnd,
                            None,
                        )
                        .0;
                        let command = match selected {
                            1 => Some("pause"),
                            2 => Some("reload"),
                            3 => Some("open-config"),
                            4 => Some("cheatsheet"),
                            5 => Some("quit"),
                            6 => Some("settings"),
                            7 => Some(if game { "game-mode end" } else { "game-mode start" }),
                            _ => None,
                        };
                        if let Some(command) = command {
                            state.borrow().enqueue(command.into());
                        }
                        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                        let _ = DestroyMenu(menu);
                        if selected == 0 {
                            let icon = tray_info(hwnd, &state.borrow());
                            let _ = Shell_NotifyIconW(NIM_SETFOCUS, &icon);
                        }
                    }
                }
                return LRESULT(0);
            }
            if message == APPBAR_MESSAGE && is_appbar {
                if wparam.0 == ABN_POSCHANGED as usize {
                    state.borrow_mut().appbar_dirty = true;
                }
                if wparam.0 == ABN_FULLSCREENAPP as usize {
                    let _ = SetWindowPos(
                        hwnd,
                        Some(if lparam.0 != 0 { HWND_BOTTOM } else { HWND_TOPMOST }),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
                return LRESULT(0);
            }
            match message {
                WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE | WM_DPICHANGED => {
                    let mut data = state.borrow_mut();
                    data.preferences_dirty = true;
                    data.interaction_dirty = true;
                }
                WM_MOUSEMOVE if is_appbar => {
                    let point = mouse_point(lparam);
                    {
                        let mut data = state.borrow_mut();
                        let hovered = data
                            .hit_regions
                            .iter()
                            .find(|(rect, _)| rect.contains(point))
                            .map(|(rect, _)| *rect);
                        if hovered != data.hovered {
                            data.hovered = hovered;
                            data.interaction_dirty = true;
                        }
                    }
                    let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        ..Default::default()
                    });
                }
                WM_MOUSELEAVE if is_appbar => {
                    let mut data = state.borrow_mut();
                    data.hovered = None;
                    data.interaction_dirty = true;
                }
                WM_SETCURSOR if is_appbar => {
                    let hovered = state.borrow().hovered.is_some();
                    if hovered {
                        SetCursor(LoadCursorW(None, IDC_HAND).ok());
                        return LRESULT(1);
                    }
                }
                WM_LBUTTONDOWN if is_appbar => {
                    let point = mouse_point(lparam);
                    let capture = {
                        let mut data = state.borrow_mut();
                        data.press(point);
                        data.interaction_dirty = true;
                        data.pressed.is_some()
                    };
                    if capture {
                        SetCapture(hwnd);
                    }
                    return LRESULT(0);
                }
                WM_CAPTURECHANGED if is_appbar => {
                    let mut data = state.borrow_mut();
                    data.pressed = None;
                    data.interaction_dirty = true;
                }
                WM_NCHITTEST if state.borrow().click_through => {
                    return LRESULT(HTTRANSPARENT as isize);
                }
                WM_ACTIVATE | WM_WINDOWPOSCHANGED if is_appbar => {
                    let registered = state.borrow().appbar_registered;
                    if registered {
                        let mut appbar = APPBARDATA {
                            cbSize: size_of::<APPBARDATA>() as u32,
                            hWnd: hwnd,
                            ..Default::default()
                        };
                        let notification = if message == WM_ACTIVATE {
                            ABM_ACTIVATE
                        } else {
                            ABM_WINDOWPOSCHANGED
                        };
                        SHAppBarMessage(notification, &mut appbar);
                    }
                }
                WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
                WM_ERASEBKGND => return LRESULT(1),
                WM_PAINT => {
                    let _ = ValidateRect(Some(hwnd), None);
                    return LRESULT(0);
                }
                WM_LBUTTONUP if is_appbar => {
                    let point = mouse_point(lparam);
                    {
                        let mut data = state.borrow_mut();
                        if let Some(command) = data.release(point) {
                            data.enqueue(command);
                        }
                        data.interaction_dirty = true;
                    }
                    let _ = ReleaseCapture();
                    return LRESULT(0);
                }
                WM_CLOSE => return LRESULT(0),
                WM_NCDESTROY => {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

fn mouse_point(lparam: LPARAM) -> Point {
    Point::new((lparam.0 as u16) as i16 as i32, ((lparam.0 >> 16) as u16) as i16 as i32)
}

struct Motion {
    from: f32,
    to: f32,
    started: Instant,
    duration: f32,
}
impl Motion {
    fn value(&self) -> f32 {
        let t = (self.started.elapsed().as_secs_f32() / self.duration).min(1.0);
        self.from + (self.to - self.from) * (1.0 - (1.0 - t).powi(3))
    }
}
fn transition(
    graphics: &Graphics,
    from: f32,
    to: f32,
    seconds: f32,
) -> Result<IDCompositionAnimation> {
    unsafe {
        let animation = graphics.composition.CreateAnimation()?;
        let delta = to - from;
        animation.AddCubic(
            0.0,
            from,
            3.0 * delta / seconds,
            -3.0 * delta / (seconds * seconds),
            delta / (seconds * seconds * seconds),
        )?;
        animation.End(f64::from(seconds), to)?;
        Ok(animation)
    }
}
struct Feedback {
    surface: IDCompositionSurface,
    visual: IDCompositionVisual,
    effect: IDCompositionEffectGroup,
    clip: IDCompositionRectangleClip,
    motion: Motion,
    color: String,
    rect: Option<Rect>,
}
impl Feedback {
    fn new(graphics: &Graphics, parent: &IDCompositionVisual) -> Result<Self> {
        unsafe {
            let surface = graphics.bar_surfaces.CreateSurface(
                1,
                1,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_PREMULTIPLIED,
            )?;
            let visual = graphics.composition.CreateVisual()?;
            let effect = graphics.composition.CreateEffectGroup()?;
            effect.SetOpacity2(0.0)?;
            visual.SetEffect(&effect)?;
            visual.SetContent(&surface)?;
            let clip = graphics.composition.CreateRectangleClip()?;
            clip.SetLeft2(0.0)?;
            clip.SetTop2(0.0)?;
            clip.SetRight2(1.0)?;
            clip.SetBottom2(1.0)?;
            visual.SetClip(&clip)?;
            parent.AddVisual(&visual, true, None)?;
            Ok(Self {
                surface,
                visual,
                effect,
                clip,
                motion: Motion { from: 0.0, to: 0.0, started: Instant::now(), duration: 0.083 },
                color: String::new(),
                rect: None,
            })
        }
    }
    fn update(
        &mut self,
        graphics: &Graphics,
        rect: Option<Rect>,
        pressed: bool,
        background: &str,
        preferences: &Preferences,
    ) -> Result<()> {
        unsafe {
            let rgb = parse_color(background, 1.0);
            let color = if rgb.r + rgb.g + rgb.b > 1.5 { "#000000" } else { "#ffffff" };
            if self.color != color {
                let drawing = Drawing::begin(
                    graphics,
                    &self.surface,
                    Rect::new(0, 0, 1, 1),
                    DrawKind::Feedback,
                )?;
                graphics.context.Clear(Some(&parse_color(color, 1.0)));
                drawing.finish()?;
                self.color = color.into();
            }
            if let Some(bounds) = rect.filter(|bounds| !bounds.is_empty()) {
                let radius = bounds.height as f32 * (5.0 / 28.0);
                let rx = radius / bounds.width as f32;
                let ry = radius / bounds.height as f32;
                self.clip.SetTopLeftRadiusX2(rx)?;
                self.clip.SetTopRightRadiusX2(rx)?;
                self.clip.SetBottomLeftRadiusX2(rx)?;
                self.clip.SetBottomRightRadiusX2(rx)?;
                self.clip.SetTopLeftRadiusY2(ry)?;
                self.clip.SetTopRightRadiusY2(ry)?;
                self.clip.SetBottomLeftRadiusY2(ry)?;
                self.clip.SetBottomRightRadiusY2(ry)?;
                self.visual.SetTransform2(&Matrix3x2 {
                    M11: bounds.width as f32,
                    M12: 0.0,
                    M21: 0.0,
                    M22: bounds.height as f32,
                    M31: bounds.x as f32,
                    M32: bounds.y as f32,
                })?;
            }
            let opacity = if rect.is_none() || preferences.contrast {
                0.0
            } else if pressed {
                0.17
            } else {
                0.085
            };
            if opacity != self.motion.to || rect != self.rect || !preferences.animations {
                let from =
                    if rect != self.rect && rect.is_some() { 0.0 } else { self.motion.value() };
                if preferences.animations {
                    self.effect.SetOpacity(&transition(graphics, from, opacity, 0.083)?)?;
                } else {
                    self.effect.SetOpacity2(opacity)?;
                }
                self.motion =
                    Motion { from, to: opacity, started: Instant::now(), duration: 0.083 };
                self.rect = rect;
            }
            Ok(())
        }
    }
}

struct Tooltips {
    hwnd: HWND,
    parent: HWND,
    tools: Vec<(Rect, Vec<u16>)>,
}
impl Tooltips {
    fn new(parent: HWND) -> Result<Self> {
        unsafe {
            let controls = INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES,
            };
            InitCommonControlsEx(&controls).ok()?;
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                TOOLTIPS_CLASSW,
                w!(""),
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                0,
                0,
                0,
                0,
                Some(parent),
                None,
                Some(GetModuleHandleW(None)?.into()),
                None,
            )?;
            SendMessageW(hwnd, TTM_SETMAXTIPWIDTH, Some(WPARAM(0)), Some(LPARAM(480)));
            Ok(Self { hwnd, parent, tools: Vec::new() })
        }
    }
    fn info(&mut self, index: usize) -> TTTOOLINFOW {
        let (rect, text) = &mut self.tools[index];
        TTTOOLINFOW {
            cbSize: size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_SUBCLASS,
            hwnd: self.parent,
            uId: index + 1,
            rect: windows::Win32::Foundation::RECT {
                left: rect.x,
                top: rect.y,
                right: rect.right(),
                bottom: rect.bottom(),
            },
            lpszText: PWSTR(text.as_mut_ptr()),
            ..Default::default()
        }
    }
    fn update(&mut self, targets: &[BarTarget]) {
        let next = targets
            .iter()
            .filter(|target| !target.label.is_empty())
            .map(|target| (target.rect, wide(&target.label)))
            .collect::<Vec<_>>();
        unsafe {
            if self.tools.len() != next.len() {
                for index in 0..self.tools.len() {
                    let info = self.info(index);
                    SendMessageW(
                        self.hwnd,
                        TTM_DELTOOLW,
                        Some(WPARAM(0)),
                        Some(LPARAM((&info as *const TTTOOLINFOW) as isize)),
                    );
                }
                self.tools = next;
                for index in 0..self.tools.len() {
                    let info = self.info(index);
                    SendMessageW(
                        self.hwnd,
                        TTM_ADDTOOLW,
                        Some(WPARAM(0)),
                        Some(LPARAM((&info as *const TTTOOLINFOW) as isize)),
                    );
                }
            } else {
                for (index, next) in next.into_iter().enumerate() {
                    if self.tools[index] != next {
                        let old = std::mem::replace(&mut self.tools[index], next);
                        let info = self.info(index);
                        SendMessageW(
                            self.hwnd,
                            TTM_SETTOOLINFOW,
                            Some(WPARAM(0)),
                            Some(LPARAM((&info as *const TTTOOLINFOW) as isize)),
                        );
                        drop(old);
                    }
                }
            }
        }
    }
}
impl Drop for Tooltips {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

struct TextFormat {
    font: String,
    size: f32,
    format: IDWriteTextFormat,
}

struct TextLayout {
    text: String,
    font: String,
    size: f32,
    width: i32,
    height: i32,
    layout: IDWriteTextLayout,
}

#[derive(Default)]
struct TextLayouts(Vec<Option<TextLayout>>);

impl TextLayouts {
    /// Retain only current visible item slots, including when no drawing is needed.
    fn prepare(&mut self, count: usize, visible: bool) {
        let count = if visible { count } else { 0 };
        let shrinking = count < self.0.len();
        self.0.resize_with(count, || None);
        if shrinking {
            // Release the slot storage as well as the removed COM/string values.
            self.0.shrink_to_fit();
        }
    }
}

fn text_layout(
    graphics: &Graphics,
    format: &mut Option<TextFormat>,
    cached: &mut Option<TextLayout>,
    text: &str,
    bounds: Rect,
    font: &str,
    size: f32,
) -> Result<IDWriteTextLayout> {
    let width = bounds.width.max(1);
    let height = bounds.height.max(1);
    if cached.as_ref().is_none_or(|cached| {
        cached.text != text
            || cached.font != font
            || cached.size != size
            || cached.width != width
            || cached.height != height
    }) {
        let format = text_format(graphics, format, font, size)?;
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let layout = unsafe {
            graphics.write.CreateTextLayout(&utf16, &format, width as f32, height as f32)?
        };
        {
            let mut counters = graphics.diagnostics.borrow_mut();
            counters.text_layouts_created = counters.text_layouts_created.saturating_add(1);
        }
        let mut offset = 0u32;
        for character in text.chars() {
            let len = character.len_utf16() as u32;
            if ('\u{e000}'..='\u{f8ff}').contains(&character)
                || ('\u{f0000}'..='\u{10ffff}').contains(&character)
            {
                let range = DWRITE_TEXT_RANGE { startPosition: offset, length: len };
                unsafe {
                    layout.SetFontCollection(&graphics.symbols, range)?;
                    layout.SetFontFamilyName(w!("Symbols Nerd Font"), range)?;
                }
            }
            offset += len;
        }
        // Never publish a partially configured layout after a fallible font override.
        *cached =
            Some(TextLayout { text: text.into(), font: font.into(), size, width, height, layout });
    }
    Ok(cached.as_ref().expect("created text layout").layout.clone())
}

fn text_format(
    graphics: &Graphics,
    cached: &mut Option<TextFormat>,
    font: &str,
    size: f32,
) -> Result<IDWriteTextFormat> {
    if cached.as_ref().is_none_or(|cached| cached.font != font || cached.size != size) {
        unsafe {
            let family = wide(font);
            let format = graphics.write.CreateTextFormat(
                PCWSTR(family.as_ptr()),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let ellipsis = graphics.write.CreateEllipsisTrimmingSign(&format)?;
            format.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    ..Default::default()
                },
                &ellipsis,
            )?;
            *cached = Some(TextFormat { font: font.into(), size, format });
        }
        let mut counters = graphics.diagnostics.borrow_mut();
        counters.text_formats_created = counters.text_formats_created.saturating_add(1);
    }
    Ok(cached.as_ref().expect("created text format").format.clone())
}

fn draw_bar_text(
    context: &ID2D1DeviceContext,
    layout: &IDWriteTextLayout,
    bounds: Rect,
    brush: &ID2D1SolidColorBrush,
    center_content: bool,
) -> Result<()> {
    let mut origin = Vector2 { X: bounds.x as f32, Y: bounds.y as f32 };
    unsafe {
        if center_content {
            // Signed overhangs include the shaped glyphs' side bearings, ascenders
            // and descenders, including those from the bundled symbol font.
            let ink = layout.GetOverhangMetrics()?;
            origin.X += (ink.left - ink.right) / 2.0;
            origin.Y += (ink.top - ink.bottom) / 2.0;
            // Clip to the button's content box, independent of the shifted layout.
            context.PushAxisAlignedClip(&d2_rect(bounds), D2D1_ANTIALIAS_MODE_ALIASED);
            let _clip = Clip(context);
            context.DrawTextLayout(origin, layout, brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
        } else {
            context.DrawTextLayout(origin, layout, brush, D2D1_DRAW_TEXT_OPTIONS_CLIP);
        }
    }
    Ok(())
}

struct BorderBrush {
    colors: [String; 2],
    brush: ID2D1LinearGradientBrush,
}

fn border_brush(
    graphics: &Graphics,
    cached: &mut Vec<BorderBrush>,
    index: usize,
    colors: &[String; 2],
) -> Result<ID2D1LinearGradientBrush> {
    if cached.get(index).is_none_or(|cached| cached.colors != *colors) {
        unsafe {
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: parse_color(&colors[0], 1.0) },
                D2D1_GRADIENT_STOP { position: 1.0, color: parse_color(&colors[1], 1.0) },
            ];
            let target: ID2D1RenderTarget = graphics.context.cast()?;
            let collection = target.CreateGradientStopCollection(
                &stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            )?;
            let brush = target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES::default(),
                None,
                &collection,
            )?;
            let cached_brush = BorderBrush { colors: colors.clone(), brush };
            if index == cached.len() {
                cached.push(cached_brush);
            } else {
                cached[index] = cached_brush;
            }
        }
        let mut counters = graphics.diagnostics.borrow_mut();
        counters.gradient_brushes_created = counters.gradient_brushes_created.saturating_add(1);
    }
    Ok(cached[index].brush.clone())
}

struct Canvas {
    hwnd: HWND,
    data: Rc<RefCell<WindowData>>,
    requested_rect: Rect,
    rect: Rect,
    context: ID2D1DeviceContext,
    surface: Option<IDCompositionSurface>,
    strips: Vec<IDCompositionVisual>,
    border_atlas: Option<(Rect, IDCompositionSurface)>,
    // One format per canvas, one layout per visible item and one brush per ring.
    // Entries are replaced in place; no text or widget history is retained.
    text_format: Option<TextFormat>,
    text_layouts: TextLayouts,
    border_brushes: Vec<BorderBrush>,
    _target: IDCompositionTarget,
    visual: IDCompositionVisual,
    visible: bool,
    reservation: Option<(windows::Win32::Foundation::RECT, windows::Win32::Foundation::RECT)>,
    feedback: Option<Feedback>,
    entrance: Option<IDCompositionEffectGroup>,
    tooltip: Option<Tooltips>,
}

struct AtlasPiece {
    target: Rect,
    source: Rect,
    transposed: bool,
}

// Short horizontal slices pack thin rings without allocating their empty interiors.
fn pack_border_atlas(regions: impl IntoIterator<Item = Rect>) -> (Vec<AtlasPiece>, Rect) {
    const WIDTH: i32 = 1024;
    let mut slices = Vec::new();
    for region in regions {
        let transposed = region.height > region.width;
        let (length, thickness) =
            if transposed { (region.height, region.width) } else { (region.width, region.height) };
        for offset in (0..length).step_by(WIDTH as usize) {
            let width = WIDTH.min(length - offset);
            let target = if transposed {
                Rect::new(region.x, region.y + offset, thickness, width)
            } else {
                Rect::new(region.x + offset, region.y, width, thickness)
            };
            slices.push((target, transposed, width, thickness));
        }
    }
    // Fill full rows before the short tails, keeping capacity stable during resize.
    slices.sort_unstable_by_key(|(_, _, width, thickness)| std::cmp::Reverse((*width, *thickness)));
    let (mut x, mut y, mut row_height) = (0, 0, 0);
    let mut pieces = Vec::with_capacity(slices.len());
    for (target, transposed, width, thickness) in slices {
        if x + width > WIDTH {
            x = 0;
            y += row_height;
            row_height = 0;
        }
        pieces.push(AtlasPiece { target, source: Rect::new(x, y, width, thickness), transposed });
        x += width;
        row_height = row_height.max(thickness);
    }
    let height = (y + row_height).max(1) as u32;
    (pieces, Rect::new(0, 0, WIDTH, height.next_power_of_two() as i32))
}

struct Clip<'a>(&'a ID2D1DeviceContext);
impl Drop for Clip<'_> {
    fn drop(&mut self) {
        unsafe {
            self.0.PopAxisAlignedClip();
        }
    }
}

impl Canvas {
    fn new(
        graphics: &Graphics,
        rect: Rect,
        border: bool,
        commands: Arc<Mutex<Vec<String>>>,
    ) -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("moon.surface"),
                hbrBackground: HBRUSH::default(),
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            };
            // RegisterClassW returns zero if an earlier surface registered this class.
            RegisterClassW(&class);
            let data = Rc::new(RefCell::new(WindowData {
                hit_regions: Vec::new(),
                commands,
                click_through: border,
                appbar: !border,
                appbar_dirty: true,
                appbar_registered: false,
                taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
                tray: false,
                preferences_dirty: true,
                ..Default::default()
            }));
            let ex_style = WS_EX_NOACTIVATE
                | WS_EX_TOOLWINDOW
                | WS_EX_NOREDIRECTIONBITMAP
                | if border { WS_EX_TRANSPARENT | WS_EX_LAYERED } else { WS_EX_TOPMOST };
            let hwnd = CreateWindowExW(
                ex_style,
                w!("moon.surface"),
                w!("moon"),
                WS_POPUP,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                None,
                None,
                Some(instance.into()),
                Some(Rc::as_ptr(&data).cast()),
            )?;
            if !border {
                // Explorer must still deliver callbacks when moon has UIAccess.
                let taskbar_created = data.borrow().taskbar_created;
                let _ = ChangeWindowMessageFilterEx(hwnd, taskbar_created, MSGFLT_ALLOW, None);
                let _ = ChangeWindowMessageFilterEx(hwnd, APPBAR_MESSAGE, MSGFLT_ALLOW, None);
            }
            if border
                && let Err(error) =
                    SetLayeredWindowAttributes(hwnd, Default::default(), 255, LWA_ALPHA)
            {
                let _ = DestroyWindow(hwnd);
                return Err(error);
            }
            if border {
                // The last ring disappears by destroying this monitor's underlay.
                // Keep DWM from animating a snapshot of the retired border.
                let disabled = BOOL::from(true);
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TRANSITIONS_FORCEDISABLED,
                    (&disabled as *const BOOL).cast(),
                    size_of::<BOOL>() as u32,
                );
                let corner = DWMWCP_DONOTROUND;
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    (&corner as *const DWM_WINDOW_CORNER_PREFERENCE).cast(),
                    size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
                );
            }
            let result = (|| -> Result<_> {
                let context = graphics.context.clone();
                let surface = if border {
                    None
                } else {
                    Some(graphics.bar_surfaces.CreateSurface(
                        rect.width.max(1) as u32,
                        rect.height.max(1) as u32,
                        DXGI_FORMAT_B8G8R8A8_UNORM,
                        DXGI_ALPHA_MODE_PREMULTIPLIED,
                    )?)
                };
                let target = graphics.composition.CreateTargetForHwnd(hwnd, true)?;
                let visual = graphics.composition.CreateVisual()?;
                if let Some(surface) = &surface {
                    visual.SetContent(surface)?;
                }
                target.SetRoot(&visual)?;
                let feedback = if border { None } else { Some(Feedback::new(graphics, &visual)?) };
                let entrance = if border {
                    None
                } else {
                    let effect = graphics.composition.CreateEffectGroup()?;
                    visual.SetEffect(&effect)?;
                    Some(effect)
                };
                let tooltip = if border { None } else { Some(Tooltips::new(hwnd)?) };
                Ok((context, surface, target, visual, feedback, entrance, tooltip))
            })();
            let (context, surface, target, visual, feedback, entrance, tooltip) = match result {
                Ok(value) => value,
                Err(error) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(error);
                }
            };
            let mut counters = graphics.diagnostics.borrow_mut();
            let count = if border {
                &mut counters.border_canvases_created
            } else {
                &mut counters.bar_canvases_created
            };
            *count = count.saturating_add(1);
            drop(counters);
            Ok(Self {
                hwnd,
                data,
                requested_rect: rect,
                rect,
                context,
                surface,
                strips: Vec::new(),
                border_atlas: None,
                text_format: None,
                text_layouts: TextLayouts::default(),
                border_brushes: Vec::new(),
                _target: target,
                visual,
                visible: false,
                reservation: None,
                feedback,
                entrance,
                tooltip,
            })
        }
    }

    fn position(&mut self, requested: Rect, visible: bool) -> Result<()> {
        unsafe {
            let (is_appbar, click_through, appbar_dirty) = {
                let data = self.data.borrow();
                (data.appbar, data.click_through, data.appbar_dirty)
            };
            let rect = if is_appbar {
                if requested != self.requested_rect || self.visible != visible || appbar_dirty {
                    self.appbar_position(requested, visible)
                } else {
                    self.rect
                }
            } else {
                requested
            };
            let resized = self.rect.width != rect.width || self.rect.height != rect.height;
            if is_appbar && resized {
                self.surface = None;
            }
            if self.rect != rect || self.visible != visible {
                SetWindowPos(
                    self.hwnd,
                    Some(if click_through { HWND_BOTTOM } else { HWND_TOPMOST }),
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    SWP_NOACTIVATE | if visible { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW },
                )?;
            }
            self.requested_rect = requested;
            self.rect = rect;
            self.visible = visible;
            Ok(())
        }
    }

    fn appbar_position(&mut self, requested: Rect, visible: bool) -> Rect {
        let registered = {
            let mut data = self.data.borrow_mut();
            data.appbar_dirty = false;
            data.appbar_registered
        };
        let mut appbar = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: self.hwnd,
            uCallbackMessage: APPBAR_MESSAGE,
            uEdge: ABE_TOP,
            rc: windows::Win32::Foundation::RECT {
                left: requested.x,
                top: requested.y,
                right: requested.right(),
                bottom: requested.y + if visible { requested.height } else { 0 },
            },
            ..Default::default()
        };
        unsafe {
            if !registered {
                self.reservation = None;
                if !visible {
                    return requested;
                }
                if SHAppBarMessage(ABM_NEW, &mut appbar) == 0 {
                    self.data.borrow_mut().appbar_dirty = true;
                    return requested;
                }
                self.data.borrow_mut().appbar_registered = true;
            }
            if SHAppBarMessage(ABM_QUERYPOS, &mut appbar) == 0 {
                self.data.borrow_mut().appbar_dirty = true;
                return self.rect;
            }
            appbar.rc.bottom = appbar.rc.top + if visible { requested.height } else { 0 };
            let proposed = appbar.rc;
            if let Some((previous, accepted)) = self.reservation
                && previous == proposed
            {
                appbar.rc = accepted;
            } else {
                if SHAppBarMessage(ABM_SETPOS, &mut appbar) == 0 {
                    self.data.borrow_mut().appbar_dirty = true;
                    return self.rect;
                }
                self.reservation = Some((proposed, appbar.rc));
            }
        }
        let rect = appbar.rc;
        Rect::new(
            rect.left,
            rect.top,
            (rect.right - rect.left).max(1),
            if visible { (rect.bottom - rect.top).max(1) } else { requested.height },
        )
    }

    fn text(
        &mut self,
        graphics: &Graphics,
        index: usize,
        item: &BarItem,
        bounds: Rect,
        font: &str,
        color: &str,
    ) -> Result<()> {
        let layout = text_layout(
            graphics,
            &mut self.text_format,
            &mut self.text_layouts.0[index],
            &item.text,
            bounds,
            font,
            item.font_size,
        )?;
        unsafe {
            let brush = self.context.CreateSolidColorBrush(&parse_color(color, 1.0), None)?;
            draw_bar_text(&self.context, &layout, bounds, &brush, item.center_content)
        }
    }

    fn draw_bar(
        &mut self,
        graphics: &Graphics,
        bar: &BarScene,
        preferences: &Preferences,
    ) -> Result<()> {
        unsafe {
            if self.surface.is_none() {
                let surface = graphics.bar_surfaces.CreateSurface(
                    self.rect.width.max(1) as u32,
                    self.rect.height.max(1) as u32,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_ALPHA_MODE_PREMULTIPLIED,
                )?;
                self.visual.SetContent(&surface)?;
                self.surface = Some(surface);
            }
            let drawing = Drawing::begin(
                graphics,
                self.surface.as_ref().expect("bar surface"),
                Rect::new(0, 0, self.rect.width, self.rect.height),
                DrawKind::Bar,
            )?;
            self.context.Clear(Some(&parse_color(&bar.background, bar.opacity)));
            let targets = bar_targets(bar);
            let hovered = {
                let mut data = self.data.borrow_mut();
                data.hit_regions = targets
                    .iter()
                    .filter_map(|target| {
                        target.command.as_ref().map(|command| (target.rect, command.clone()))
                    })
                    .collect();
                data.hovered
            };
            let scale = GetDpiForWindow(self.hwnd).max(96) as f32 / 96.0;
            for (index, item) in bar.items.iter().enumerate() {
                let bounds = item.rect.translated(-bar.rect.x, -bar.rect.y);
                if bounds.is_empty() {
                    self.text_layouts.0[index] = None;
                    continue;
                }
                let highlighted = preferences.contrast
                    && item.command.is_some()
                    && hovered.is_some_and(|hover| hover.intersects(&bounds));
                let background = if highlighted {
                    Some(&preferences.highlight)
                } else {
                    item.background.as_ref()
                };
                if let Some(background) = background {
                    let brush =
                        self.context.CreateSolidColorBrush(&parse_color(background, 1.0), None)?;
                    let radius = bounds.height as f32 * (5.0 / 28.0);
                    self.context.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: d2_rect(bounds),
                            radiusX: radius,
                            radiusY: radius,
                        },
                        &brush,
                    );
                }
                let inset = ((7.0 * scale).round() as i32).min((bounds.width - 1) / 2);
                self.text(
                    graphics,
                    index,
                    item,
                    bounds.inset(moon_core::geometry::Edges {
                        left: inset,
                        right: inset,
                        top: 0,
                        bottom: 0,
                    }),
                    &bar.font,
                    if highlighted { &preferences.highlight_text } else { &item.foreground },
                )?;
                if item.selected {
                    let brush = self
                        .context
                        .CreateSolidColorBrush(&parse_color(&item.foreground, 1.0), None)?;
                    let width = 10.0 * scale;
                    let left = bounds.x as f32 + (bounds.width as f32 - width) / 2.0;
                    let bottom = bounds.bottom() as f32 - 2.0 * scale;
                    self.context.FillRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: D2D_RECT_F {
                                left,
                                top: bottom - scale,
                                right: left + width,
                                bottom,
                            },
                            radiusX: scale / 2.0,
                            radiusY: scale / 2.0,
                        },
                        &brush,
                    );
                }
            }
            drawing.finish()?;
            if let Some(tooltip) = &mut self.tooltip {
                tooltip.update(&targets);
            }
            let backdrop: i32 = match bar.backdrop.as_str() {
                "system" => 0,
                "mica" => 2,
                "acrylic" => 3,
                _ => 1,
            };
            let _ = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                (&backdrop as *const i32).cast(),
                size_of::<i32>() as u32,
            );
            Ok(())
        }
    }

    fn draw_borders(&mut self, graphics: &Graphics, borders: &[&BorderScene]) -> Result<()> {
        unsafe {
            let bounds = Rect::new(0, 0, self.rect.width, self.rect.height);
            let regions = borders.iter().flat_map(|border| {
                ring_regions(
                    border.rect.translated(-self.rect.x, -self.rect.y),
                    border.radius.max(border.width).ceil() as i32 + 2,
                    bounds,
                )
            });
            let (pieces, needed) = pack_border_atlas(regions);
            if self
                .border_atlas
                .as_ref()
                .is_none_or(|(capacity, _)| capacity.height < needed.height)
            {
                let surface = graphics.border_surfaces.CreateSurface(
                    needed.width as u32,
                    needed.height as u32,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_ALPHA_MODE_PREMULTIPLIED,
                )?;
                self.border_atlas = Some((needed, surface));
                let mut counters = graphics.diagnostics.borrow_mut();
                counters.border_atlases_created = counters.border_atlases_created.saturating_add(1);
            }
            let (capacity, surface) = self.border_atlas.as_ref().expect("created atlas");
            while self.strips.len() > pieces.len() {
                if let Some(visual) = self.strips.pop() {
                    self.visual.RemoveVisual(&visual)?;
                }
            }
            for (index, piece) in pieces.iter().enumerate() {
                if index == self.strips.len() {
                    let visual = graphics.composition.CreateVisual()?;
                    self.visual.AddVisual(&visual, true, None)?;
                    self.strips.push(visual);
                }
                let visual = &self.strips[index];
                visual.SetContent(surface)?;
                visual.SetClip2(&d2_rect(piece.source))?;
                let transform = if piece.transposed {
                    Matrix3x2 {
                        M11: 0.0,
                        M12: 1.0,
                        M21: 1.0,
                        M22: 0.0,
                        M31: (piece.target.x - piece.source.y) as f32,
                        M32: (piece.target.y - piece.source.x) as f32,
                    }
                } else {
                    Matrix3x2 {
                        M11: 1.0,
                        M12: 0.0,
                        M21: 0.0,
                        M22: 1.0,
                        M31: (piece.target.x - piece.source.x) as f32,
                        M32: (piece.target.y - piece.source.y) as f32,
                    }
                };
                visual.SetTransform2(&transform)?;
            }
            self.border_brushes.truncate(borders.len());
            let painted = borders
                .iter()
                .enumerate()
                .map(|(index, border)| -> Result<_> {
                    let rect = border.rect.translated(-self.rect.x, -self.rect.y);
                    let brush =
                        border_brush(graphics, &mut self.border_brushes, index, &border.colors)?;
                    let span = (rect.width + rect.height) as f32 / 2.0;
                    brush.SetStartPoint(Vector2 { X: rect.x as f32, Y: rect.y as f32 });
                    brush.SetEndPoint(Vector2 { X: rect.x as f32 + span, Y: rect.y as f32 + span });
                    let half = border.width / 2.0;
                    let outline = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F {
                            left: rect.x as f32 + half,
                            top: rect.y as f32 + half,
                            right: rect.right() as f32 - half,
                            bottom: rect.bottom() as f32 - half,
                        },
                        radiusX: border.radius,
                        radiusY: border.radius,
                    };
                    Ok((rect, outline, brush, border.width))
                })
                .collect::<Result<Vec<_>>>()?;
            let drawing = Drawing::begin(graphics, surface, *capacity, DrawKind::Border)?;
            self.context.Clear(Some(&D2D1_COLOR_F::default()));
            let mut base = Matrix3x2::default();
            self.context.GetTransform(&mut base);
            for piece in pieces {
                self.context.SetTransform(&base);
                self.context
                    .PushAxisAlignedClip(&d2_rect(piece.source), D2D1_ANTIALIAS_MODE_ALIASED);
                let _clip = Clip(&self.context);
                let transform = if piece.transposed {
                    Matrix3x2 {
                        M11: 0.0,
                        M12: 1.0,
                        M21: 1.0,
                        M22: 0.0,
                        M31: base.M31 + (piece.source.x - piece.target.y) as f32,
                        M32: base.M32 + (piece.source.y - piece.target.x) as f32,
                    }
                } else {
                    Matrix3x2 {
                        M11: 1.0,
                        M12: 0.0,
                        M21: 0.0,
                        M22: 1.0,
                        M31: base.M31 + (piece.source.x - piece.target.x) as f32,
                        M32: base.M32 + (piece.source.y - piece.target.y) as f32,
                    }
                };
                self.context.SetTransform(&transform);
                for (_, outline, brush, width) in
                    painted.iter().filter(|(rect, _, _, _)| rect.intersects(&piece.target))
                {
                    self.context.DrawRoundedRectangle(outline, brush, *width, None);
                }
            }
            drawing.finish()?;
            Ok(())
        }
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        self.tooltip.take();
        unsafe {
            let registered = self.data.borrow().appbar_registered;
            if registered {
                let mut data = APPBARDATA {
                    cbSize: size_of::<APPBARDATA>() as u32,
                    hWnd: self.hwnd,
                    ..Default::default()
                };
                SHAppBarMessage(ABM_REMOVE, &mut data);
            }
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

struct Drawing {
    context: ID2D1DeviceContext,
    surface: IDCompositionSurface,
    active: bool,
}
impl Drawing {
    fn begin(
        graphics: &Graphics,
        surface: &IDCompositionSurface,
        region: Rect,
        kind: DrawKind,
    ) -> Result<Self> {
        unsafe {
            let context = &graphics.context;
            {
                let mut counters = graphics.diagnostics.borrow_mut();
                let draw = counters.draw(kind);
                draw.begin_draw_calls = draw.begin_draw_calls.saturating_add(1);
                draw.max_requested_pixels = draw
                    .max_requested_pixels
                    .max(region.width.max(0) as u64 * region.height.max(0) as u64);
            }
            let mut offset = windows::Win32::Foundation::POINT::default();
            let update = windows::Win32::Foundation::RECT {
                left: region.x,
                top: region.y,
                right: region.right(),
                bottom: region.bottom(),
            };
            let dxgi: IDXGISurface = match surface.BeginDraw(Some(&update), &mut offset) {
                Ok(surface) => surface,
                Err(error) => {
                    let mut counters = graphics.diagnostics.borrow_mut();
                    let draw = counters.draw(kind);
                    draw.begin_draw_failures = draw.begin_draw_failures.saturating_add(1);
                    return Err(error);
                }
            };
            {
                let mut counters = graphics.diagnostics.borrow_mut();
                let draw = counters.draw(kind);
                match dxgi.GetDesc() {
                    Ok(desc) => draw.observe(desc.Width, desc.Height, desc.SampleDesc.Count),
                    Err(_) => draw.descriptor_failures = draw.descriptor_failures.saturating_add(1),
                }
            }
            let bitmap = match context.CreateBitmapFromDxgiSurface(
                &dxgi,
                Some(&D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                    colorContext: std::mem::ManuallyDrop::new(None),
                }),
            ) {
                Ok(bitmap) => bitmap,
                Err(error) => {
                    let _ = surface.EndDraw();
                    return Err(error);
                }
            };
            context.SetTarget(&bitmap);
            context.SetTransform(&Matrix3x2 {
                M11: 1.0,
                M12: 0.0,
                M21: 0.0,
                M22: 1.0,
                M31: (offset.x - region.x) as f32,
                M32: (offset.y - region.y) as f32,
            });
            context.BeginDraw();
            context.PushAxisAlignedClip(&d2_rect(region), D2D1_ANTIALIAS_MODE_ALIASED);
            Ok(Self { context: context.clone(), surface: surface.clone(), active: true })
        }
    }
    fn finish(mut self) -> Result<()> {
        self.active = false;
        unsafe {
            self.context.PopAxisAlignedClip();
            let draw = self.context.EndDraw(None, None);
            self.context.SetTarget(None);
            let composition = self.surface.EndDraw();
            draw.and(composition)
        }
    }
}
impl Drop for Drawing {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                self.context.PopAxisAlignedClip();
                let _ = self.context.EndDraw(None, None);
                self.context.SetTarget(None);
                let _ = self.surface.EndDraw();
            }
        }
    }
}

struct NativeEdge {
    process_id: u32,
    original: u32,
    current: u32,
}
impl NativeEdge {
    fn owns(&self, id: WindowId) -> bool {
        self.process_id != 0 && crate::platform::process_id(id) == self.process_id
    }
    fn capture(id: WindowId, process_id: u32) -> Option<Self> {
        let mut edge = Self { process_id, original: 0xffff_ffff, current: 0xffff_ffff };
        if !edge.owns(id) {
            return None;
        }
        unsafe {
            // BORDER_COLOR is documented for Set only. Preserve a readable
            // original when supported; otherwise restore Windows' default.
            let mut color = 0u32;
            if DwmGetWindowAttribute(
                crate::platform::hwnd(id),
                DWMWA_BORDER_COLOR,
                (&mut color as *mut u32).cast(),
                size_of::<u32>() as u32,
            )
            .is_ok()
            {
                edge.original = color;
            }
        }
        edge.current = edge.original;
        edge.owns(id).then_some(edge)
    }
    fn apply(&mut self, id: WindowId, color: u32) -> bool {
        if self.owns(id) && crate::platform::set_border_color(id, Some(color)).is_ok() {
            self.current = color;
            true
        } else {
            false
        }
    }
}

/// Keep this on the message-loop thread, dropping it before COM is uninitialized.
pub struct Surfaces {
    bars: BTreeMap<MonitorId, Canvas>,
    borders: BTreeMap<MonitorId, Canvas>,
    graphics: Graphics,
    commands: Arc<Mutex<Vec<String>>>,
    previous: Option<Scene>,
    _tray: Tray,
    native_edges: BTreeMap<WindowId, NativeEdge>,
    preferences: Preferences,
}
impl Surfaces {
    pub fn diagnostics(&self) -> RendererDiagnostics {
        let mut diagnostics = self.graphics.diagnostics.borrow().clone();
        diagnostics.text_layouts_retained =
            self.bars.values().map(|canvas| canvas.text_layouts.0.iter().flatten().count()).sum();
        diagnostics
    }
    /// Nominal RGBA surface pixels, excluding compositor buffers, caches and other process memory.
    pub fn surface_pixel_bytes(&self) -> u64 {
        let pixels = |rect: Rect| rect.width.max(0) as u64 * rect.height.max(0) as u64;
        let bars: u64 = self.bars.values().map(|canvas| pixels(canvas.rect)).sum();
        let borders: u64 = self
            .borders
            .values()
            .filter_map(|canvas| canvas.border_atlas.as_ref())
            .map(|(capacity, _)| pixels(*capacity))
            .sum();
        let feedback = self.bars.values().filter(|canvas| canvas.feedback.is_some()).count() as u64;
        (bars + borders + feedback) * 4
    }
    pub fn backend(&self) -> &'static str {
        self.graphics.backend
    }
    pub fn new() -> Result<Self> {
        let commands = Arc::default();
        Ok(Self {
            bars: BTreeMap::new(),
            borders: BTreeMap::new(),
            graphics: Graphics::new(Rc::default())?,
            _tray: Tray::new(Arc::clone(&commands))?,
            commands,
            previous: None,
            native_edges: BTreeMap::new(),
            preferences: Preferences::read(),
        })
    }
    pub fn drain_commands(&self) -> Vec<String> {
        self.commands.lock().map(|mut commands| std::mem::take(&mut *commands)).unwrap_or_default()
    }
    pub fn hide_bars(&mut self) {
        // Dropping the canvases destroys the windows and releases their AppBar reservations.
        self.bars.clear();
        if let Some(previous) = &mut self.previous {
            // Recreated bars need a fresh draw even when their content is unchanged.
            previous.bars.clear();
        }
    }
    pub fn update(
        &mut self,
        world: &World,
        values: &BTreeMap<String, String>,
        error: Option<&str>,
    ) -> Result<()> {
        if self._tray.data.borrow().preferences_dirty
            || self.bars.values().any(|canvas| canvas.data.borrow().preferences_dirty)
        {
            self.preferences = Preferences::read();
            self._tray.data.borrow_mut().preferences_dirty = false;
            for canvas in self.bars.values() {
                let mut data = canvas.data.borrow_mut();
                data.preferences_dirty = false;
                data.interaction_dirty = true;
            }
        }
        let game = world
            .games
            .foreground
            .and_then(|id| world.games.sessions.get(&id))
            .or_else(|| world.games.sessions.values().next());
        self._tray.update(world.paused, error.is_some(), self.preferences.contrast, game)?;
        self.update_native_edges(world, values.get("system.theme").map(String::as_str));
        match self.render(world, values, error) {
            Ok(()) => Ok(()),
            Err(_) => {
                // Recreate the entire shared device after device loss. Window drop
                // releases AppBar reservations before replacement windows register.
                // Invalidate before the fallible device creation: even if that
                // fails, the next attempt must redraw all discarded canvases.
                self.previous = None;
                self.bars.clear();
                self.borders.clear();
                let diagnostics = self.graphics.diagnostics.clone();
                {
                    let mut counters = diagnostics.borrow_mut();
                    counters.device_recreation_attempts =
                        counters.device_recreation_attempts.saturating_add(1);
                }
                self.graphics = Graphics::new(diagnostics)?;
                self.render(world, values, error)
            }
        }
    }
    fn update_native_edges(&mut self, world: &World, system_theme: Option<&str>) {
        self.native_edges.retain(|id, edge| {
            if !edge.owns(*id) {
                return false;
            }
            if world.windows.get(id).is_some_and(|window| window.info.process_id == edge.process_id)
            {
                true
            } else {
                if edge.current != edge.original {
                    edge.apply(*id, edge.original);
                }
                false
            }
        });
        for (id, window) in &world.windows {
            if crate::platform::process_id(*id) != window.info.process_id {
                continue;
            }
            if !self.native_edges.contains_key(id) {
                let Some(edge) = NativeEdge::capture(*id, window.info.process_id) else {
                    continue;
                };
                self.native_edges.insert(*id, edge);
            }
            let edge = self.native_edges.get_mut(id).expect("captured native edge");
            let desired = if !world.config.borders.enabled
                || world.paused
                || window.suspended
                || world.game_protected(*id)
            {
                edge.original
            } else if window.fullscreen
                || world.native_fullscreen.is_some_and(|(fullscreen, _)| fullscreen == *id)
            {
                0xffff_fffe
            } else if window.floating && self.preferences.contrast {
                u32::from_str_radix(
                    if world.focused == Some(*id) {
                        &self.preferences.highlight[1..]
                    } else {
                        &self.preferences.text[1..]
                    },
                    16,
                )
                .map(rgb_to_colorref)
                .unwrap_or(0xffff_ffff)
            } else if window.floating {
                world
                    .config
                    .color_for(
                        if world.focused == Some(*id) {
                            &world.config.borders.focused
                        } else {
                            &world.config.borders.unfocused
                        },
                        system_theme,
                    )
                    .map(rgb_to_colorref)
                    .unwrap_or(0xffff_ffff)
            } else {
                0xffff_fffe
            };
            if edge.current != desired {
                edge.apply(*id, desired);
            }
        }
    }
    fn render(
        &mut self,
        world: &World,
        values: &BTreeMap<String, String>,
        error: Option<&str>,
    ) -> Result<()> {
        {
            let mut counters = self.graphics.diagnostics.borrow_mut();
            counters.render_calls = counters.render_calls.saturating_add(1);
        }
        self.bars.retain(|id, _| world.monitor(*id).is_some());
        let mut entering = Vec::new();
        let mut scene =
            scene::derive_with_bar_bounds(world, values, error, |id, requested, visible| {
                let canvas =
                    match self.bars.entry(id) {
                        std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                        std::collections::btree_map::Entry::Vacant(entry) => entry.insert(
                            Canvas::new(&self.graphics, requested, false, self.commands.clone())?,
                        ),
                    };
                if visible && !canvas.visible {
                    entering.push(id);
                }
                canvas.position(requested, visible)?;
                Ok::<_, windows::core::Error>(canvas.rect)
            })?;
        self.preferences.apply(&mut scene);
        // Border scenes contain absolute ring rectangles, not monitor bounds.
        // A bounds change can require a new canvas even when those rings match.
        let changed = self.previous.as_ref() != Some(&scene)
            || self.bars.values().any(|canvas| canvas.data.borrow().interaction_dirty)
            || world.monitors.iter().any(|monitor| {
                scene.borders.iter().any(|border| border.monitor == monitor.id)
                    && self
                        .borders
                        .get(&monitor.id)
                        .is_none_or(|canvas| canvas.rect != monitor.bounds)
            });
        if changed {
            let mut counters = self.graphics.diagnostics.borrow_mut();
            counters.changed_scenes = counters.changed_scenes.saturating_add(1);
        }
        let wait_mode = std::env::var("MOON_COMPOSITION_WAIT");
        let wait_after = matches!(wait_mode.as_deref(), Ok("after" | "1"));
        let wait_before = !wait_after && wait_mode.as_deref() != Ok("0");
        if changed && self.previous.is_some() && wait_before {
            // Bound outstanding commits before reusing drawing resources, leaving
            // the message loop free immediately after this scene is submitted.
            unsafe { self.graphics.composition.WaitForCommitCompletion()? };
        }
        self.borders.retain(|id, _| world.monitors.iter().any(|monitor| monitor.id == *id));
        for bar in &scene.bars {
            let canvas = self.bars.get_mut(&bar.monitor).expect("inserted canvas");
            canvas.text_layouts.prepare(bar.items.len(), bar.visible);
            let interaction_dirty = canvas.data.borrow().interaction_dirty;
            let same = self
                .previous
                .as_ref()
                .and_then(|previous| previous.bars.iter().find(|old| old.monitor == bar.monitor))
                .is_some_and(|old| old == bar);
            if (!same || (self.preferences.contrast && interaction_dirty)) && bar.visible {
                canvas.draw_bar(&self.graphics, bar, &self.preferences)?;
            }
            if bar.visible && (!same || interaction_dirty) {
                let (hovered, pressed) = {
                    let mut data = canvas.data.borrow_mut();
                    if data.hovered.is_some_and(|hover| {
                        !data.hit_regions.iter().any(|(rect, _)| *rect == hover)
                    }) {
                        data.hovered = None;
                    }
                    (data.hovered, data.pressed.is_some())
                };
                if let Some(feedback) = &mut canvas.feedback {
                    feedback.update(
                        &self.graphics,
                        hovered,
                        pressed,
                        &bar.background,
                        &self.preferences,
                    )?;
                }
                if let Some(entrance) = &canvas.entrance {
                    unsafe {
                        if entering.contains(&bar.monitor) && self.preferences.animations {
                            entrance.SetOpacity(&transition(&self.graphics, 0.88, 1.0, 0.167)?)?;
                        } else if !self.preferences.animations {
                            entrance.SetOpacity2(1.0)?;
                        }
                    }
                }
            }
            canvas.data.borrow_mut().interaction_dirty = false;
        }
        for monitor in &world.monitors {
            let rings: Vec<_> =
                scene.borders.iter().filter(|border| border.monitor == monitor.id).collect();
            if rings.is_empty() {
                self.borders.remove(&monitor.id);
                continue;
            }
            let recreated =
                self.borders.get(&monitor.id).is_none_or(|canvas| canvas.rect != monitor.bounds);
            if recreated {
                self.borders.remove(&monitor.id);
            }
            if !self.borders.contains_key(&monitor.id) {
                self.borders.insert(
                    monitor.id,
                    Canvas::new(&self.graphics, monitor.bounds, true, self.commands.clone())?,
                );
            }
            let canvas = self.borders.get_mut(&monitor.id).expect("inserted canvas");
            canvas.position(monitor.bounds, true)?;
            let same = self.previous.as_ref().is_some_and(|old| {
                old.borders
                    .iter()
                    .filter(|border| border.monitor == monitor.id)
                    .eq(rings.iter().copied())
            });
            if !same || recreated {
                canvas.draw_borders(&self.graphics, &rings)?;
            }
        }
        if changed {
            unsafe {
                self.graphics.composition.Commit()?;
                {
                    let mut counters = self.graphics.diagnostics.borrow_mut();
                    counters.commits = counters.commits.saturating_add(1);
                }
                if wait_after {
                    self.graphics.composition.WaitForCommitCompletion()?;
                }
            }
        }
        self.previous = Some(scene);
        Ok(())
    }
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn d2_rect(rect: Rect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: rect.x as f32,
        top: rect.y as f32,
        right: rect.right() as f32,
        bottom: rect.bottom() as f32,
    }
}
fn parse_color(text: &str, alpha: f32) -> D2D1_COLOR_F {
    let rgb = u32::from_str_radix(text.trim_start_matches('#'), 16).unwrap_or(0);
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 255) as f32 / 255.0,
        g: ((rgb >> 8) & 255) as f32 / 255.0,
        b: (rgb & 255) as f32 / 255.0,
        a: alpha.clamp(0.0, 1.0),
    }
}

struct Tray {
    hwnd: HWND,
    data: Rc<RefCell<WindowData>>,
    appearance: Option<(u16, u32, bool, bool)>,
}
impl Tray {
    fn new(commands: Arc<Mutex<Vec<String>>>) -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("moon.notification"),
                ..Default::default()
            };
            RegisterClassW(&class);
            let data = Rc::new(RefCell::new(WindowData {
                hit_regions: Vec::new(),
                commands,
                click_through: false,
                appbar: false,
                appbar_dirty: false,
                appbar_registered: false,
                taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
                tray: true,
                preferences_dirty: true,
                tray_tip: "Moon · Open Settings".into(),
                ..Default::default()
            }));
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                w!("moon.notification"),
                w!("moon"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                Some(Rc::as_ptr(&data).cast()),
            )?;
            let taskbar_created = data.borrow().taskbar_created;
            let _ = ChangeWindowMessageFilterEx(hwnd, taskbar_created, MSGFLT_ALLOW, None);
            let _ = ChangeWindowMessageFilterEx(hwnd, TRAY_MESSAGE, MSGFLT_ALLOW, None);
            let icon = match load_brand_icon(101, GetDpiForWindow(hwnd).max(96)) {
                Ok(icon) => icon,
                Err(error) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(error);
                }
            };
            data.borrow_mut().tray_icon = icon;
            let icon = tray_info(hwnd, &data.borrow());
            add_tray(icon);
            Ok(Self { hwnd, data, appearance: None })
        }
    }
}
impl Tray {
    fn update(
        &mut self,
        paused: bool,
        error: bool,
        contrast: bool,
        game: Option<&moon_core::gaming::Game>,
    ) -> Result<()> {
        unsafe {
            let dpi = GetDpiForWindow(self.hwnd).max(96);
            let color = GetSysColor(COLOR_WINDOWTEXT);
            let light_text = (color & 255) + ((color >> 8) & 255) + ((color >> 16) & 255) > 384;
            let resource = if contrast { if light_text { 103 } else { 102 } } else { 101 };
            let appearance = (resource, dpi, paused, error);
            let game_changed = {
                let data = self.data.borrow();
                data.game_active != game.is_some()
                    || game.is_some_and(|g| {
                        data.game_identity.as_ref().is_none_or(|(process, source)| {
                            process != &g.process || *source != g.source
                        })
                    })
            };
            if self.appearance == Some(appearance) && !game_changed {
                return Ok(());
            }
            if self.appearance.is_none_or(|old| old.0 != resource || old.1 != dpi) {
                let icon = load_brand_icon(resource, dpi)?;
                let old = std::mem::replace(&mut self.data.borrow_mut().tray_icon, icon);
                if !old.is_invalid() {
                    let _ = DestroyIcon(old);
                }
            }
            let icon = {
                let mut data = self.data.borrow_mut();
                data.paused = paused;
                data.game_active = game.is_some();
                data.game_identity = game.map(|g| (g.process.clone(), g.source));
                data.tray_tip = if let Some(game) = game {
                    use moon_core::gaming::GameSource;
                    let reason = match game.source {
                        GameSource::Windows => "Windows",
                        GameSource::Manual => "this session",
                        GameSource::Rule => "app rule",
                    };
                    format!("Moon · Game mode · {} ({reason})", game.process)
                } else if error {
                    "Moon · Needs attention".into()
                } else if paused {
                    "Moon · Tiling paused".into()
                } else {
                    "Moon · Open Settings".into()
                };
                tray_info(self.hwnd, &data)
            };
            let _ = Shell_NotifyIconW(NIM_MODIFY, &icon);
            self.appearance = Some(appearance);
            Ok(())
        }
    }
}
fn load_brand_icon(resource: u16, dpi: u32) -> Result<HICON> {
    unsafe {
        let size = GetSystemMetricsForDpi(SM_CXSMICON, dpi).max(16);
        if let Ok(icon) = LoadImageW(
            Some(GetModuleHandleW(None)?.into()),
            PCWSTR(usize::from(resource) as *const u16),
            IMAGE_ICON,
            size,
            size,
            LR_DEFAULTCOLOR,
        ) {
            return Ok(HICON(icon.0));
        }
        // Standalone developer probes do not link application resources.
        let bytes: &[u8] = match resource {
            102 => include_bytes!("../assets/icons/moon-tray-light-64.png"),
            103 => include_bytes!("../assets/icons/moon-tray-dark-64.png"),
            _ => include_bytes!("../assets/icons/moon-256.png"),
        };
        CreateIconFromResourceEx(bytes, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR)
    }
}
unsafe fn tray_info(hwnd: HWND, data: &WindowData) -> NOTIFYICONDATAW {
    let mut icon = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY_MESSAGE,
        hIcon: data.tray_icon,
        ..Default::default()
    };
    let tip = wide(&data.tray_tip);
    let length = tip.len().min(icon.szTip.len());
    icon.szTip[..length].copy_from_slice(&tip[..length]);
    icon.szTip[icon.szTip.len() - 1] = 0;
    icon
}
impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let icon = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &icon);
            let _ = DestroyWindow(self.hwnd);
            let tray_icon = self.data.borrow().tray_icon;
            if !tray_icon.is_invalid() {
                let _ = DestroyIcon(tray_icon);
            }
        }
    }
}
unsafe fn add_tray(mut icon: NOTIFYICONDATAW) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &icon);
        icon.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let _ = Shell_NotifyIconW(NIM_SETVERSION, &icon);
    }
}

impl Drop for Surfaces {
    fn drop(&mut self) {
        for (id, edge) in &mut self.native_edges {
            if edge.current != edge.original {
                edge.apply(*id, edge.original);
            }
        }
    }
}
fn rgb_to_colorref(rgb: u32) -> u32 {
    ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff)
}

fn ring_regions(rect: Rect, thickness: i32, bounds: Rect) -> Vec<Rect> {
    let thickness = thickness.min(rect.width / 2).min(rect.height / 2).max(1);
    [
        Rect::new(rect.x, rect.y, rect.width, thickness),
        Rect::new(rect.x, rect.bottom() - thickness, rect.width, thickness),
        Rect::new(rect.x, rect.y + thickness, thickness, rect.height - 2 * thickness),
        Rect::new(
            rect.right() - thickness,
            rect.y + thickness,
            thickness,
            rect.height - 2 * thickness,
        ),
    ]
    .into_iter()
    .filter_map(|region| {
        let clipped = Rect::from_edges(
            region.x.max(bounds.x),
            region.y.max(bounds.y),
            region.right().min(bounds.right()),
            region.bottom().min(bounds.bottom()),
        );
        (!clipped.is_empty()).then_some(clipped)
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_glyph_pixels_are_centered_at_multiple_scales() -> Result<()> {
        let _apartment = crate::platform::Apartment::new()?;
        let graphics = Graphics::new(Rc::default())?;
        let context = &graphics.context;
        let size = D2D_SIZE_U { width: 128, height: 80 };
        unsafe {
            let bitmap = |options| {
                context.CreateBitmap(
                    size,
                    None,
                    0,
                    &D2D1_BITMAP_PROPERTIES1 {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        bitmapOptions: options,
                        dpiX: 96.0,
                        dpiY: 96.0,
                        ..Default::default()
                    },
                )
            };
            let target = bitmap(D2D1_BITMAP_OPTIONS_TARGET)?;
            let readback = bitmap(D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW)?;
            let brush = context.CreateSolidColorBrush(&parse_color("#ffffff", 1.0), None)?;
            let mut format = None;
            for scale in [1.0f32, 1.25, 1.5, 2.0] {
                for (text, width_dip) in [
                    ("\u{f120}", 20.0),
                    ("\u{f269}", 20.0),
                    ("10", 26.0),
                    ("g", 20.0),
                    ("A long workspace name", 20.0),
                ] {
                    let font_size = 13.0 * scale;
                    let bounds = Rect::new(
                        11,
                        7,
                        (width_dip * scale).round() as i32,
                        (28.0 * scale).round() as i32,
                    );
                    let layout = text_layout(
                        &graphics,
                        &mut format,
                        &mut None,
                        text,
                        bounds,
                        "Segoe UI",
                        font_size,
                    )?;
                    context.SetTarget(&target);
                    context.BeginDraw();
                    context.Clear(None);
                    draw_bar_text(context, &layout, bounds, &brush, true)?;
                    context.EndDraw(None, None)?;
                    context.SetTarget(None);
                    readback.CopyFromBitmap(None, &target, None)?;
                    let mapped = readback.Map(D2D1_MAP_OPTIONS_READ)?;
                    let mut left = size.width as i32;
                    let mut top = size.height as i32;
                    let mut right = 0;
                    let mut bottom = 0;
                    // Each row contains BGRA pixels; ignore faint antialiasing fringes.
                    for y in 0..size.height {
                        let row = std::slice::from_raw_parts(
                            mapped.bits.add((y * mapped.pitch) as usize).cast::<[u8; 4]>(),
                            size.width as usize,
                        );
                        for (x, pixel) in row.iter().enumerate() {
                            if pixel[3] >= 32 {
                                left = left.min(x as i32);
                                right = right.max(x as i32 + 1);
                                top = top.min(y as i32);
                                bottom = bottom.max(y as i32 + 1);
                            }
                        }
                    }
                    readback.Unmap()?;
                    let horizontal = (left - bounds.x, bounds.right() - right);
                    let vertical = (top - bounds.y, bounds.bottom() - bottom);
                    assert!(left < right && top < bottom, "empty ink for {text} at scale {scale}");
                    assert!(
                        horizontal.0 >= 0
                            && horizontal.1 >= 0
                            && vertical.0 >= 0
                            && vertical.1 >= 0,
                        "ink escaped bounds for {text:?} at scale {scale}"
                    );
                    // Grayscale antialiasing and hinting can shift an outer edge by a pixel.
                    assert!(
                        (horizontal.0 - horizontal.1).abs() <= 2
                            && (vertical.0 - vertical.1).abs() <= 2,
                        "{text} scale {scale}: horizontal {horizontal:?}, vertical {vertical:?}"
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn native_surface_factories_share_context_and_commit_without_a_window() {
        let _apartment = crate::platform::Apartment::new().unwrap();
        let diagnostics = Rc::default();
        let graphics = Graphics::new(Rc::clone(&diagnostics)).unwrap();
        // Exercise actual V2 factory surfaces through the retained V1 device,
        // with the production draw path and one context for both resource pools.
        // Backing dimensions are allocator policy, not a contract to assert.
        unsafe {
            let root = graphics.composition.CreateVisual().unwrap();
            let mut surfaces = Vec::new();
            for (factory, rect, kind) in [
                (&graphics.bar_surfaces, Rect::new(0, 0, 3840, 50), DrawKind::Bar),
                (&graphics.bar_surfaces, Rect::new(0, 0, 1, 1), DrawKind::Feedback),
                (&graphics.border_surfaces, Rect::new(0, 0, 1024, 512), DrawKind::Border),
            ] {
                let surface = factory
                    .CreateSurface(
                        rect.width as u32,
                        rect.height as u32,
                        DXGI_FORMAT_B8G8R8A8_UNORM,
                        DXGI_ALPHA_MODE_PREMULTIPLIED,
                    )
                    .unwrap();
                let visual = graphics.composition.CreateVisual().unwrap();
                visual.SetContent(&surface).unwrap();
                root.AddVisual(&visual, true, None).unwrap();
                surfaces.push((surface, rect, kind));
            }
            for _ in 0..4 {
                for (surface, rect, kind) in &surfaces {
                    let drawing = Drawing::begin(&graphics, surface, *rect, *kind).unwrap();
                    graphics.context.Clear(Some(&parse_color("#a46852", 0.75)));
                    drawing.finish().unwrap();
                }
                graphics.composition.Commit().unwrap();
                graphics.composition.WaitForCommitCompletion().unwrap();
            }
        }
        let counters = diagnostics.borrow();
        assert_eq!(counters.devices_created, 1);
        assert_eq!(counters.surface_factories_created, 2);
        assert_eq!(counters.device_recreation_attempts, 0);
        for draws in [&counters.bar, &counters.border, &counters.feedback] {
            assert_eq!(draws.begin_draw_calls, 4);
            assert_eq!(draws.begin_draw_failures, 0);
            assert_eq!(draws.descriptor_failures, 0);
        }
    }

    #[test]
    fn native_render_resources_reuse_and_invalidate_without_changing_styles() {
        let _apartment = crate::platform::Apartment::new().unwrap();
        let diagnostics = Rc::default();
        let graphics = Graphics::new(Rc::clone(&diagnostics)).unwrap();
        let mut format = None;
        let initial = text_format(&graphics, &mut format, "Segoe UI", 14.0).unwrap();
        for _ in 0..128 {
            let reused = text_format(&graphics, &mut format, "Segoe UI", 14.0).unwrap();
            assert_eq!(reused.as_raw(), initial.as_raw());
        }
        assert_eq!(diagnostics.borrow().text_formats_created, 1);
        let resized = text_format(&graphics, &mut format, "Segoe UI", 18.0).unwrap();
        assert_ne!(resized.as_raw(), initial.as_raw());
        let changed_font = text_format(&graphics, &mut format, "Arial", 18.0).unwrap();
        assert_ne!(changed_font.as_raw(), resized.as_raw());
        assert_eq!(diagnostics.borrow().text_formats_created, 3);
        unsafe {
            assert_eq!(changed_font.GetFontSize(), 18.0);
            assert_eq!(changed_font.GetWordWrapping(), DWRITE_WORD_WRAPPING_NO_WRAP);
            assert_eq!(changed_font.GetParagraphAlignment(), DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let mut trimming = DWRITE_TRIMMING::default();
            let mut ellipsis = None;
            changed_font.GetTrimming(&mut trimming, &mut ellipsis).unwrap();
            assert_eq!(trimming.granularity, DWRITE_TRIMMING_GRANULARITY_CHARACTER);
            assert!(ellipsis.is_some());
        }

        let mut brushes = Vec::new();
        let colors = ["#ff0000".into(), "#0000ff".into()];
        let first = border_brush(&graphics, &mut brushes, 0, &colors).unwrap();
        for _ in 0..128 {
            for index in 0..8 {
                let reused = border_brush(&graphics, &mut brushes, index, &colors).unwrap();
                if index == 0 {
                    assert_eq!(reused.as_raw(), first.as_raw());
                }
            }
        }
        assert_eq!(diagnostics.borrow().gradient_brushes_created, 8);
        let recolored =
            border_brush(&graphics, &mut brushes, 0, &["#00ff00".into(), "#ffffff".into()])
                .unwrap();
        assert_ne!(recolored.as_raw(), first.as_raw());
        assert_eq!(diagnostics.borrow().gradient_brushes_created, 9);
        unsafe {
            let stops = recolored.GetGradientStopCollection().unwrap();
            let mut actual = [D2D1_GRADIENT_STOP::default(); 2];
            stops.GetGradientStops(&mut actual);
            assert_eq!(stops.GetColorInterpolationGamma(), D2D1_GAMMA_2_2);
            assert_eq!(stops.GetExtendMode(), D2D1_EXTEND_MODE_CLAMP);
            assert_eq!(actual[0].color, parse_color("#00ff00", 1.0));
            assert_eq!(actual[1].color, parse_color("#ffffff", 1.0));
        }

        let mut layouts = TextLayouts::default();
        layouts.prepare(2, true);
        // The emoji occupies two UTF-16 units before the private-use glyphs.
        let text = "A\u{1f600}\u{f017}\u{f0001}B";
        let bounds = Rect::new(20, 10, 240, 32);
        let initial =
            text_layout(&graphics, &mut format, &mut layouts.0[0], text, bounds, "Segoe UI", 14.0)
                .unwrap();
        for _ in 0..128 {
            let reused = text_layout(
                &graphics,
                &mut format,
                &mut layouts.0[0],
                text,
                bounds.translated(100, 50),
                "Segoe UI",
                14.0,
            )
            .unwrap();
            assert_eq!(reused.as_raw(), initial.as_raw());
        }
        assert_eq!(diagnostics.borrow().text_layouts_created, 1);
        unsafe {
            let mut family = [0u16; 64];
            initial.GetFontFamilyName(0, &mut family, None).unwrap();
            assert_eq!(String::from_utf16_lossy(&family[..8]), "Segoe UI");
            for position in [3, 4] {
                family.fill(0);
                initial.GetFontFamilyName(position, &mut family, None).unwrap();
                let length = family.iter().position(|&unit| unit == 0).unwrap();
                assert_eq!(String::from_utf16_lossy(&family[..length]), "Symbols Nerd Font");
                let mut collection = None;
                initial.GetFontCollection(position, &mut collection, None).unwrap();
                assert_eq!(collection.unwrap().as_raw(), graphics.symbols.as_raw());
            }
            let mut effect = None;
            initial.GetDrawingEffect(0, &mut effect, None).unwrap();
            assert!(effect.is_none(), "theme colours must remain supplied by the drawing brush");
            assert_eq!(initial.GetMaxWidth(), 240.0);
            assert_eq!(initial.GetMaxHeight(), 32.0);
            assert_eq!(initial.GetWordWrapping(), DWRITE_WORD_WRAPPING_NO_WRAP);
            assert_eq!(initial.GetParagraphAlignment(), DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let mut trimming = DWRITE_TRIMMING::default();
            let mut ellipsis = None;
            initial.GetTrimming(&mut trimming, &mut ellipsis).unwrap();
            assert_eq!(trimming.granularity, DWRITE_TRIMMING_GRANULARITY_CHARACTER);
            assert!(ellipsis.is_some());
        }

        let mut previous = initial;
        for (text, font, size, bounds) in [
            ("Changed text", "Segoe UI", 14.0, bounds),
            ("Changed text", "Arial", 14.0, bounds),
            ("Changed text", "Arial", 18.0, bounds),
            ("Changed text", "Arial", 18.0, Rect::new(20, 10, 120, 32)),
            ("Changed text", "Arial", 18.0, Rect::new(20, 10, 120, 40)),
        ] {
            let updated =
                text_layout(&graphics, &mut format, &mut layouts.0[0], text, bounds, font, size)
                    .unwrap();
            assert_ne!(updated.as_raw(), previous.as_raw());
            unsafe {
                assert_eq!(updated.GetMaxWidth(), bounds.width as f32);
                assert_eq!(updated.GetMaxHeight(), bounds.height as f32);
                let mut actual_size = 0.0;
                updated.GetFontSize(0, &mut actual_size, None).unwrap();
                assert_eq!(actual_size, size);
                let mut actual_font = [0u16; 64];
                updated.GetFontFamilyName(0, &mut actual_font, None).unwrap();
                let length = actual_font.iter().position(|&unit| unit == 0).unwrap();
                assert_eq!(String::from_utf16_lossy(&actual_font[..length]), font);
                let mut line = [DWRITE_LINE_METRICS::default(); 1];
                let mut lines = 0;
                updated.GetLineMetrics(Some(&mut line), &mut lines).unwrap();
                assert_eq!(lines, 1);
                assert_eq!(line[0].length as usize, text.encode_utf16().count());
            }
            previous = updated;
        }
        assert_eq!(diagnostics.borrow().text_layouts_created, 6);

        let removed = text_layout(
            &graphics,
            &mut format,
            &mut layouts.0[1],
            "Removed widget",
            bounds,
            "Segoe UI",
            14.0,
        )
        .unwrap();
        layouts.prepare(1, true);
        assert_eq!(layouts.0.len(), 1);
        assert_eq!(layouts.0[0].as_ref().unwrap().layout.as_raw(), previous.as_raw());
        layouts.prepare(2, true);
        assert!(layouts.0[1].is_none(), "removed widget must not retain a history entry");
        let readded = text_layout(
            &graphics,
            &mut format,
            &mut layouts.0[1],
            "Removed widget",
            bounds,
            "Segoe UI",
            14.0,
        )
        .unwrap();
        assert_ne!(readded.as_raw(), removed.as_raw());
        assert_eq!(diagnostics.borrow().text_layouts_created, 8);
        layouts.prepare(0, true);
        assert!(layouts.0.is_empty());
        layouts.prepare(2, true);
        assert!(layouts.0.iter().all(Option::is_none));
        let _ =
            text_layout(&graphics, &mut format, &mut layouts.0[0], text, bounds, "Segoe UI", 14.0)
                .unwrap();
        layouts.prepare(2, false);
        assert!(layouts.0.is_empty(), "a hidden bar must release every retained layout");
        assert_eq!(diagnostics.borrow().text_layouts_created, 9);
    }

    #[test]
    fn backing_size_diagnostics_count_reuse_without_retaining_surfaces() {
        let mut diagnostics = SurfaceDrawDiagnostics::default();
        diagnostics.observe(1024, 512, 1);
        diagnostics.observe(1024, 512, 1);
        diagnostics.observe(1024, 1024, 1);
        diagnostics.observe(1024, 512, 4);
        assert_eq!(diagnostics.sizes.len(), 3);
        assert_eq!(diagnostics.sizes[0].observations, 2);
        assert_eq!(diagnostics.max_observed_backing_pixels, 1024 * 1024);
        assert_eq!(diagnostics.unlisted_size_observations, 0);
    }

    #[test]
    fn backing_size_diagnostics_remain_bounded_after_many_sizes() {
        let mut diagnostics = SurfaceDrawDiagnostics::default();
        for width in 1..=128 {
            diagnostics.observe(width, 512, 1);
        }
        diagnostics.observe(1, 512, 1);
        assert_eq!(diagnostics.sizes.len(), 16);
        assert_eq!(diagnostics.sizes[0].observations, 2);
        assert_eq!(diagnostics.unlisted_size_observations, 112);
        assert_eq!(diagnostics.max_observed_backing_pixels, 128 * 512);
    }

    fn bar_fixture(items: Vec<moon_core::scene::BarItem>) -> BarScene {
        BarScene {
            monitor: MonitorId(1),
            rect: Rect::new(1920, -200, 1600, 40),
            visible: true,
            background: "#141417".into(),
            items,
            font: "Segoe UI".into(),
            opacity: 0.6,
            backdrop: "mica".into(),
        }
    }

    #[test]
    fn hiding_bars_destroys_every_window_and_clears_the_cached_bars() {
        let _apartment = crate::platform::Apartment::new().unwrap();
        let mut surfaces = Surfaces::new().unwrap();
        let mut scene = Scene::default();
        let windows = [MonitorId(1), MonitorId(2)].map(|monitor| {
            let mut bar = bar_fixture(Vec::new());
            bar.monitor = monitor;
            // Create hidden bar windows so the test leaves the work area alone.
            let canvas =
                Canvas::new(&surfaces.graphics, bar.rect, false, surfaces.commands.clone())
                    .unwrap();
            let hwnd = canvas.hwnd;
            surfaces.bars.insert(monitor, canvas);
            scene.bars.push(bar);
            hwnd
        });
        surfaces.previous = Some(scene);

        surfaces.hide_bars();

        for hwnd in windows {
            assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
        }
        assert!(surfaces.bars.is_empty());
        assert!(surfaces.previous.as_ref().unwrap().bars.is_empty());
    }

    #[test]
    fn negotiated_appbar_bounds_are_shared_by_layout_and_hit_targets() {
        let world = World::new(
            moon_core::config::Config::default(),
            vec![moon_core::world::Monitor {
                id: MonitorId(1),
                name: "Fixture".into(),
                bounds: Rect::new(-1920, -400, 1920, 1080),
                work_area: Rect::new(-1920, -400, 1920, 1040),
                scale: 1.0,
            }],
        )
        .unwrap();
        let approved = Rect::new(-1810, -360, 1750, 40);
        let scene = scene::derive_with_bar_bounds(
            &world,
            &BTreeMap::new(),
            None,
            |id, request, visible| {
                assert_eq!(id, MonitorId(1));
                assert_eq!(request, Rect::new(-1920, -400, 1920, 40));
                assert!(visible);
                Ok::<_, ()>(approved)
            },
        )
        .unwrap();
        let bar = &scene.bars[0];
        assert_eq!(bar.rect, approved);
        let targets = bar_targets(bar);
        assert!(!targets.is_empty());
        let client = Rect::new(0, 0, approved.width, approved.height);
        assert!(targets.iter().all(|target| {
            target.rect.intersection_area(&client)
                == i64::from(target.rect.width) * i64::from(target.rect.height)
        }));
    }

    #[test]
    fn adjacent_icon_and_text_share_an_action_without_merging_other_controls() {
        let item = |x, width, label: &str| moon_core::scene::BarItem {
            rect: Rect::new(x, -194, width, 28),
            label: label.into(),
            selected: false,
            center_content: false,
            text: "fixture".into(),
            foreground: "#ffffff".into(),
            background: None,
            command: Some("settings".into()),
            font_size: 14.0,
        };
        let bar = bar_fixture(vec![
            item(1940, 30, "Volume"),
            item(1970, 90, "Volume"),
            item(2060, 50, "Other control"),
            item(2116, 50, "Other control"),
        ]);
        let targets = bar_targets(&bar);
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].rect, Rect::new(20, 6, 120, 28));
        let mut data = WindowData {
            hit_regions: targets
                .into_iter()
                .map(|target| (target.rect, target.command.unwrap()))
                .collect(),
            ..Default::default()
        };
        data.press(Point::new(25, 10)); // Icon.
        assert_eq!(data.release(Point::new(100, 10)), Some("settings".into())); // Label.
        data.press(Point::new(25, 10));
        assert_eq!(data.release(Point::new(150, 10)), None); // Distinct control.
        data.press(Point::new(150, 10));
        assert_eq!(data.release(Point::new(200, 10)), None); // Separate pill.
    }

    #[test]
    fn disabled_transparency_uses_opaque_theme_and_contrast_keeps_system_colours() {
        let mut preferences = Preferences {
            animations: true,
            contrast: false,
            transparency: false,
            window: "#000000".into(),
            text: "#ffffff".into(),
            highlight: "#ffff00".into(),
            highlight_text: "#000000".into(),
            button: "#222222".into(),
            button_text: "#ffffff".into(),
        };
        let original = Scene { bars: vec![bar_fixture(Vec::new())], borders: Vec::new() };
        let mut opaque = original.clone();
        preferences.apply(&mut opaque);
        assert_eq!(opaque.bars[0].opacity, 1.0);
        assert_eq!(opaque.bars[0].backdrop, "none");
        assert_eq!(opaque.bars[0].background, original.bars[0].background);
        preferences.transparency = true;
        let mut enabled = original.clone();
        preferences.apply(&mut enabled);
        assert_eq!(enabled, original);
        preferences.contrast = true;
        preferences.apply(&mut enabled);
        assert_eq!(enabled.bars[0].opacity, 1.0);
        assert_eq!(enabled.bars[0].backdrop, "none");
        assert_eq!(enabled.bars[0].background, preferences.window);
    }

    #[test]
    fn bar_activation_requires_press_and_release_inside_the_same_action() {
        let mut data = WindowData {
            hit_regions: vec![
                (Rect::new(0, 0, 20, 20), "workspace 1".into()),
                (Rect::new(30, 0, 20, 20), "workspace 2".into()),
            ],
            ..Default::default()
        };
        assert_eq!(data.release(Point::new(5, 5)), None);
        data.press(Point::new(5, 5));
        assert_eq!(data.release(Point::new(35, 5)), None);
        data.press(Point::new(5, 5));
        assert_eq!(data.release(Point::new(5, 5)), Some("workspace 1".into()));
        data.press(Point::new(5, 5));
        data.hit_regions[0].1 = "quit".into();
        assert_eq!(data.release(Point::new(5, 5)), None);
        data.press(Point::new(25, 5));
        assert_eq!(data.release(Point::new(35, 5)), None);
    }

    #[test]
    fn native_edge_rejects_wrong_process_owner() {
        struct OwnedWindow(HWND);
        impl Drop for OwnedWindow {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                }
            }
        }
        let window = OwnedWindow(unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("moon native edge ownership test"),
                WS_POPUP,
                0,
                0,
                64,
                64,
                None,
                None,
                Some(GetModuleHandleW(None).unwrap().into()),
                None,
            )
            .unwrap()
        });
        let id = crate::platform::window_id(window.0);
        let process_id = crate::platform::process_id(id);
        let wrong_owner = process_id.wrapping_add(1).max(1);
        let initial = 0x0033_2211;
        let replacement = 0x0066_5544;
        crate::platform::set_border_color(id, Some(initial)).unwrap();
        assert!(NativeEdge::capture(id, wrong_owner).is_none());
        let mut stale =
            NativeEdge { process_id: wrong_owner, original: replacement, current: initial };
        assert!(!stale.apply(id, replacement));
        let mut current = NativeEdge::capture(id, process_id).unwrap();
        assert_eq!(current.process_id, process_id);
        assert_eq!(stale.current, initial);
        assert!(current.apply(id, replacement));
        assert_eq!(current.current, replacement);
        assert!(current.apply(id, current.original));
        drop(window);
        assert!(NativeEdge::capture(id, process_id).is_none());
        assert!(!current.apply(id, replacement));
    }

    #[test]
    fn atlas_slices_preserve_all_pixels_without_overlap() {
        let originals: Vec<_> = (0..40)
            .map(|index| {
                let long = 1 + index * 211;
                let short = 1 + index % 31;
                let (width, height) = if index % 2 == 0 { (long, short) } else { (short, long) };
                Rect::new(index * 10_000, index * 10_000, width, height)
            })
            .collect();
        let (pieces, capacity) = pack_border_atlas(originals.iter().copied());
        let area = |rect: Rect| u64::from(rect.width as u32) * u64::from(rect.height as u32);
        assert_eq!(
            pieces.iter().map(|piece| area(piece.target)).sum::<u64>(),
            originals.iter().copied().map(area).sum::<u64>(),
        );
        for (index, piece) in pieces.iter().enumerate() {
            assert!(capacity.contains(Point::new(piece.source.x, piece.source.y)));
            assert!(
                capacity.contains(Point::new(piece.source.right() - 1, piece.source.bottom() - 1))
            );
            assert!(originals.iter().any(|rect| {
                rect.contains(Point::new(piece.target.x, piece.target.y))
                    && rect
                        .contains(Point::new(piece.target.right() - 1, piece.target.bottom() - 1))
            }));
            assert_eq!(area(piece.source), area(piece.target));
            for other in &pieces[index + 1..] {
                assert!(!piece.source.intersects(&other.source));
                assert!(!piece.target.intersects(&other.target));
            }
        }
    }
}
