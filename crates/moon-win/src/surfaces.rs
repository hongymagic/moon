//! Main-thread DirectComposition surfaces. All visible contents come from core scenes.
//! A shared D3D/D2D device serves every monitor; unchanged scenes are never redrawn.
use moon_core::{
    geometry::{Point, Rect},
    ids::{MonitorId, WindowId},
    scene::{self, BarScene, BorderScene, Scene},
    world::World,
};
use std::{
    collections::BTreeMap,
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

struct Graphics {
    backend: &'static str,
    context: ID2D1DeviceContext,
    composition: IDCompositionDevice,
    write: IDWriteFactory5,
    symbols: IDWriteFontCollection,
    loader: IDWriteInMemoryFontFileLoader,
}

impl Graphics {
    fn new() -> Result<Self> {
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
            let composition = DCompositionCreateDevice(&dxgi_device)?;
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
            Ok(Self { backend, context, composition, write, symbols, loader })
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
        AccessibilityPreferences {
            motion_allowed: animations.as_bool() && !high_contrast,
            high_contrast,
        }
    }
}

#[derive(Clone, PartialEq)]
struct Preferences {
    animations: bool,
    contrast: bool,
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
    tray_icon: HICON,
    tray_tip: String,
}

impl WindowData {
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
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowData;
        if !pointer.is_null() {
            let data = &mut *pointer;
            if message == data.taskbar_created {
                if data.appbar {
                    data.appbar_dirty = true;
                    data.appbar_registered = false;
                }
                if data.tray {
                    add_tray(hwnd, data);
                    data.preferences_dirty = true;
                }
            }
            if message == TRAY_MESSAGE && data.tray {
                let notification = lparam.0 as u32 & 0xffff;
                if (notification == NIN_SELECT || notification == (NIN_SELECT | NINF_KEY))
                    && let Ok(mut commands) = data.commands.lock()
                {
                    commands.push("settings".into());
                }
                if notification == WM_CONTEXTMENU || notification == WM_RBUTTONUP {
                    let menu = CreatePopupMenu();
                    if let Ok(menu) = menu {
                        for (id, label) in [
                            (6, "Settings…"),
                            (1, if data.paused { "Resume tiling" } else { "Pause tiling" }),
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
                            _ => None,
                        };
                        if let Some(command) = command
                            && let Ok(mut commands) = data.commands.lock()
                        {
                            commands.push(command.into());
                        }
                        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
                        let _ = DestroyMenu(menu);
                        if selected == 0 {
                            let _ = Shell_NotifyIconW(NIM_SETFOCUS, &tray_info(hwnd, data));
                        }
                    }
                }
                return LRESULT(0);
            }
            if message == APPBAR_MESSAGE && data.appbar {
                if wparam.0 == ABN_POSCHANGED as usize {
                    data.appbar_dirty = true;
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
                    data.preferences_dirty = true;
                    data.interaction_dirty = true;
                }
                WM_MOUSEMOVE if data.appbar => {
                    let point = mouse_point(lparam);
                    let hovered = data
                        .hit_regions
                        .iter()
                        .find(|(rect, _)| rect.contains(point))
                        .map(|(rect, _)| *rect);
                    if hovered != data.hovered {
                        data.hovered = hovered;
                        data.interaction_dirty = true;
                    }
                    let _ = TrackMouseEvent(&mut TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        ..Default::default()
                    });
                }
                WM_MOUSELEAVE if data.appbar => {
                    data.hovered = None;
                    data.interaction_dirty = true;
                }
                WM_SETCURSOR if data.appbar && data.hovered.is_some() => {
                    SetCursor(LoadCursorW(None, IDC_HAND).ok());
                    return LRESULT(1);
                }
                WM_LBUTTONDOWN if data.appbar => {
                    let point = mouse_point(lparam);
                    data.press(point);
                    if data.pressed.is_some() {
                        SetCapture(hwnd);
                        data.interaction_dirty = true;
                    }
                    return LRESULT(0);
                }
                WM_CAPTURECHANGED if data.appbar => {
                    data.pressed = None;
                    data.interaction_dirty = true;
                }
                WM_NCHITTEST if data.click_through => return LRESULT(HTTRANSPARENT as isize),
                WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
                WM_ERASEBKGND => return LRESULT(1),
                WM_PAINT => {
                    let _ = ValidateRect(Some(hwnd), None);
                    return LRESULT(0);
                }
                WM_LBUTTONUP if data.appbar => {
                    let point = mouse_point(lparam);
                    if let Some(command) = data.release(point)
                        && let Ok(mut commands) = data.commands.lock()
                    {
                        commands.push(command);
                    }
                    let _ = ReleaseCapture();
                    data.interaction_dirty = true;
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
            let surface = graphics.composition.CreateSurface(
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
                let drawing =
                    Drawing::begin(&graphics.context, &self.surface, Rect::new(0, 0, 1, 1))?;
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
    fn update(&mut self, bar: &BarScene) {
        let next = bar
            .items
            .iter()
            .filter(|item| !item.label.is_empty() && !item.rect.is_empty())
            .map(|item| (item.rect.translated(-bar.rect.x, -bar.rect.y), wide(&item.label)))
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

struct Canvas {
    hwnd: HWND,
    data: Box<WindowData>,
    rect: Rect,
    context: ID2D1DeviceContext,
    surface: Option<IDCompositionSurface>,
    strips: Vec<IDCompositionVisual>,
    border_atlas: Option<(Rect, IDCompositionSurface)>,
    _target: IDCompositionTarget,
    visual: IDCompositionVisual,
    visible: bool,
    reservation: Option<windows::Win32::Foundation::RECT>,
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
            let mut data = Box::new(WindowData {
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
            });
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
                Some((&mut *data as *mut WindowData).cast()),
            )?;
            if !border {
                // Explorer must still deliver callbacks when moon has UIAccess.
                let _ = ChangeWindowMessageFilterEx(hwnd, data.taskbar_created, MSGFLT_ALLOW, None);
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
                    Some(graphics.composition.CreateSurface(
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
            Ok(Self {
                hwnd,
                data,
                rect,
                context,
                surface,
                strips: Vec::new(),
                border_atlas: None,
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

    fn position(&mut self, rect: Rect, visible: bool) -> Result<()> {
        unsafe {
            if self.rect != rect || self.visible != visible {
                SetWindowPos(
                    self.hwnd,
                    Some(if self.data.click_through { HWND_BOTTOM } else { HWND_TOPMOST }),
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    SWP_NOACTIVATE | if visible { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW },
                )?;
            }
            if self.data.appbar
                && (self.rect != rect || self.visible != visible || self.data.appbar_dirty)
            {
                let mut appbar = APPBARDATA {
                    cbSize: size_of::<APPBARDATA>() as u32,
                    hWnd: self.hwnd,
                    uCallbackMessage: APPBAR_MESSAGE,
                    uEdge: ABE_TOP,
                    rc: windows::Win32::Foundation::RECT {
                        left: rect.x,
                        top: rect.y,
                        right: rect.right(),
                        bottom: rect.y + if visible { rect.height } else { 0 },
                    },
                    ..Default::default()
                };
                if !self.data.appbar_registered {
                    self.data.appbar_registered = SHAppBarMessage(ABM_NEW, &mut appbar) != 0;
                    self.reservation = None;
                }
                SHAppBarMessage(ABM_QUERYPOS, &mut appbar);
                appbar.rc.bottom = appbar.rc.top + if visible { rect.height } else { 0 };
                if self.reservation != Some(appbar.rc) {
                    SHAppBarMessage(ABM_SETPOS, &mut appbar);
                    self.reservation = Some(appbar.rc);
                }
                self.data.appbar_dirty = false;
            }
            self.rect = rect;
            self.visible = visible;
            Ok(())
        }
    }

    fn text(
        &self,
        graphics: &Graphics,
        text: &str,
        bounds: Rect,
        font: &str,
        size: f32,
        color: &str,
    ) -> Result<()> {
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
            let utf16: Vec<u16> = text.encode_utf16().collect();
            let layout = graphics.write.CreateTextLayout(
                &utf16,
                &format,
                bounds.width.max(1) as f32,
                bounds.height.max(1) as f32,
            )?;
            let mut offset = 0u32;
            for character in text.chars() {
                let len = character.len_utf16() as u32;
                if ('\u{e000}'..='\u{f8ff}').contains(&character)
                    || ('\u{f0000}'..='\u{10ffff}').contains(&character)
                {
                    let range = DWRITE_TEXT_RANGE { startPosition: offset, length: len };
                    layout.SetFontCollection(&graphics.symbols, range)?;
                    layout.SetFontFamilyName(w!("Symbols Nerd Font"), range)?;
                }
                offset += len;
            }
            let brush = self.context.CreateSolidColorBrush(&parse_color(color, 1.0), None)?;
            self.context.DrawTextLayout(
                Vector2 { X: bounds.x as f32, Y: bounds.y as f32 },
                &layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
            Ok(())
        }
    }

    fn draw_bar(
        &mut self,
        graphics: &Graphics,
        bar: &BarScene,
        preferences: &Preferences,
    ) -> Result<()> {
        unsafe {
            let drawing = Drawing::begin(
                &self.context,
                self.surface.as_ref().expect("bar surface"),
                Rect::new(0, 0, self.rect.width, self.rect.height),
            )?;
            self.context.Clear(Some(&parse_color(&bar.background, bar.opacity)));
            self.data.hit_regions.clear();
            let scale = GetDpiForWindow(self.hwnd).max(96) as f32 / 96.0;
            for item in &bar.items {
                let bounds = item.rect.translated(-bar.rect.x, -bar.rect.y);
                let highlighted = preferences.contrast
                    && item.command.is_some()
                    && self.data.hovered == Some(bounds);
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
                self.text(
                    graphics,
                    &item.text,
                    bounds.inset(moon_core::geometry::Edges {
                        left: (7.0 * scale).round() as i32,
                        right: (7.0 * scale).round() as i32,
                        top: 0,
                        bottom: 0,
                    }),
                    &bar.font,
                    item.font_size,
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
                if let Some(command) = &item.command {
                    self.data.hit_regions.push((bounds, command.clone()));
                }
            }
            drawing.finish()?;
            if let Some(tooltip) = &mut self.tooltip {
                tooltip.update(bar);
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
                let surface = graphics.composition.CreateSurface(
                    needed.width as u32,
                    needed.height as u32,
                    DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_ALPHA_MODE_PREMULTIPLIED,
                )?;
                self.border_atlas = Some((needed, surface));
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
            let painted = borders
                .iter()
                .map(|border| -> Result<_> {
                    let rect = border.rect.translated(-self.rect.x, -self.rect.y);
                    let stops = [
                        D2D1_GRADIENT_STOP {
                            position: 0.0,
                            color: parse_color(&border.colors[0], 1.0),
                        },
                        D2D1_GRADIENT_STOP {
                            position: 1.0,
                            color: parse_color(&border.colors[1], 1.0),
                        },
                    ];
                    let target: ID2D1RenderTarget = self.context.cast()?;
                    let collection = target.CreateGradientStopCollection(
                        &stops,
                        D2D1_GAMMA_2_2,
                        D2D1_EXTEND_MODE_CLAMP,
                    )?;
                    let span = (rect.width + rect.height) as f32 / 2.0;
                    let brush = target.CreateLinearGradientBrush(
                        &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                            startPoint: Vector2 { X: rect.x as f32, Y: rect.y as f32 },
                            endPoint: Vector2 { X: rect.x as f32 + span, Y: rect.y as f32 + span },
                        },
                        None,
                        &collection,
                    )?;
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
            let drawing = Drawing::begin(&self.context, surface, *capacity)?;
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
            if self.data.appbar {
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
        context: &ID2D1DeviceContext,
        surface: &IDCompositionSurface,
        region: Rect,
    ) -> Result<Self> {
        unsafe {
            let mut offset = windows::Win32::Foundation::POINT::default();
            let update = windows::Win32::Foundation::RECT {
                left: region.x,
                top: region.y,
                right: region.right(),
                bottom: region.bottom(),
            };
            let dxgi: IDXGISurface = surface.BeginDraw(Some(&update), &mut offset)?;
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
            graphics: Graphics::new()?,
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
    pub fn update(
        &mut self,
        world: &World,
        values: &BTreeMap<String, String>,
        error: Option<&str>,
    ) -> Result<()> {
        if self._tray._data.preferences_dirty
            || self.bars.values().any(|canvas| canvas.data.preferences_dirty)
        {
            self.preferences = Preferences::read();
            self._tray._data.preferences_dirty = false;
            for canvas in self.bars.values_mut() {
                canvas.data.preferences_dirty = false;
                canvas.data.interaction_dirty = true;
            }
        }
        self._tray.update(world.paused, error.is_some(), self.preferences.contrast)?;
        self.update_native_edges(world, values.get("system.theme").map(String::as_str));
        let mut scene = scene::derive(world, values, error);
        self.preferences.apply(&mut scene);
        match self.render(world, scene.clone()) {
            Ok(()) => Ok(()),
            Err(_) => {
                // Recreate the entire shared device after device loss. Window drop
                // releases AppBar reservations before replacement windows register.
                // Invalidate before the fallible device creation: even if that
                // fails, the next attempt must redraw all discarded canvases.
                self.previous = None;
                self.bars.clear();
                self.borders.clear();
                self.graphics = Graphics::new()?;
                self.render(world, scene)
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
            let desired = if !world.config.borders.enabled || world.paused || window.suspended {
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
    fn render(&mut self, world: &World, scene: Scene) -> Result<()> {
        // Border scenes contain absolute ring rectangles, not monitor bounds.
        // A bounds change can require a new canvas even when those rings match.
        let changed = self.previous.as_ref() != Some(&scene)
            || self.bars.values().any(|canvas| canvas.data.interaction_dirty)
            || world.monitors.iter().any(|monitor| {
                scene.borders.iter().any(|border| border.monitor == monitor.id)
                    && self
                        .borders
                        .get(&monitor.id)
                        .is_none_or(|canvas| canvas.rect != monitor.bounds)
            });
        let wait_mode = std::env::var("MOON_COMPOSITION_WAIT");
        let wait_after = matches!(wait_mode.as_deref(), Ok("after" | "1"));
        let wait_before = !wait_after && wait_mode.as_deref() != Ok("0");
        if changed && self.previous.is_some() && wait_before {
            // Bound outstanding commits before reusing drawing resources, leaving
            // the message loop free immediately after this scene is submitted.
            unsafe { self.graphics.composition.WaitForCommitCompletion()? };
        }
        self.bars.retain(|id, _| scene.bars.iter().any(|bar| bar.monitor == *id));
        self.borders.retain(|id, _| world.monitors.iter().any(|monitor| monitor.id == *id));
        for bar in &scene.bars {
            let recreated = self.bars.get(&bar.monitor).is_none_or(|canvas| {
                canvas.rect.width != bar.rect.width || canvas.rect.height != bar.rect.height
            });
            if recreated {
                self.bars.remove(&bar.monitor);
            }
            if !self.bars.contains_key(&bar.monitor) {
                self.bars.insert(
                    bar.monitor,
                    Canvas::new(&self.graphics, bar.rect, false, self.commands.clone())?,
                );
            }
            let canvas = self.bars.get_mut(&bar.monitor).expect("inserted canvas");
            let entering = bar.visible && !canvas.visible;
            canvas.position(bar.rect, bar.visible)?;
            let same = self
                .previous
                .as_ref()
                .and_then(|previous| previous.bars.iter().find(|old| old.monitor == bar.monitor))
                .is_some_and(|old| old == bar);
            if (!same || recreated || (self.preferences.contrast && canvas.data.interaction_dirty))
                && bar.visible
            {
                canvas.draw_bar(&self.graphics, bar, &self.preferences)?;
            }
            if bar.visible && (!same || recreated || canvas.data.interaction_dirty) {
                if canvas.data.hovered.is_some_and(|hover| {
                    !canvas.data.hit_regions.iter().any(|(rect, _)| *rect == hover)
                }) {
                    canvas.data.hovered = None;
                }
                if let Some(feedback) = &mut canvas.feedback {
                    feedback.update(
                        &self.graphics,
                        canvas.data.hovered,
                        canvas.data.pressed.is_some(),
                        &bar.background,
                        &self.preferences,
                    )?;
                }
                if let Some(entrance) = &canvas.entrance {
                    unsafe {
                        if entering && self.preferences.animations {
                            entrance.SetOpacity(&transition(&self.graphics, 0.88, 1.0, 0.167)?)?;
                        } else if !self.preferences.animations {
                            entrance.SetOpacity2(1.0)?;
                        }
                    }
                }
            }
            canvas.data.interaction_dirty = false;
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
    _data: Box<WindowData>,
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
            let mut data = Box::new(WindowData {
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
            });
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
                Some((&mut *data as *mut WindowData).cast()),
            )?;
            let _ = ChangeWindowMessageFilterEx(hwnd, data.taskbar_created, MSGFLT_ALLOW, None);
            let _ = ChangeWindowMessageFilterEx(hwnd, TRAY_MESSAGE, MSGFLT_ALLOW, None);
            data.tray_icon = match load_brand_icon(101, GetDpiForWindow(hwnd).max(96)) {
                Ok(icon) => icon,
                Err(error) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(error);
                }
            };
            add_tray(hwnd, &data);
            Ok(Self { hwnd, _data: data, appearance: None })
        }
    }
}
impl Tray {
    fn update(&mut self, paused: bool, error: bool, contrast: bool) -> Result<()> {
        unsafe {
            let dpi = GetDpiForWindow(self.hwnd).max(96);
            let color = GetSysColor(COLOR_WINDOWTEXT);
            let light_text = (color & 255) + ((color >> 8) & 255) + ((color >> 16) & 255) > 384;
            let resource = if contrast { if light_text { 103 } else { 102 } } else { 101 };
            let appearance = (resource, dpi, paused, error);
            if self.appearance == Some(appearance) {
                return Ok(());
            }
            if self.appearance.is_none_or(|old| old.0 != resource || old.1 != dpi) {
                let icon = load_brand_icon(resource, dpi)?;
                let old = std::mem::replace(&mut self._data.tray_icon, icon);
                if !old.is_invalid() {
                    let _ = DestroyIcon(old);
                }
            }
            self._data.paused = paused;
            self._data.tray_tip = if error {
                "Moon · Configuration needs attention"
            } else if paused {
                "Moon · Tiling paused"
            } else {
                "Moon · Open Settings"
            }
            .into();
            let icon = tray_info(self.hwnd, &self._data);
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
            if !self._data.tray_icon.is_invalid() {
                let _ = DestroyIcon(self._data.tray_icon);
            }
        }
    }
}
unsafe fn add_tray(hwnd: HWND, data: &WindowData) {
    unsafe {
        let mut icon = tray_info(hwnd, data);
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
