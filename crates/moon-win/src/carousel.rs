//! Short-lived, clipped DWM presentation for workspace canvases.
//! No foreign styles, regions, parenting, capture or input injection are used.
use crate::platform;
use moon_core::{
    geometry::{Point, Rect},
    ids::{WindowId, WorkspaceId},
    motion::WorkspaceMotion,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Dwm::*, Gdi::*},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON, VK_XBUTTON1, VK_XBUTTON2,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, Result, w},
};

const WAKE: u32 = WM_APP + 59;
const DURATION: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRef {
    pub id: WindowId,
    pub process_id: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageWindow {
    pub window: WindowRef,
    pub rect: Rect,
}
#[derive(Clone, Debug)]
pub struct Transition {
    pub from_workspace: WorkspaceId,
    pub to_workspace: WorkspaceId,
    /// Back-to-front order within each workspace; an empty workspace is valid.
    pub from: Vec<PageWindow>,
    pub to: Vec<PageWindow>,
    pub viewport: Rect,
    /// Positive means the new page enters from the right.
    pub direction: i32,
    pub background_rgb: u32,
}

#[derive(Default)]
struct Input {
    wake: Cell<HWND>,
    showing: Cell<bool>,
    cancelled: Cell<bool>,
    swallowed: Cell<u8>,
    origin: Cell<POINT>,
    viewports: RefCell<Vec<Rect>>,
}
thread_local! { static INPUT: RefCell<Weak<Input>> = const { RefCell::new(Weak::new()) }; }
impl Input {
    fn wake(&self) {
        unsafe {
            let _ = PostMessageW(Some(self.wake.get()), WAKE, WPARAM(0), LPARAM(0));
        }
    }
    fn cancel(&self) {
        if !self.cancelled.replace(true) {
            self.wake();
        }
    }
    fn contains(&self, point: POINT) -> bool {
        self.viewports.borrow().iter().any(|rect| rect.contains(Point::new(point.x, point.y)))
    }
}

fn button(message: u32, data: u32) -> Option<(u8, bool)> {
    match message {
        WM_LBUTTONDOWN => Some((1, true)),
        WM_LBUTTONUP => Some((1, false)),
        WM_RBUTTONDOWN => Some((2, true)),
        WM_RBUTTONUP => Some((2, false)),
        WM_MBUTTONDOWN => Some((4, true)),
        WM_MBUTTONUP => Some((4, false)),
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            Some((if data >> 16 == 1 { 8 } else { 16 }, message == WM_XBUTTONDOWN))
        }
        _ => None,
    }
}
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let consumed = code >= 0
        && INPUT.with(|slot| {
            let Some(input) = slot.borrow().upgrade() else {
                return false;
            };
            let event = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            let message = wparam.0 as u32;
            if let Some((bit, down)) = button(message, event.mouseData) {
                if !down && input.swallowed.get() & bit != 0 {
                    input.swallowed.set(input.swallowed.get() & !bit);
                    input.wake();
                    return true;
                }
                if down && input.showing.get() && input.contains(event.pt) {
                    input.swallowed.set(input.swallowed.get() | bit);
                    input.cancel();
                    return true;
                }
            }
            if input.showing.get() {
                if message == WM_MOUSEMOVE && event.pt != input.origin.get() {
                    input.cancel();
                }
                if matches!(message, WM_MOUSEWHEEL | WM_MOUSEHWHEEL) && input.contains(event.pt) {
                    input.cancel();
                    return true;
                }
            }
            false
        });
    if consumed { LRESULT(1) } else { unsafe { CallNextHookEx(None, code, wparam, lparam) } }
}

struct WindowData {
    input: Rc<Input>,
    color: COLORREF,
}
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *mut WindowData;
    if message == WM_NCDESTROY {
        if !pointer.is_null() {
            unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                drop(Box::from_raw(pointer));
            }
        }
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    if !pointer.is_null() {
        let data = unsafe { &*pointer };
        match message {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                unsafe {
                    let dc = BeginPaint(window, &mut paint);
                    let mut rect = RECT::default();
                    let _ = GetClientRect(window, &mut rect);
                    let brush = CreateSolidBrush(data.color);
                    FillRect(dc, &rect, brush);
                    let _ = DeleteObject(brush.into());
                    let _ = EndPaint(window, &paint);
                }
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_MOUSEMOVE => {
                let mut point = POINT::default();
                if unsafe { GetCursorPos(&mut point) }.is_ok() && point != data.input.origin.get() {
                    data.input.cancel();
                }
                return LRESULT(0);
            }
            // The overlay owns these messages; it never replays them to an app.
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL | WM_POINTERDOWN | WM_POINTERUPDATE => {
                data.input.cancel();
                return LRESULT(0);
            }
            WM_POINTERUP => return LRESULT(0),
            WAKE => return LRESULT(0),
            _ => {}
        }
    }
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

struct OwnedWindow(HWND);
impl Drop for OwnedWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
fn create_window(input: &Rc<Input>, rect: Option<Rect>, rgb: u32) -> Result<OwnedWindow> {
    let instance = unsafe { GetModuleHandleW(None)? };
    let bounds = rect.unwrap_or(Rect::new(0, 0, 0, 0));
    let handle = unsafe {
        CreateWindowExW(
            if rect.is_some() {
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
            } else {
                WINDOW_EX_STYLE(0)
            },
            w!("moon.carousel"),
            w!("Moon workspace transition"),
            WS_POPUP,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            if rect.is_none() { Some(HWND_MESSAGE) } else { None },
            None,
            Some(instance.into()),
            None,
        )?
    };
    let color = COLORREF(((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff));
    let data = Box::into_raw(Box::new(WindowData { input: input.clone(), color }));
    unsafe {
        SetWindowLongPtrW(handle, GWLP_USERDATA, data as isize);
    }
    Ok(OwnedWindow(handle))
}

fn valid(window: WindowRef) -> bool {
    if platform::process_id(window.id) != window.process_id
        || !platform::is_window(window.id)
        || !platform::window_on_current_virtual_desktop(window.id).unwrap_or(false)
    {
        return false;
    }
    let mut affinity = 0;
    // Query is documented to succeed only for layered windows. DWM remains the
    // authority for other windows; protected/black content has no capture fallback.
    !(unsafe { GetWindowDisplayAffinity(platform::hwnd(window.id), &mut affinity) }.is_ok()
        && affinity != 0)
}
fn motion_allowed() -> bool {
    unsafe {
        let mut animate = BOOL(0);
        let mut contrast =
            HIGHCONTRASTW { cbSize: size_of::<HIGHCONTRASTW>() as u32, ..Default::default() };
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut animate as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
            && SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.cbSize,
                Some((&mut contrast as *mut HIGHCONTRASTW).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .is_ok()
            && animate.as_bool()
            && (contrast.dwFlags & HCF_HIGHCONTRASTON).0 == 0
    }
}
fn mouse_button_held() -> bool {
    [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
}

struct Thumbnail {
    handle: isize,
    source: WindowRef,
}
impl Drop for Thumbnail {
    fn drop(&mut self) {
        unsafe {
            let _ = DwmUnregisterThumbnail(self.handle);
        }
    }
}
impl Thumbnail {
    fn new(destination: HWND, source: WindowRef) -> Result<Self> {
        if !valid(source) {
            return Err(windows::core::Error::from_hresult(E_INVALIDARG));
        }
        Ok(Self {
            handle: unsafe { DwmRegisterThumbnail(destination, platform::hwnd(source.id))? },
            source,
        })
    }
    fn draw(&self, destination: Rect, viewport: Rect) -> Result<()> {
        let dimensions = unsafe { DwmQueryThumbnailSourceSize(self.handle)? };
        if dimensions.cx <= 0 || dimensions.cy <= 0 {
            return Err(windows::core::Error::from_hresult(E_FAIL));
        }
        let mut outer = RECT::default();
        unsafe {
            GetWindowRect(platform::hwnd(self.source.id), &mut outer)?;
        }
        let frame = platform::window_rect(self.source.id)?;
        let width = outer.right - outer.left;
        let height = outer.bottom - outer.top;
        if width <= 0 || height <= 0 {
            return Err(windows::core::Error::from_hresult(E_FAIL));
        }
        let source = Rect::from_edges(
            ((i64::from(frame.x - outer.left) * i64::from(dimensions.cx)) / i64::from(width))
                as i32,
            ((i64::from(frame.y - outer.top) * i64::from(dimensions.cy)) / i64::from(height))
                as i32,
            ((i64::from(frame.right() - outer.left) * i64::from(dimensions.cx)) / i64::from(width))
                as i32,
            ((i64::from(frame.bottom() - outer.top) * i64::from(dimensions.cy)) / i64::from(height))
                as i32,
        );
        let mut properties = DWM_THUMBNAIL_PROPERTIES {
            dwFlags: DWM_TNP_VISIBLE,
            fVisible: false.into(),
            ..Default::default()
        };
        if let Some((destination, source)) = clip(destination, source, viewport) {
            properties.dwFlags |= DWM_TNP_RECTDESTINATION
                | DWM_TNP_RECTSOURCE
                | DWM_TNP_OPACITY
                | DWM_TNP_SOURCECLIENTAREAONLY;
            properties.rcDestination = RECT {
                left: destination.x - viewport.x,
                top: destination.y - viewport.y,
                right: destination.right() - viewport.x,
                bottom: destination.bottom() - viewport.y,
            };
            properties.rcSource = RECT {
                left: source.x,
                top: source.y,
                right: source.right(),
                bottom: source.bottom(),
            };
            properties.opacity = 255;
            properties.fVisible = true.into();
            properties.fSourceClientAreaOnly = false.into();
        }
        unsafe { DwmUpdateThumbnailProperties(self.handle, &properties) }
    }
}
fn clip(destination: Rect, source: Rect, viewport: Rect) -> Option<(Rect, Rect)> {
    if destination.width <= 0 || destination.height <= 0 || source.width <= 0 || source.height <= 0
    {
        return None;
    }
    let shown = Rect::from_edges(
        destination.x.max(viewport.x),
        destination.y.max(viewport.y),
        destination.right().min(viewport.right()),
        destination.bottom().min(viewport.bottom()),
    );
    if shown.width <= 0 || shown.height <= 0 {
        return None;
    }
    let scale = |offset: i32, source: i32, destination: i32| {
        ((i64::from(offset) * i64::from(source)) / i64::from(destination)) as i32
    };
    let cropped = Rect::from_edges(
        source.x + scale(shown.x - destination.x, source.width, destination.width),
        source.y + scale(shown.y - destination.y, source.height, destination.height),
        source.x + scale(shown.right() - destination.x, source.width, destination.width),
        source.y + scale(shown.bottom() - destination.y, source.height, destination.height),
    );
    Some((shown, cropped))
}

struct PresentedPage {
    workspace: WorkspaceId,
    thumbnails: Vec<Thumbnail>,
    windows: Vec<PageWindow>,
}
impl PresentedPage {
    fn new(destination: HWND, workspace: WorkspaceId, windows: Vec<PageWindow>) -> Result<Self> {
        let thumbnails = windows
            .iter()
            .map(|page| Thumbnail::new(destination, page.window))
            .collect::<Result<_>>()?;
        Ok(Self { workspace, thumbnails, windows })
    }
}

struct Overlay {
    // Thumbnail relationships are released before their destination window.
    pages: Vec<PresentedPage>,
    window: OwnedWindow,
    spec: Transition,
    motion: WorkspaceMotion,
}
impl Overlay {
    fn new(input: &Rc<Input>, spec: Transition) -> Result<Self> {
        let window = create_window(input, Some(spec.viewport), spec.background_rgb)?;
        let pages = vec![
            PresentedPage::new(window.0, spec.from_workspace, spec.from.clone())?,
            PresentedPage::new(window.0, spec.to_workspace, spec.to.clone())?,
        ];
        let motion = WorkspaceMotion::new(spec.from_workspace, spec.to_workspace, spec.direction);
        Ok(Self { pages, window, spec, motion })
    }
    fn can_retarget(&self, spec: &Transition) -> bool {
        self.spec.viewport == spec.viewport
            && self.spec.background_rgb == spec.background_rgb
            && self.motion.target == spec.from_workspace
            && self
                .pages
                .iter()
                .find(|page| page.workspace == spec.from_workspace)
                .is_some_and(|page| page.windows == spec.from)
            && self
                .pages
                .iter()
                .find(|page| page.workspace == spec.to_workspace)
                .is_none_or(|page| page.windows == spec.to)
            && self.valid()
    }
    fn retarget(&mut self, spec: Transition, progress: f64) -> Result<()> {
        let motion = self.motion.retarget(spec.to_workspace, spec.direction, progress);
        let count: usize = motion
            .pages
            .iter()
            .map(|page| {
                self.pages
                    .iter()
                    .find(|old| old.workspace == page.workspace)
                    .map_or(spec.to.len(), |old| old.windows.len())
            })
            .sum();
        if count > 512 {
            return Err(windows::core::Error::from_hresult(E_INVALIDARG));
        }
        let mut previous = std::mem::take(&mut self.pages);
        previous.retain(|old| motion.pages.iter().any(|page| page.workspace == old.workspace));
        for page in &motion.pages {
            if let Some(index) = previous.iter().position(|old| old.workspace == page.workspace) {
                let retained = previous.remove(index);
                if page.workspace == spec.to_workspace && retained.windows != spec.to {
                    return Err(windows::core::Error::from_hresult(E_INVALIDARG));
                }
                self.pages.push(retained);
            } else {
                self.pages.push(PresentedPage::new(
                    self.window.0,
                    page.workspace,
                    spec.to.clone(),
                )?);
            }
        }
        self.spec = spec;
        self.motion = motion;
        Ok(())
    }
    fn draw(&self, progress: f64) -> Result<()> {
        for page in &self.pages {
            let Some(position) =
                self.motion.pages.iter().find(|position| position.workspace == page.workspace)
            else {
                return Err(windows::core::Error::from_hresult(E_UNEXPECTED));
            };
            let shift =
                (f64::from(self.spec.viewport.width) * position.position(progress)).round() as i32;
            for (thumbnail, window) in page.thumbnails.iter().zip(&page.windows) {
                let mut rect = window.rect;
                rect.x = rect.x.saturating_add(shift);
                thumbnail.draw(rect, self.spec.viewport)?;
            }
        }
        Ok(())
    }
    fn valid(&self) -> bool {
        self.pages.iter().flat_map(|page| &page.windows).all(|page| valid(page.window))
    }
}

pub struct Carousel {
    input: Rc<Input>,
    _wake: OwnedWindow,
    hook: Option<HHOOK>,
    overlays: Vec<Overlay>,
    started: Option<Instant>,
}
impl Carousel {
    pub fn new() -> Result<Self> {
        if INPUT.with(|slot| slot.borrow().strong_count() != 0) {
            return Err(windows::core::Error::from_hresult(E_UNEXPECTED));
        }
        let instance = unsafe { GetModuleHandleW(None)? };
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: w!("moon.carousel"),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0
            && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
        {
            return Err(windows::core::Error::from_thread());
        }
        let input = Rc::new(Input::default());
        let wake = create_window(&input, None, 0)?;
        input.wake.set(wake.0);
        INPUT.with(|slot| *slot.borrow_mut() = Rc::downgrade(&input));
        Ok(Self { input, _wake: wake, hook: None, overlays: Vec::new(), started: None })
    }
    pub fn is_active(&self) -> bool {
        !self.overlays.is_empty()
    }
    fn retire_hook(&mut self) {
        if !self.is_active()
            && self.input.swallowed.get() == 0
            && let Some(hook) = self.hook.take()
        {
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
        }
    }
    pub fn cancel(&mut self) {
        self.input.showing.set(false);
        self.overlays.clear();
        self.started = None;
        self.input.viewports.borrow_mut().clear();
        self.input.cancelled.set(false);
        self.retire_hook();
    }
    /// A secure-desktop/session boundary ends the old desktop's input sequence.
    /// Unlike ordinary cancellation, no matching release can be relied on here.
    pub fn suspend(&mut self) {
        self.input.swallowed.set(0);
        self.cancel();
    }
    pub fn prepare(&mut self, specs: &[Transition]) -> Result<bool> {
        self.retire_hook();
        if self.input.swallowed.get() != 0
            || specs.is_empty()
            || !motion_allowed()
            || mouse_button_held()
            || self.input.cancelled.get()
        {
            self.cancel();
            return Ok(false);
        }
        if specs.len() > 32
            || specs.iter().enumerate().any(|(index, spec)| {
                specs[..index].iter().any(|previous| previous.viewport == spec.viewport)
            })
            || specs.iter().any(|spec| {
                spec.from_workspace == spec.to_workspace
                    || spec.viewport.width <= 0
                    || spec.viewport.height <= 0
                    || spec.from.len() + spec.to.len() > 512
                    || spec.from.is_empty() && spec.to.is_empty()
                    || spec.from.iter().chain(&spec.to).any(|page| !valid(page.window))
            })
        {
            self.cancel();
            return Ok(false);
        }
        if self.is_active() {
            let progress = self.progress();
            let compatible = progress < 1.0
                && specs.len() == self.overlays.len()
                && specs
                    .iter()
                    .all(|spec| self.overlays.iter().any(|overlay| overlay.can_retarget(spec)));
            if compatible {
                let result = (|| {
                    for spec in specs {
                        let overlay = self
                            .overlays
                            .iter_mut()
                            .find(|overlay| overlay.can_retarget(spec))
                            .expect("compatible overlay was checked");
                        overlay.retarget(spec.clone(), progress)?;
                        overlay.draw(0.0)?;
                    }
                    // Retain the already visible popups and mouse ownership. Only
                    // their thumbnail positions and bounded page set change.
                    unsafe {
                        DwmFlush()?;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    self.cancel();
                    return Err(error);
                }
                self.started = None;
                return Ok(true);
            }
            self.cancel();
        }
        let mut origin = POINT::default();
        unsafe {
            GetCursorPos(&mut origin)?;
        }
        self.input.origin.set(origin);
        self.input.cancelled.set(false);
        let built: Result<Vec<_>> =
            specs.iter().map(|spec| Overlay::new(&self.input, spec.clone())).collect();
        let Ok(overlays) = built else {
            return Ok(false);
        };
        self.overlays = overlays;
        let result = (|| {
            for overlay in &self.overlays {
                overlay.draw(0.0)?;
            }
            let instance = unsafe { GetModuleHandleW(None)? };
            self.hook = Some(unsafe {
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(instance.into()), 0)?
            });
            // A down delivered before these overlays belong to its application.
            // Check again after preparation so its release cannot hit our popup.
            if mouse_button_held() {
                return Ok(false);
            }
            *self.input.viewports.borrow_mut() = specs.iter().map(|spec| spec.viewport).collect();
            self.input.showing.set(true);
            for overlay in &self.overlays {
                unsafe {
                    let _ = ShowWindow(overlay.window.0, SW_SHOWNOACTIVATE);
                    let _ = UpdateWindow(overlay.window.0);
                }
            }
            // The old page must be composed before native final rectangles change.
            unsafe {
                DwmFlush()?;
            }
            Ok(true)
        })();
        match result {
            Ok(true) => Ok(true),
            Ok(false) => {
                self.cancel();
                Ok(false)
            }
            Err(error) => {
                self.cancel();
                Err(error)
            }
        }
    }
    fn progress(&self) -> f64 {
        self.started.map_or(0.0, |time| time.elapsed().as_secs_f64() / DURATION.as_secs_f64())
    }
    pub fn animate(&mut self) -> Result<()> {
        if self.is_active() {
            self.started = Some(Instant::now());
            self.tick()?;
        }
        Ok(())
    }
    pub fn tick(&mut self) -> Result<()> {
        let allowed = !self.is_active() || motion_allowed();
        self.tick_with_motion_allowed(allowed)
    }
    fn tick_with_motion_allowed(&mut self, motion_allowed: bool) -> Result<()> {
        if !self.is_active() {
            self.retire_hook();
            return Ok(());
        }
        if self.input.cancelled.get()
            || !motion_allowed
            || self.overlays.iter().any(|overlay| !overlay.valid())
        {
            self.cancel();
            return Ok(());
        }
        let progress = self.progress();
        if progress >= 1.0 {
            self.cancel();
            return Ok(());
        }
        for overlay in &self.overlays {
            if let Err(error) = overlay.draw(progress) {
                self.cancel();
                return Err(error);
            }
        }
        Ok(())
    }
}
impl Drop for Carousel {
    fn drop(&mut self) {
        self.cancel();
        if let Some(hook) = self.hook.take() {
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
            }
        }
        INPUT.with(|slot| *slot.borrow_mut() = Weak::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hidden_owned_overlays_retire_after_cancel_preference_change_expiry_or_stale_source() {
        for reason in ["cancel", "monitor-removed", "reduced-motion", "expiry", "stale-source"] {
            let mut carousel = Carousel::new().unwrap();
            let spec = Transition {
                from_workspace: WorkspaceId(0),
                to_workspace: WorkspaceId(1),
                from: Vec::new(),
                to: Vec::new(),
                viewport: Rect::new(0, 0, 40, 40),
                direction: 1,
                background_rgb: 0,
            };
            // Hidden owned popups only: no input hooks, display changes or foreign windows.
            let overlay = Overlay::new(&carousel.input, spec.clone()).unwrap();
            let handle = overlay.window.0;
            carousel.overlays.push(overlay);
            carousel.started = Some(Instant::now());
            match reason {
                "cancel" => {
                    carousel.input.cancelled.set(true);
                    carousel.cancel();
                }
                "reduced-motion" => carousel.tick_with_motion_allowed(false).unwrap(),
                "monitor-removed" => assert!(!carousel.prepare(&[]).unwrap()),
                "expiry" => {
                    carousel.started = Some(Instant::now() - DURATION);
                    carousel.tick_with_motion_allowed(true).unwrap();
                }
                _ => {
                    carousel.overlays[0].pages[0].windows.push(PageWindow {
                        window: WindowRef { id: WindowId(0), process_id: u32::MAX },
                        rect: spec.viewport,
                    });
                    carousel.tick_with_motion_allowed(true).unwrap();
                }
            }
            assert!(!carousel.is_active(), "{reason}");
            assert!(!unsafe { IsWindow(Some(handle)) }.as_bool(), "{reason}");
            assert!(carousel.started.is_none());
            assert!(!carousel.input.cancelled.get());
            assert!(carousel.input.viewports.borrow().is_empty());
            // Cancellation must not poison the next presentation.
            let next = Overlay::new(&carousel.input, spec).unwrap();
            let next_handle = next.window.0;
            carousel.overlays.push(next);
            carousel.started = Some(Instant::now());
            carousel.tick_with_motion_allowed(true).unwrap();
            assert!(carousel.is_active());
            drop(carousel);
            assert!(!unsafe { IsWindow(Some(next_handle)) }.as_bool());
            assert!(INPUT.with(|slot| slot.borrow().upgrade().is_none()));
        }
    }

    #[test]
    fn translated_page_is_clipped_without_scaling_visible_content() {
        let viewport = Rect::new(-1920, 40, 1920, 1040);
        let destination = Rect::new(-2200, 60, 1800, 1000);
        let source = Rect::new(8, 8, 1800, 1000);
        let (shown, crop) = clip(destination, source, viewport).unwrap();
        assert_eq!(shown, Rect::new(-1920, 60, 1520, 1000));
        assert_eq!(crop, Rect::new(288, 8, 1520, 1000));
        assert!(clip(Rect::new(0, 0, 1920, 1000), source, viewport).is_none());
    }
}
