//! Input callbacks match commands and neutralize consumed Alt chords before release.
//! Physical key data is never sent to the main thread or retained in logs.

use crate::platform::{wide, window_id};
use moon_core::ids::WindowId;
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            LibraryLoader::GetModuleHandleW,
            Performance::HardwareCounterProfiling::{
                DisableThreadProfiling, EnableThreadProfiling, PERFORMANCE_DATA,
                ReadThreadProfilingData,
            },
            Power::{
                EFFECTIVE_POWER_MODE, EFFECTIVE_POWER_MODE_V2, EffectivePowerModeGameMode,
                PowerRegisterForEffectivePowerModeNotifications,
                PowerUnregisterFromEffectivePowerModeNotifications,
            },
            RemoteDesktop::*,
            Threading::{GetCurrentThread, GetCurrentThreadId},
        },
        UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{Error, PCWSTR, Result},
};

#[derive(Clone, Debug)]
pub struct HookBinding {
    pub chord: String,
    pub commands: Vec<String>,
}
#[derive(Clone, Debug)]
pub enum PlatformEvent {
    Window { kind: u32, id: WindowId },
    Commands { commands: Arc<[String]>, received: std::time::Instant, foreground: Option<WindowId> },
    EmergencyRestore,
    HookError(String),
    System(SystemEvent),
    PointerMoved,
}

#[derive(Clone, Copy, Debug)]
pub enum SystemEvent {
    DisplayChanged,
    SettingsChanged,
    SessionLocked,
    SessionUnlocked,
    Suspended,
    Resumed,
    DesktopChanged,
    ShellRestarted,
    SessionEnding,
    GamePower(bool),
}

#[derive(Clone)]
struct Binding {
    key: u32,
    modifiers: u8,
    commands: Arc<[String]>,
}
/// Published by the daemon after classifying the actual foreground. A newly
/// focused window owns its first key while that classification is pending.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameInput {
    pub foreground: Option<WindowId>,
    pub desktop: Option<(WindowId, u32)>,
    pub playing: bool,
    pub session: bool,
    pub release_emergency: bool,
}
impl GameInput {
    pub fn new(
        foreground: Option<WindowId>,
        desktop: Option<(WindowId, u32)>,
        games: &moon_core::gaming::Games,
        pass_every_key: bool,
    ) -> Self {
        let playing = games.foreground.is_some();
        Self {
            foreground,
            desktop: desktop.filter(|(id, _)| !games.sessions.contains_key(id)),
            playing,
            session: !games.sessions.is_empty(),
            release_emergency: playing && pass_every_key,
        }
    }
    fn forwards(self, foreground: Option<WindowId>, key: u32, modifiers: u8) -> bool {
        if self.session && (moon_core::config::KeyChord { key, modifiers }).is_task_switch() {
            return true;
        }
        if foreground == self.foreground {
            return self.playing;
        }
        // The switcher can hand input back before its WinEvent is delivered.
        // Only the previously classified desktop HWND and owner may use Moon
        // bindings in that interval. All unknown foregrounds own their keys.
        !self.session
            || !self.desktop.is_some_and(|(id, pid)| {
                foreground == Some(id) && crate::platform::process_id(id) == pid
            })
    }
}
type SharedGameInput = Arc<RwLock<Option<GameInput>>>;

struct State {
    game: SharedGameInput,
    bindings: Vec<Binding>,
    sender: Sender<PlatformEvent>,
    pressed: [bool; 256],
    swallowed: HashSet<u32>,
    taskbar_message: u32,
    pointer: Arc<PointerState>,
}
thread_local! { static STATE:RefCell<Option<State>> = const { RefCell::new(None) }; }
const UPDATE: u32 = WM_APP + 41;
const RENEW: u32 = WM_APP + 42;
const POINTER: u32 = WM_APP + 43;
const GAME_HOTKEY: u32 = WM_APP + 44;
const GAME_POWER: u32 = WM_APP + 45;

unsafe extern "system" fn power_changed(
    mode: EFFECTIVE_POWER_MODE,
    context: *const core::ffi::c_void,
) {
    // Only an integer thread ID crosses the callback lifetime boundary.
    let _ = unsafe {
        PostThreadMessageW(
            context as usize as u32,
            GAME_POWER,
            WPARAM(usize::from(mode == EffectivePowerModeGameMode)),
            LPARAM(0),
        )
    };
}

fn register_emergency() -> Result<()> {
    unsafe {
        RegisterHotKey(
            None,
            EMERGENCY,
            MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
            VK_F12.0 as u32,
        )
    }
}

// Documented WM_WTSSESSION_CHANGE value, absent from windows 0.62.2 metadata.
const SESSION_DESKTOP_READY: u32 = 0x0f;
#[derive(Default)]
struct PointerState {
    position: AtomicU64,
    pending: AtomicBool,
}
const EMERGENCY: i32 = 0x4d01;
const MASK_TAG: usize = 0x4d4f4f4e;
// Microsoft documents 0xE8 as unassigned. A key-up generates no character.
const ALT_MASK: VIRTUAL_KEY = VIRTUAL_KEY(0xe8);
#[cfg(test)]
static TEST_RENEWALS: AtomicU64 = AtomicU64::new(0);

fn is_internal_mask(info: &KBDLLHOOKSTRUCT) -> bool {
    info.dwExtraInfo == MASK_TAG
        && info.vkCode == u32::from(ALT_MASK.0)
        && info.scanCode == 0
        && info.flags.contains(LLKHF_INJECTED | LLKHF_UP)
        && !info.flags.contains(LLKHF_LOWER_IL_INJECTED)
}

fn keyboard_snapshot(mut down: impl FnMut(VIRTUAL_KEY) -> bool) -> [bool; 256] {
    let mut pressed = [false; 256];
    for (key, value) in pressed.iter_mut().enumerate() {
        // Generic modifier aliases must never form independent sticky latches.
        // Other held keys belong to the application until their next release.
        if !matches!(key, 0x10..=0x12) {
            *value = down(VIRTUAL_KEY(key as u16));
        }
    }
    pressed
}

fn modifier_mask(pressed: &[bool; 256]) -> u8 {
    let down = |key: VIRTUAL_KEY| pressed[key.0 as usize];
    u8::from(down(VK_LMENU) || down(VK_RMENU))
        | (u8::from(down(VK_LSHIFT) || down(VK_RSHIFT)) << 1)
        | (u8::from(down(VK_LCONTROL) || down(VK_RCONTROL)) << 2)
        | (u8::from(down(VK_LWIN) || down(VK_RWIN)) << 3)
}

fn record_key(pressed: &mut [bool; 256], info: &KBDLLHOOKSTRUCT, up: bool) -> (u32, bool) {
    let key = if matches!(info.vkCode, 0x10..=0x12) {
        // Keep only a sided latch. Right Ctrl/Alt are extended keys even when
        // the current input layout maps their scan codes to Hangul/Hanja.
        let (left, right) = match info.vkCode {
            0x10 => (VK_LSHIFT, VK_RSHIFT),
            0x11 => (VK_LCONTROL, VK_RCONTROL),
            _ => (VK_LMENU, VK_RMENU),
        };
        let is_right = if info.vkCode == u32::from(VK_SHIFT.0) {
            (unsafe { MapVirtualKeyW(info.scanCode, MAPVK_VSC_TO_VK_EX) }) == u32::from(VK_RSHIFT.0)
        } else {
            info.flags.contains(LLKHF_EXTENDED)
        };
        u32::from(if is_right { right.0 } else { left.0 })
    } else {
        info.vkCode
    };
    let was_pressed = pressed[key as usize];
    pressed[key as usize] = !up;
    (key, was_pressed)
}

const LATENCY_BUCKETS: usize = 4097;
static LATENCY: [std::sync::atomic::AtomicU64; LATENCY_BUCKETS] =
    [const { std::sync::atomic::AtomicU64::new(0) }; LATENCY_BUCKETS];
static LOCAL_LATENCY: [std::sync::atomic::AtomicU64; LATENCY_BUCKETS] =
    [const { std::sync::atomic::AtomicU64::new(0) }; LATENCY_BUCKETS];
static LOCAL_MAX_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static MAX_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// Fixed storage; callbacks record only elapsed time, never input identities.
// Phase timings are diagnostic and remain included in the existing totals.
struct PhaseLatency {
    buckets: [AtomicU64; LATENCY_BUCKETS],
    max_ns: AtomicU64,
}
impl PhaseLatency {
    const fn new() -> Self {
        Self { buckets: [const { AtomicU64::new(0) }; LATENCY_BUCKETS], max_ns: AtomicU64::new(0) }
    }
    fn record(&self, elapsed: std::time::Duration) {
        let ns = elapsed.as_nanos().min(u64::MAX as u128) as u64;
        self.buckets[(ns.div_ceil(1000) as usize).min(LATENCY_BUCKETS - 1)]
            .fetch_add(1, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
    }
    fn measure(&self) -> PhaseMeasure<'_> {
        PhaseMeasure { latency: self, started: std::time::Instant::now() }
    }
    fn stats(&self) -> PhaseStats {
        let counts = self.buckets.each_ref().map(|counter| counter.load(Ordering::Relaxed));
        let samples: u64 = counts.iter().sum();
        let percentile = |percent: u64| {
            let target = (samples * percent).div_ceil(100);
            let mut sum = 0;
            counts
                .iter()
                .position(|count| {
                    sum += count;
                    sum >= target
                })
                .unwrap_or(LATENCY_BUCKETS - 1) as u64
        };
        PhaseStats {
            samples,
            p50_us: percentile(50),
            p99_us: percentile(99),
            max_us: self.max_ns.load(Ordering::Relaxed).div_ceil(1000),
        }
    }
}
struct PhaseMeasure<'a> {
    latency: &'a PhaseLatency,
    started: std::time::Instant,
}
impl Drop for PhaseMeasure<'_> {
    fn drop(&mut self) {
        self.latency.record(self.started.elapsed());
    }
}
static MATCHING_LATENCY: PhaseLatency = PhaseLatency::new();
static MASK_LATENCY: PhaseLatency = PhaseLatency::new();
static ENQUEUE_LATENCY: PhaseLatency = PhaseLatency::new();

#[derive(Clone, Copy)]
struct DispatchSnapshot {
    switches: u32,
    wait_reasons: u64,
}
#[derive(Default)]
struct InputProfileCounters {
    requested: AtomicBool,
    available: AtomicBool,
    enable_error: AtomicU64,
    disable_error: AtomicU64,
    successful_samples: AtomicU64,
    read_failures: AtomicU64,
    last_read_error: AtomicU64,
    nested_calls_skipped: AtomicU64,
    calls_with_switches: AtomicU64,
    total_switches: AtomicU64,
    max_switches: AtomicU64,
    wait_reason_bitmap: AtomicU64,
}
impl InputProfileCounters {
    fn record(&self, before: DispatchSnapshot, after: DispatchSnapshot) {
        let switches = u64::from(after.switches.wrapping_sub(before.switches));
        self.successful_samples.fetch_add(1, Ordering::Relaxed);
        self.calls_with_switches.fetch_add(u64::from(switches != 0), Ordering::Relaxed);
        self.total_switches.fetch_add(switches, Ordering::Relaxed);
        self.max_switches.fetch_max(switches, Ordering::Relaxed);
        // Unlike the cumulative switch count, this bitmap covers only the
        // interval since the previous read. Never subtract the prior bitmap.
        self.wait_reason_bitmap.fetch_or(after.wait_reasons, Ordering::Relaxed);
    }
    fn stats(&self) -> InputProfileStats {
        InputProfileStats {
            requested: self.requested.load(Ordering::Relaxed),
            available: self.available.load(Ordering::Relaxed),
            enable_error: self.enable_error.load(Ordering::Relaxed) as u32,
            disable_error: self.disable_error.load(Ordering::Relaxed) as u32,
            successful_samples: self.successful_samples.load(Ordering::Relaxed),
            read_failures: self.read_failures.load(Ordering::Relaxed),
            last_read_error: self.last_read_error.load(Ordering::Relaxed) as u32,
            nested_calls_skipped: self.nested_calls_skipped.load(Ordering::Relaxed),
            calls_with_switches: self.calls_with_switches.load(Ordering::Relaxed),
            total_switches: self.total_switches.load(Ordering::Relaxed),
            max_switches: self.max_switches.load(Ordering::Relaxed),
            wait_reason_bitmap: self.wait_reason_bitmap.load(Ordering::Relaxed),
        }
    }
}
static INPUT_PROFILE: std::sync::LazyLock<InputProfileCounters> =
    std::sync::LazyLock::new(InputProfileCounters::default);
thread_local! {
    static INPUT_PROFILE_HANDLE: Cell<Option<HANDLE>> = const { Cell::new(None) };
}

struct InputProfiler(HANDLE);
impl InputProfiler {
    fn start() -> Option<Self> {
        if std::env::var("MOON_PROFILE_INPUT").as_deref() != Ok("1") {
            return None;
        }
        INPUT_PROFILE.requested.store(true, Ordering::Relaxed);
        let mut handle = HANDLE::default();
        // THREAD_PROFILING_FLAG_DISPATCH = 1. HardwareCounters = 0: no hardware
        // profiling, driver configuration, or global trace session is requested.
        let error = unsafe { EnableThreadProfiling(GetCurrentThread(), 1, 0, &mut handle) };
        INPUT_PROFILE.enable_error.store(u64::from(error), Ordering::Relaxed);
        if error != 0 {
            return None;
        }
        INPUT_PROFILE.available.store(true, Ordering::Relaxed);
        INPUT_PROFILE_HANDLE.with(|cell| cell.set(Some(handle)));
        Some(Self(handle))
    }
}
impl Drop for InputProfiler {
    fn drop(&mut self) {
        INPUT_PROFILE_HANDLE.with(|cell| cell.set(None));
        // This guard is created and dropped inside the owned hook thread,
        // including every startup failure path, as required by this API.
        let error = unsafe { DisableThreadProfiling(self.0) };
        INPUT_PROFILE.disable_error.store(u64::from(error), Ordering::Relaxed);
    }
}
fn read_input_profile(handle: HANDLE) -> Option<DispatchSnapshot> {
    let mut data = PERFORMANCE_DATA {
        Size: std::mem::size_of::<PERFORMANCE_DATA>() as u16,
        Version: 1, // PERFORMANCE_DATA_VERSION
        ..Default::default()
    };
    // READ_THREAD_PROFILING_FLAG_DISPATCHING = 1. CycleTime is intentionally
    // ignored: CPU cycles cannot be converted portably into elapsed time.
    let error = unsafe { ReadThreadProfilingData(handle, 1, &mut data) };
    if error != 0 {
        INPUT_PROFILE.read_failures.fetch_add(1, Ordering::Relaxed);
        INPUT_PROFILE.last_read_error.store(u64::from(error), Ordering::Relaxed);
        None
    } else {
        Some(DispatchSnapshot {
            switches: data.ContextSwitchCount,
            wait_reasons: data.WaitReasonBitMap,
        })
    }
}
fn input_profile_before_mask() -> Option<(HANDLE, DispatchSnapshot)> {
    let handle = INPUT_PROFILE_HANDLE.with(Cell::get)?;
    if MASK_CALL.with(|cell| cell.get().is_some()) {
        // A nested read would reset the outer sample's wait-reason bitmap.
        INPUT_PROFILE.nested_calls_skipped.fetch_add(1, Ordering::Relaxed);
        return None;
    }
    read_input_profile(handle).map(|snapshot| (handle, snapshot))
}

#[derive(Clone, Copy)]
struct MaskCallTiming {
    started: std::time::Instant,
    callbacks: u64,
    callback: Option<(std::time::Instant, std::time::Instant)>,
}
impl MaskCallTiming {
    fn observe(&mut self, entered: std::time::Instant, returning: std::time::Instant) {
        self.callbacks = self.callbacks.saturating_add(1);
        if self.callbacks == 1 {
            self.callback = Some((entered, returning));
        }
    }
}
thread_local! {
    static MASK_CALL: Cell<Option<MaskCallTiming>> = const { Cell::new(None) };
}
struct MaskDispatchLatency {
    send_input: PhaseLatency,
    before_callback: PhaseLatency,
    callback_handler: PhaseLatency,
    after_callback: PhaseLatency,
    calls_without_callback: AtomicU64,
    calls_with_multiple_callbacks: AtomicU64,
    callbacks_outside_call: AtomicU64,
}
impl MaskDispatchLatency {
    const fn new() -> Self {
        Self {
            send_input: PhaseLatency::new(),
            before_callback: PhaseLatency::new(),
            callback_handler: PhaseLatency::new(),
            after_callback: PhaseLatency::new(),
            calls_without_callback: AtomicU64::new(0),
            calls_with_multiple_callbacks: AtomicU64::new(0),
            callbacks_outside_call: AtomicU64::new(0),
        }
    }
    fn record(&self, call: MaskCallTiming, returned: std::time::Instant) {
        self.send_input.record(returned.duration_since(call.started));
        match (call.callbacks, call.callback) {
            (1, Some((entered, returning))) => {
                self.before_callback.record(entered.duration_since(call.started));
                self.callback_handler.record(returning.duration_since(entered));
                self.after_callback.record(returned.duration_since(returning));
            }
            (0, _) => {
                self.calls_without_callback.fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.calls_with_multiple_callbacks.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn stats(&self) -> MaskDispatchStats {
        MaskDispatchStats {
            send_input: self.send_input.stats(),
            before_callback: self.before_callback.stats(),
            callback_handler: self.callback_handler.stats(),
            after_callback: self.after_callback.stats(),
            calls_without_callback: self.calls_without_callback.load(Ordering::Relaxed),
            calls_with_multiple_callbacks: self
                .calls_with_multiple_callbacks
                .load(Ordering::Relaxed),
            callbacks_outside_call: self.callbacks_outside_call.load(Ordering::Relaxed),
        }
    }
}
static MASK_DISPATCH_LATENCY: MaskDispatchLatency = MaskDispatchLatency::new();

fn observe_mask_callback(entered: std::time::Instant) {
    // This boundary precedes diagnostic bookkeeping and the existing callback
    // timing destructor; their cost consequently belongs to after_callback.
    let returning = std::time::Instant::now();
    MASK_CALL.with(|cell| {
        if let Some(mut call) = cell.get() {
            call.observe(entered, returning);
            cell.set(Some(call));
        } else {
            MASK_DISPATCH_LATENCY.callbacks_outside_call.fetch_add(1, Ordering::Relaxed);
        }
    });
}

#[derive(serde::Serialize)]
pub struct PhaseStats {
    pub samples: u64,
    /// Rounded up to microseconds; 4096 is the terminal bucket's lower bound.
    pub p50_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}
#[derive(serde::Serialize)]
pub struct MaskDispatchStats {
    /// Every SendInput attempt, including failures and nonreentrant delivery.
    /// Starts immediately before arming thread-local timing; excludes recording
    /// its histograms after return. The existing alt_mask includes that overhead.
    pub send_input: PhaseStats,
    /// These three populations include only calls with exactly one observed
    /// tagged callback. Their percentiles must not be added or subtracted.
    pub before_callback: PhaseStats,
    /// Callback entry through mask recognition, before diagnostic bookkeeping.
    pub callback_handler: PhaseStats,
    /// Includes callback metric bookkeeping and Windows call completion.
    pub after_callback: PhaseStats,
    pub calls_without_callback: u64,
    pub calls_with_multiple_callbacks: u64,
    /// A tagged callback with no active call cannot be causally paired here.
    pub callbacks_outside_call: u64,
}
#[derive(serde::Serialize)]
pub struct InputProfileStats {
    pub requested: bool,
    /// Enable succeeded on the owned hook thread; zero error with requested=false
    /// means disabled, not tested or supported. All counters are cumulative.
    pub available: bool,
    pub enable_error: u32,
    pub disable_error: u32,
    pub successful_samples: u64,
    pub read_failures: u64,
    pub last_read_error: u32,
    pub nested_calls_skipped: u64,
    pub calls_with_switches: u64,
    pub total_switches: u64,
    pub max_switches: u64,
    /// Union of reasons observed between paired reads, not reason frequencies.
    pub wait_reason_bitmap: u64,
}
#[derive(serde::Serialize)]
pub struct HookPhases {
    /// State lookup, key ownership and binding match, excluding internal masks.
    pub matching: PhaseStats,
    /// The complete synchronous SendInput call, including reentrant dispatch.
    pub alt_mask: PhaseStats,
    pub alt_mask_dispatch: MaskDispatchStats,
    /// Opt-in dispatch reads bracket SendInput. Existing alt_mask and own/whole
    /// callback timing boundaries remain unchanged and include profiling cost.
    pub input_profile: InputProfileStats,
    /// Event enqueue and consumed-key bookkeeping, including failure reporting.
    pub enqueue: PhaseStats,
}
#[derive(serde::Serialize)]
pub struct HookStats {
    pub callbacks: u64,
    pub p99_us: u64,
    pub max_us: u64,
    /// Matching, enqueueing and ordered Alt masking, excluding CallNextHookEx.
    pub own_p99_us: u64,
    pub own_max_us: u64,
    pub phases: HookPhases,
}
pub fn stats() -> HookStats {
    use std::sync::atomic::Ordering::Relaxed;
    let counts = LATENCY.each_ref().map(|counter| counter.load(Relaxed));
    let local_counts = LOCAL_LATENCY.each_ref().map(|counter| counter.load(Relaxed));
    let callbacks: u64 = counts.iter().sum();
    let target = (callbacks * 99).div_ceil(100);
    let mut sum = 0;
    let p99_us = counts
        .iter()
        .position(|count| {
            sum += count;
            sum >= target
        })
        .unwrap_or(LATENCY_BUCKETS - 1) as u64;
    let mut local_sum = 0;
    let own_p99_us = local_counts
        .iter()
        .position(|count| {
            local_sum += count;
            local_sum >= target
        })
        .unwrap_or(LATENCY_BUCKETS - 1) as u64;
    HookStats {
        callbacks,
        p99_us,
        max_us: MAX_NS.load(Relaxed).div_ceil(1000),
        own_p99_us,
        own_max_us: LOCAL_MAX_NS.load(Relaxed).div_ceil(1000),
        phases: HookPhases {
            matching: MATCHING_LATENCY.stats(),
            alt_mask: MASK_LATENCY.stats(),
            alt_mask_dispatch: MASK_DISPATCH_LATENCY.stats(),
            input_profile: INPUT_PROFILE.stats(),
            enqueue: ENQUEUE_LATENCY.stats(),
        },
    }
}

fn compile(bindings: Vec<HookBinding>) -> Result<Vec<Binding>> {
    bindings
        .into_iter()
        .filter(|binding| !binding.commands.is_empty())
        .map(|binding| {
            let chord = moon_core::config::parse_chord(&binding.chord)
                .map_err(|error| Error::new(E_INVALIDARG, error))?;
            Ok(Binding {
                key: chord.key,
                modifiers: chord.modifiers,
                commands: binding.commands.into(),
            })
        })
        .collect()
}

unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    struct Measure {
        start: std::time::Instant,
        local_recorded: bool,
    }
    impl Measure {
        fn record_local(&mut self) {
            use std::sync::atomic::Ordering::Relaxed;
            if !self.local_recorded {
                let ns = self.start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                LOCAL_LATENCY[(ns.div_ceil(1000) as usize).min(LATENCY_BUCKETS - 1)]
                    .fetch_add(1, Relaxed);
                LOCAL_MAX_NS.fetch_max(ns, Relaxed);
                self.local_recorded = true;
            }
        }
    }
    impl Drop for Measure {
        fn drop(&mut self) {
            use std::sync::atomic::Ordering::Relaxed;
            self.record_local();
            let ns = self.start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            LATENCY[(ns.div_ceil(1000) as usize).min(LATENCY_BUCKETS - 1)].fetch_add(1, Relaxed);
            MAX_NS.fetch_max(ns, Relaxed);
        }
    }
    let mut measure = Measure { start: std::time::Instant::now(), local_recorded: false };
    if code >= 0 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if is_internal_mask(info) {
            // Deliver this internal no-character event to Windows without
            // invoking other remappers. Every other event follows the chain.
            observe_mask_callback(measure.start);
            return LRESULT(0);
        }
        if !info.flags.contains(LLKHF_LOWER_IL_INJECTED) {
            enum Match {
                Forward,
                Swallow,
                Command {
                    key: u32,
                    alt: bool,
                    was_swallowed: bool,
                    commands: Arc<[String]>,
                    sender: Sender<PlatformEvent>,
                    received: std::time::Instant,
                    foreground: Option<WindowId>,
                },
            }
            let matching_started = std::time::Instant::now();
            let matched = STATE.with(|cell| {
                let mut value = cell.borrow_mut();
                let Some(state) = value.as_mut() else {
                    return Match::Forward;
                };
                if info.vkCode >= 256 {
                    return Match::Forward;
                }
                let up = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
                let (key, was_pressed) = record_key(&mut state.pressed, info, up);
                if up {
                    return if state.swallowed.remove(&key) {
                        Match::Swallow
                    } else {
                        Match::Forward
                    };
                }
                let was_swallowed = state.swallowed.contains(&key);
                if was_pressed && !was_swallowed {
                    // The initial down reached the app, so repeats and up must
                    // keep doing so even when modifiers change mid-press.
                    return Match::Forward;
                }
                let modifiers = modifier_mask(&state.pressed);
                if let Some(binding) = state
                    .bindings
                    .iter()
                    .find(|binding| binding.key == key && binding.modifiers == modifiers)
                {
                    let foreground = crate::platform::foreground();
                    let forwards = match state.game.try_read() {
                        Ok(game) => {
                            game.is_some_and(|policy| policy.forwards(foreground, key, modifiers))
                        }
                        Err(_) => true,
                    };
                    if forwards {
                        return if was_swallowed { Match::Swallow } else { Match::Forward };
                    }
                    Match::Command {
                        key,
                        alt: modifiers & 1 != 0,
                        was_swallowed,
                        commands: binding.commands.clone(),
                        sender: state.sender.clone(),
                        received: std::time::Instant::now(),
                        foreground,
                    }
                } else if was_swallowed {
                    Match::Swallow
                } else {
                    Match::Forward
                }
            });
            MATCHING_LATENCY.record(matching_started.elapsed());
            match matched {
                Match::Swallow => return LRESULT(1),
                Match::Command {
                    key,
                    alt,
                    was_swallowed,
                    commands,
                    sender,
                    received,
                    foreground,
                } => {
                    // SendInput can reenter the callback. No State borrow is
                    // held, and masking precedes both modifier release and any
                    // focus change caused by the queued command.
                    let masked = if alt {
                        let _phase = MASK_LATENCY.measure();
                        mask_alt_chord()
                    } else {
                        true
                    };
                    let _phase = ENQUEUE_LATENCY.measure();
                    if !masked {
                        let _ = sender.send(PlatformEvent::HookError(
                            "Windows refused the internal Alt mask; shortcut was not executed"
                                .into(),
                        ));
                    } else if sender
                        .send(PlatformEvent::Commands { commands, received, foreground })
                        .is_ok()
                    {
                        STATE.with(|cell| {
                            if let Some(state) = cell.borrow_mut().as_mut() {
                                state.swallowed.insert(key);
                            }
                        });
                        return LRESULT(1);
                    }
                    // A repeat of an already consumed down must not leak to an
                    // app whose corresponding up will still be consumed.
                    if was_swallowed {
                        return LRESULT(1);
                    }
                }
                Match::Forward => {}
            }
        }
    }
    measure.record_local();
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEMOVE {
        let input = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if input.flags & LLMHF_LOWER_IL_INJECTED == 0 {
            STATE.with(|state| {
                if let Some(state) = state.borrow().as_ref() {
                    let packed = ((input.pt.x as u32 as u64) << 32) | (input.pt.y as u32 as u64);
                    state.pointer.position.store(packed, Ordering::Release);
                    if !state.pointer.pending.swap(true, Ordering::AcqRel)
                        && state.sender.send(PlatformEvent::PointerMoved).is_err()
                    {
                        state.pointer.pending.store(false, Ordering::Release);
                    }
                }
            });
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn win_event(
    _: HWINEVENTHOOK,
    event: u32,
    handle: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if event == EVENT_SYSTEM_DESKTOPSWITCH {
        send_system(SystemEvent::DesktopChanged);
        return;
    }
    if handle.is_invalid() || (object != 0 && object != OBJID_WINDOW.0) || child != 0 {
        return;
    }
    STATE.with(|cell| {
        if let Some(state) = cell.borrow().as_ref() {
            let _ = state.sender.send(PlatformEvent::Window { kind: event, id: window_id(handle) });
        }
    });
}

fn send_system(event: SystemEvent) {
    STATE.with(|state| {
        if let Some(state) = state.borrow().as_ref() {
            let _ = state.sender.send(PlatformEvent::System(event));
        }
    });
}

unsafe extern "system" fn notifications(
    handle: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let taskbar = STATE.with(|state| {
        state
            .borrow()
            .as_ref()
            .is_some_and(|state| state.taskbar_message != 0 && state.taskbar_message == message)
    });
    let event = if taskbar {
        Some(SystemEvent::ShellRestarted)
    } else {
        match message {
            WM_DISPLAYCHANGE => Some(SystemEvent::DisplayChanged),
            WM_SETTINGCHANGE => Some(SystemEvent::SettingsChanged),
            WM_POWERBROADCAST => match wparam.0 as u32 {
                PBT_APMSUSPEND => Some(SystemEvent::Suspended),
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => Some(SystemEvent::Resumed),
                _ => None,
            },
            WM_WTSSESSION_CHANGE => match wparam.0 as u32 {
                WTS_SESSION_LOCK => Some(SystemEvent::SessionLocked),
                WTS_SESSION_UNLOCK => Some(SystemEvent::SessionUnlocked),
                WTS_CONSOLE_DISCONNECT | WTS_REMOTE_DISCONNECT => Some(SystemEvent::Suspended),
                WTS_CONSOLE_CONNECT
                | WTS_REMOTE_CONNECT
                | WTS_SESSION_LOGON
                | SESSION_DESKTOP_READY => Some(SystemEvent::Resumed),
                WTS_SESSION_LOGOFF => Some(SystemEvent::SessionEnding),
                _ => None,
            },
            WM_ENDSESSION if wparam.0 != 0 => Some(SystemEvent::SessionEnding),
            _ => None,
        }
    };
    if let Some(event) = event {
        send_system(event);
        if matches!(event, SystemEvent::Resumed | SystemEvent::SessionUnlocked) {
            let _ =
                unsafe { PostThreadMessageW(GetCurrentThreadId(), RENEW, WPARAM(0), LPARAM(0)) };
        }
        return LRESULT(1);
    }
    unsafe { DefWindowProcW(handle, message, wparam, lparam) }
}

struct NotificationWindow {
    handle: HWND,
    session_registered: bool,
}
impl NotificationWindow {
    fn new() -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let name = wide("MoonSystemNotifications");
            let class = WNDCLASSW {
                lpfnWndProc: Some(notifications),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(name.as_ptr()),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err(Error::from_thread());
            }
            let handle = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                PCWSTR(name.as_ptr()),
                PCWSTR(name.as_ptr()),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            // The WTS service can still be starting during logon. Keep window,
            // display and power intake live and retry session registration later.
            let session_registered =
                WTSRegisterSessionNotification(handle, NOTIFY_FOR_THIS_SESSION).is_ok();
            Ok(Self { handle, session_registered })
        }
    }
}
impl NotificationWindow {
    fn retry_session_registration(&mut self) {
        retry_session_registration(&mut self.session_registered, || unsafe {
            WTSRegisterSessionNotification(self.handle, NOTIFY_FOR_THIS_SESSION).is_ok()
        });
    }
}

fn retry_session_registration(registered: &mut bool, register: impl FnOnce() -> bool) {
    if !*registered && register() {
        *registered = true;
        // Lock/unlock events may have occurred before registration recovered.
        // Resumed requests a fresh fail-closed query; it does not imply unlocked.
        send_system(SystemEvent::Resumed);
    }
}

impl Drop for NotificationWindow {
    fn drop(&mut self) {
        unsafe {
            if self.session_registered {
                let _ = WTSUnRegisterSessionNotification(self.handle);
            }
            let _ = DestroyWindow(self.handle);
        }
    }
}

pub struct Hooks {
    id: u32,
    game: SharedGameInput,
    pending: Arc<Mutex<Option<Vec<Binding>>>>,
    pointer: Arc<PointerState>,
    thread: Option<JoinHandle<()>>,
}
impl Hooks {
    pub fn start(
        bindings: Vec<HookBinding>,
        sender: Sender<PlatformEvent>,
        keyboard_enabled: bool,
    ) -> Result<Self> {
        Self::with_input(bindings, sender, keyboard_enabled, None)
    }

    pub fn with_input(
        bindings: Vec<HookBinding>,
        sender: Sender<PlatformEvent>,
        keyboard_enabled: bool,
        initial: Option<GameInput>,
    ) -> Result<Self> {
        let bindings = compile(bindings)?;
        let pending: Arc<Mutex<Option<Vec<Binding>>>> = Arc::new(Mutex::new(None));
        let changes = pending.clone();
        let pointer = Arc::new(PointerState::default());
        let pointer_thread = pointer.clone();
        let game = Arc::new(RwLock::new(initial));
        let hook_game = game.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("moon-intake".into())
            .spawn(move || unsafe {
                let _input_profile = if keyboard_enabled { InputProfiler::start() } else { None };
                let id = GetCurrentThreadId();
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                STATE.with(|state| {
                    *state.borrow_mut() = Some(State {
                        bindings,
                        game: hook_game.clone(),
                        sender: sender.clone(),
                        pressed: keyboard_snapshot(|key| GetAsyncKeyState(key.0 as i32) < 0),
                        swallowed: HashSet::with_capacity(16),
                        pointer: pointer_thread,
                        taskbar_message: RegisterWindowMessageW(PCWSTR(
                            wide("TaskbarCreated").as_ptr(),
                        )),
                    })
                });
                let mut notifications = match NotificationWindow::new() {
                    Ok(window) => window,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                let mut event_hooks = Vec::new();
                for (first, last) in [
                    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                    (EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_DESKTOPSWITCH),
                    (EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND),
                    (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
                    (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
                    (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE),
                    (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
                ] {
                    let hook = SetWinEventHook(
                        first,
                        last,
                        None,
                        Some(win_event),
                        0,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    );
                    if hook.is_invalid() {
                        let _ = ready_tx.send(Err(Error::from_thread()));
                        return;
                    }
                    event_hooks.push(hook);
                }
                let mut hook = if keyboard_enabled {
                    match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0) {
                        Ok(value) => Some(value),
                        Err(error) => {
                            let _ = ready_tx.send(Err(error));
                            return;
                        }
                    }
                } else {
                    None
                };
                if let Err(error) = register_emergency() {
                    for hook in event_hooks {
                        let _ = UnhookWinEvent(hook);
                    }
                    if let Some(hook) = hook {
                        let _ = UnhookWindowsHookEx(hook);
                    }
                    let _ = ready_tx.send(Err(error));
                    return;
                }
                // Windows exposes no hook-is-installed API. Renew it periodically so a
                // timeout or secure-desktop transition cannot disable bindings forever.
                let timer = SetTimer(None, 0, 30_000, None);
                let mut emergency_registered = true;
                let mut power_handle = std::ptr::null_mut();
                let power_registered = PowerRegisterForEffectivePowerModeNotifications(
                    EFFECTIVE_POWER_MODE_V2,
                    Some(power_changed),
                    Some(id as usize as *const _),
                    &mut power_handle,
                )
                .is_ok();
                let mut mouse_enabled = false;
                let mut mouse_hook = None;
                let _ = ready_tx.send(Ok(id));
                while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                    if message.message == UPDATE {
                        if let Some(bindings) =
                            changes.lock().unwrap_or_else(|e| e.into_inner()).take()
                        {
                            STATE.with(|state| {
                                if let Some(state) = state.borrow_mut().as_mut() {
                                    state.bindings = bindings;
                                }
                            });
                        }
                        continue;
                    } else if message.message == GAME_POWER {
                        let _ = sender.send(PlatformEvent::System(SystemEvent::GamePower(
                            message.wParam.0 != 0,
                        )));
                        continue;
                    }
                    if matches!(message.message, GAME_HOTKEY | WM_TIMER) {
                        let release = hook_game
                            .read()
                            .unwrap_or_else(|e| e.into_inner())
                            .is_some_and(|p| p.release_emergency);
                        if release && emergency_registered {
                            if UnregisterHotKey(None, EMERGENCY).is_ok() {
                                emergency_registered = false;
                            }
                        } else if !release && !emergency_registered {
                            match register_emergency() {
                                Ok(()) => emergency_registered = true,
                                Err(error) => {
                                    let _ = sender.send(PlatformEvent::HookError(format!(
                                        "Emergency restore hotkey is unavailable: {error}"
                                    )));
                                }
                            }
                        }
                        if message.message == GAME_HOTKEY {
                            continue;
                        }
                    }
                    if message.message == POINTER {
                        mouse_enabled = message.wParam.0 != 0;
                        if let Some(old) = mouse_hook.take() {
                            let _ = UnhookWindowsHookEx(old);
                        }
                        if mouse_enabled {
                            match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0) {
                                Ok(hook) => mouse_hook = Some(hook),
                                Err(error) => {
                                    let _ =
                                        sender.send(PlatformEvent::HookError(error.to_string()));
                                }
                            }
                        }
                    } else if message.message == WM_HOTKEY && message.wParam.0 == EMERGENCY as usize
                    {
                        if !hook_game
                            .read()
                            .unwrap_or_else(|e| e.into_inner())
                            .is_some_and(|p| p.release_emergency)
                        {
                            let _ = sender.send(PlatformEvent::EmergencyRestore);
                        }
                    } else if matches!(message.message, WM_TIMER | RENEW) {
                        notifications.retry_session_registration();
                        if mouse_enabled {
                            if let Some(old) = mouse_hook.take() {
                                let _ = UnhookWindowsHookEx(old);
                            }
                            match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), None, 0) {
                                Ok(hook) => mouse_hook = Some(hook),
                                Err(error) => {
                                    let _ =
                                        sender.send(PlatformEvent::HookError(error.to_string()));
                                }
                            }
                        }
                        if !keyboard_enabled {
                            continue;
                        }
                        if let Some(old) = hook.take() {
                            let _ = UnhookWindowsHookEx(old);
                        }
                        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), None, 0) {
                            Ok(new) => hook = Some(new),
                            Err(error) => {
                                let _ = sender.send(PlatformEvent::HookError(error.to_string()));
                            }
                        }
                        STATE.with(|state| {
                            if let Some(state) = state.borrow_mut().as_mut() {
                                state.pressed =
                                    keyboard_snapshot(|key| GetAsyncKeyState(key.0 as i32) < 0);
                            }
                        });
                        #[cfg(test)]
                        TEST_RENEWALS.fetch_add(1, Ordering::Release);
                    } else {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                if let Some(hook) = mouse_hook {
                    let _ = UnhookWindowsHookEx(hook);
                }
                if power_registered {
                    let _ = PowerUnregisterFromEffectivePowerModeNotifications(power_handle);
                }
                if emergency_registered {
                    let _ = UnregisterHotKey(None, EMERGENCY);
                }
                if timer != 0 {
                    let _ = KillTimer(None, timer);
                }
                drop(notifications);
                if let Some(hook) = hook {
                    let _ = UnhookWindowsHookEx(hook);
                }
                for hook in event_hooks {
                    let _ = UnhookWinEvent(hook);
                }
                STATE.with(|state| *state.borrow_mut() = None);
            })
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;
        let id = ready_rx
            .recv()
            .map_err(|_| Error::new(E_FAIL, "Input thread stopped before initialization"))??;
        Ok(Self { id, pending, pointer, game, thread: Some(thread) })
    }
    pub fn set_game_input(&self, policy: GameInput) -> Result<()> {
        if *self.game.read().unwrap_or_else(|e| e.into_inner()) == Some(policy) {
            return Ok(());
        }
        let previous = self.game.write().unwrap_or_else(|e| e.into_inner()).replace(policy);
        if previous.is_some_and(|p| p.release_emergency) != policy.release_emergency {
            unsafe {
                PostThreadMessageW(self.id, GAME_HOTKEY, WPARAM(0), LPARAM(0))?;
            }
        }
        Ok(())
    }
    pub fn set_pointer_tracking(&self, enabled: bool) -> Result<()> {
        unsafe { PostThreadMessageW(self.id, POINTER, WPARAM(usize::from(enabled)), LPARAM(0)) }
    }
    pub fn take_pointer(&self) -> moon_core::geometry::Point {
        self.pointer.pending.store(false, Ordering::Release);
        let value = self.pointer.position.load(Ordering::Acquire);
        moon_core::geometry::Point::new((value >> 32) as u32 as i32, value as u32 as i32)
    }
    pub fn update_bindings(&self, bindings: Vec<HookBinding>) -> Result<()> {
        *self.pending.lock().unwrap_or_else(|e| e.into_inner()) = Some(compile(bindings)?);
        unsafe { PostThreadMessageW(self.id, UPDATE, WPARAM(0), LPARAM(0)) }
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        let _ = unsafe { PostThreadMessageW(self.id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Must run inside the trigger callback, before any queued modifier release.
fn mask_alt_chord() -> bool {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: ALT_MASK,
                dwFlags: KEYEVENTF_KEYUP,
                dwExtraInfo: MASK_TAG,
                ..Default::default()
            },
        },
    };
    let profile_before = input_profile_before_mask();
    let started = std::time::Instant::now();
    let previous = MASK_CALL
        .with(|cell| cell.replace(Some(MaskCallTiming { started, callbacks: 0, callback: None })));
    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) == 1 };
    let returned = std::time::Instant::now();
    if let Some((handle, before)) = profile_before
        && let Some(after) = read_input_profile(handle)
    {
        INPUT_PROFILE.record(before, after);
    }
    // Preserve an outer observation if Windows dispatch reenters a mask call.
    if let Some(call) = MASK_CALL.with(|cell| cell.replace(previous)) {
        MASK_DISPATCH_LATENCY.record(call, returned);
    }
    sent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_session_registration_requests_one_authoritative_recheck() {
        let (sender, receiver) = mpsc::channel();
        STATE.with(|state| {
            *state.borrow_mut() = Some(State {
                game: Arc::default(),
                bindings: Vec::new(),
                sender,
                pressed: [false; 256],
                swallowed: HashSet::new(),
                taskbar_message: 0,
                pointer: Arc::default(),
            });
        });
        let mut registered = false;
        retry_session_registration(&mut registered, || false);
        assert!(!registered);
        assert!(receiver.try_recv().is_err());
        retry_session_registration(&mut registered, || true);
        assert!(registered);
        assert!(matches!(receiver.try_recv(), Ok(PlatformEvent::System(SystemEvent::Resumed))));
        retry_session_registration(&mut registered, || panic!("already registered"));
        assert!(receiver.try_recv().is_err());
        drop(STATE.with(|state| state.borrow_mut().take()));
    }

    static FORWARDED_F24: AtomicU64 = AtomicU64::new(0);
    static FORWARDED_MASK: AtomicU64 = AtomicU64::new(0);
    unsafe extern "system" fn observe_f24(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if info.dwExtraInfo == MASK_TAG && info.flags.contains(LLKHF_INJECTED) {
                FORWARDED_MASK.fetch_add(1, Ordering::Relaxed);
            } else if info.vkCode == u32::from(VK_F24.0) {
                FORWARDED_F24.fetch_add(1, Ordering::Relaxed);
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
    struct ForwardObserver {
        id: u32,
        thread: Option<JoinHandle<()>>,
    }
    impl ForwardObserver {
        fn start() -> std::result::Result<Self, Box<dyn std::error::Error>> {
            FORWARDED_F24.store(0, Ordering::Relaxed);
            FORWARDED_MASK.store(0, Ordering::Relaxed);
            let (ready, receiver) = mpsc::sync_channel(1);
            let thread = thread::spawn(move || unsafe {
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                match SetWindowsHookExW(WH_KEYBOARD_LL, Some(observe_f24), None, 0) {
                    Ok(hook) => {
                        let _ = ready.send(Ok(GetCurrentThreadId()));
                        while GetMessageW(&mut message, None, 0, 0).as_bool() {}
                        let _ = UnhookWindowsHookEx(hook);
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                }
            });
            Ok(Self { id: receiver.recv()??, thread: Some(thread) })
        }
    }
    impl Drop for ForwardObserver {
        fn drop(&mut self) {
            unsafe {
                let _ = PostThreadMessageW(self.id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
    enum InputScenario {
        Renewal,
        Bursts,
        Defaults,
        KeyPairs,
    }
    #[test]
    fn gaming_forwards_the_entire_switcher_round_trip_and_keeps_desktop_keys() {
        let game = Some(WindowId(100));
        let desktop = Some(WindowId(200));
        let playing =
            GameInput { foreground: game, playing: true, session: true, ..Default::default() };
        let away = GameInput { foreground: desktop, playing: false, ..playing };
        for modifiers in [1, 3] {
            assert!(playing.forwards(game, VK_TAB.0.into(), modifiers));
            assert!(away.forwards(desktop, VK_TAB.0.into(), modifiers));
            assert!(!GameInput { session: false, ..away }.forwards(
                desktop,
                VK_TAB.0.into(),
                modifiers
            ));
        }
        assert!(playing.forwards(game, VK_LEFT.0.into(), 1));
        assert!(!away.forwards(desktop, VK_LEFT.0.into(), 1));
        assert!(away.forwards(game, VK_LEFT.0.into(), 1));
        let known = window_id(unsafe { GetDesktopWindow() });
        let pid = crate::platform::process_id(known);
        let cached = GameInput { desktop: Some((known, pid)), ..playing };
        assert!(!cached.forwards(Some(known), VK_LEFT.0.into(), 1));
        assert!(cached.forwards(Some(known), VK_TAB.0.into(), 1));
        assert!(GameInput { desktop: Some((known, pid.wrapping_add(1))), ..playing }.forwards(
            Some(known),
            VK_LEFT.0.into(),
            1
        ));
    }

    #[test]
    #[ignore = "requires an isolated interactive desktop and MOON_TESTWIN_PATH"]
    fn held_alt_shift_renewal_preserves_hkl_and_plain_trigger()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        modifier_language_regression(InputScenario::Renewal)
    }

    #[test]
    #[ignore = "requires an isolated interactive desktop and MOON_TESTWIN_PATH"]
    fn contiguous_alt_shift_chords_preserve_hkl()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        modifier_language_regression(InputScenario::Bursts)
    }

    #[test]
    #[ignore = "requires an isolated interactive desktop and MOON_TESTWIN_PATH"]
    fn every_default_chord_routes_exact_commands_without_native_actions()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        modifier_language_regression(InputScenario::Defaults)
    }

    #[test]
    #[ignore = "requires an isolated interactive desktop and MOON_TESTWIN_PATH"]
    fn key_pairs_keep_their_initial_owner_through_repeats_and_renewal()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        modifier_language_regression(InputScenario::KeyPairs)
    }

    fn modifier_language_regression(
        scenario: InputScenario,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        use crate::platform;
        use std::time::{Duration, Instant};
        let bursts = matches!(scenario, InputScenario::Bursts);
        let defaults = matches!(scenario, InputScenario::Defaults);
        let pairs = matches!(scenario, InputScenario::KeyPairs);
        fn inject(keys: &[(VIRTUAL_KEY, bool)]) -> bool {
            let inputs: Vec<_> = keys
                .iter()
                .map(|&(key, up)| INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: key,
                            dwFlags: (if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) })
                                | if matches!(key, VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN) {
                                    KEYEVENTF_EXTENDEDKEY
                                } else {
                                    KEYBD_EVENT_FLAGS(0)
                                },
                            ..Default::default()
                        },
                    },
                })
                .collect();
            unsafe {
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) == inputs.len() as u32
            }
        }
        fn helper_counter(id: WindowId, message: u32) -> std::result::Result<u64, &'static str> {
            let mut counts = 0usize;
            let result = unsafe {
                SendMessageTimeoutW(
                    platform::hwnd(id),
                    message,
                    WPARAM(0),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG,
                    1000,
                    Some(&mut counts),
                )
            };
            if result.0 == 0 { Err("helper key-count query timed out") } else { Ok(counts as u64) }
        }
        struct Fixture {
            child: std::process::Child,
            foreground: Option<WindowId>,
            helper: Option<(WindowId, HKL)>,
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                inject(&[(VK_OEM_MINUS, true), (VK_LSHIFT, true), (VK_LMENU, true)]);
                if let Some((id, layout)) = self.helper {
                    unsafe {
                        let _ = PostMessageW(
                            Some(platform::hwnd(id)),
                            WM_INPUTLANGCHANGEREQUEST,
                            WPARAM(0),
                            LPARAM(layout.0 as isize),
                        );
                    }
                    thread::sleep(Duration::from_millis(30));
                }
                let _ = self.child.kill();
                let _ = self.child.wait();
                if let Some(id) = self.foreground.filter(|id| platform::is_window(*id)) {
                    let _ = platform::focus(id);
                }
            }
        }
        if platform::running_processes()?.iter().any(|(_, name)| {
            name.eq_ignore_ascii_case("moon.exe") || name.eq_ignore_ascii_case("testwin.exe")
        }) {
            return Err("stop existing moon/testwin processes before this interactive test".into());
        }
        if modifier_mask(&keyboard_snapshot(|key| unsafe { GetAsyncKeyState(key.0 as i32) < 0 }))
            != 0
        {
            return Err("physical modifiers must be released before this test".into());
        }
        if unsafe { GetAsyncKeyState(VK_OEM_MINUS.0 as i32) } < 0 {
            return Err("the physical minus key must be released before this test".into());
        }
        let _apartment = platform::Apartment::new()?;
        let foreground = platform::foreground();
        let mut fixture = Fixture {
            child: std::process::Command::new(std::env::var("MOON_TESTWIN_PATH")?)
                .arg("1")
                .spawn()?,
            foreground,
            helper: None,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        let helper = loop {
            if let Some(window) = platform::enumerate_windows()?
                .into_iter()
                .find(|w| w.process_id == fixture.child.id() && w.class == "MoonTestWindow")
            {
                break window.id;
            }
            if Instant::now() >= deadline {
                return Err("owned helper did not appear".into());
            }
            thread::sleep(Duration::from_millis(20));
        };
        platform::focus(helper)?;
        if platform::foreground() != Some(helper) {
            return Err("cannot activate the owned helper; no keyboard input was injected".into());
        }
        let helper_thread = unsafe { GetWindowThreadProcessId(platform::hwnd(helper), None) };
        let initial_layout = unsafe { GetKeyboardLayout(helper_thread) };
        fixture.helper = Some((helper, initial_layout));
        let available_layouts = unsafe { GetKeyboardLayoutList(None) };
        // Calibrate the counter so an older helper cannot falsely report zero.
        helper_counter(helper, WM_CHAR)?;
        let character_baseline = helper_counter(helper, WM_APP + 94)?;
        if character_baseline != 1 {
            return Err("rebuild testwin: its character-event counter did not observe the calibration message".into());
        }
        if bursts || defaults || pairs {
            if available_layouts < 2 {
                return Err("the burst regression requires two installed input layouts".into());
            }
            // Prove this desktop's Alt+Shift language switch is actually armed.
            // The change and restoration are confined to the owned helper.
            assert!(inject(&[
                (VK_LMENU, false),
                (VK_LSHIFT, false),
                (VK_LSHIFT, true),
                (VK_LMENU, true),
            ]));
            thread::sleep(Duration::from_millis(100));
            let switched = unsafe { GetKeyboardLayout(helper_thread) };
            if switched == initial_layout {
                return Err("Alt+Shift positive control did not change the owned helper's HKL; no burst result claimed".into());
            }
            unsafe {
                PostMessageW(
                    Some(platform::hwnd(helper)),
                    WM_INPUTLANGCHANGEREQUEST,
                    WPARAM(0),
                    LPARAM(initial_layout.0 as isize),
                )?;
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while unsafe { GetKeyboardLayout(helper_thread) } != initial_layout {
                if Instant::now() >= deadline {
                    return Err("owned helper did not restore its original HKL".into());
                }
                thread::sleep(Duration::from_millis(1));
            }
            println!(
                "positive control: bare Alt+Shift changed HKL {:x}->{:x}, original restored",
                initial_layout.0 as usize, switched.0 as usize
            );
            if pairs {
                assert!(inject(&[(VK_OEM_MINUS, false)]));
                thread::sleep(Duration::from_millis(20));
                assert_eq!(
                    helper_counter(helper, WM_APP + 93)?,
                    1,
                    "initial down did not reach the helper before hook installation"
                );
            }
        } else {
            assert!(inject(&[(VK_LMENU, false), (VK_LSHIFT, false)]));
        }
        let (sender, receiver) = mpsc::channel();
        let observer = ForwardObserver::start()?;
        let test_bindings = if defaults {
            moon_core::config::Config::default()
                .bindings
                .into_iter()
                .map(|(chord, commands)| HookBinding { chord, commands })
                .collect()
        } else if pairs {
            vec![HookBinding {
                chord: "alt+shift+oem_minus".into(),
                commands: vec!["modified".into()],
            }]
        } else {
            vec![
                HookBinding { chord: "oem_minus".into(), commands: vec!["plain".into()] },
                HookBinding {
                    chord: "alt+shift+oem_minus".into(),
                    commands: vec!["modified".into()],
                },
            ]
        };
        let default_routes = test_bindings.clone();
        let hook = Hooks::start(test_bindings, sender, true)?;
        let expect = |expected: &[String]| -> std::result::Result<(), Box<dyn std::error::Error>> {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if Instant::now() >= deadline {
                    return Err(format!("expected {expected:?} trigger was not received").into());
                }
                if let Ok(PlatformEvent::Commands { commands, .. }) =
                    receiver.recv_timeout(Duration::from_millis(10))
                {
                    if commands.as_ref() != expected {
                        return Err(format!(
                            "modifier mismatch: expected {expected:?}, received {commands:?}"
                        )
                        .into());
                    }
                    return Ok(());
                }
            }
        };
        let modified = ["modified".to_owned()];
        let plain = ["plain".to_owned()];
        let renew = || -> std::result::Result<(), Box<dyn std::error::Error>> {
            let before = TEST_RENEWALS.load(Ordering::Acquire);
            unsafe {
                PostThreadMessageW(hook.id, RENEW, WPARAM(0), LPARAM(0))?;
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while TEST_RENEWALS.load(Ordering::Acquire) == before {
                if Instant::now() >= deadline {
                    return Err("hook renewal did not complete".into());
                }
                thread::sleep(Duration::from_millis(1));
            }
            Ok(())
        };
        if pairs {
            let app_owned_repeat = [
                (VK_LMENU, false),
                (VK_LSHIFT, false),
                (VK_OEM_MINUS, false),
                (VK_LSHIFT, true),
                (VK_LMENU, true),
                (VK_OEM_MINUS, true),
            ];
            // The initial down preceded installation; modifiers cannot transfer
            // its repeat/up ownership away from the application.
            assert!(inject(&app_owned_repeat));
            thread::sleep(Duration::from_millis(20));
            assert_eq!(
                helper_counter(helper, WM_APP + 93)?,
                2 | (1 << 32),
                "held-at-install key lost its forwarded repeat/up"
            );
            assert!(
                !receiver.try_iter().any(|event| matches!(event, PlatformEvent::Commands { .. })),
                "an app-owned press became a shortcut"
            );

            // Forwarded input retains normal Windows language-switch behavior.
            // Each consumed phase must preserve its own immediate HKL baseline.
            let consumed_layout = unsafe { GetKeyboardLayout(helper_thread) };
            // A consumed down stays consumed across renewal and modifier release,
            // while repetition of the same chord continues to execute normally.
            assert!(inject(&[(VK_LMENU, false), (VK_LSHIFT, false), (VK_OEM_MINUS, false)]));
            expect(&modified)?;
            renew()?;
            assert!(inject(&[(VK_OEM_MINUS, false)]));
            expect(&modified)?;
            assert!(inject(&[
                (VK_LSHIFT, true),
                (VK_LMENU, true),
                (VK_OEM_MINUS, false),
                (VK_OEM_MINUS, true)
            ]));
            thread::sleep(Duration::from_millis(20));
            assert_eq!(
                helper_counter(helper, WM_APP + 93)?,
                2 | (1 << 32),
                "consumed press leaked after renewal or modifier release"
            );
            assert_eq!(unsafe { GetKeyboardLayout(helper_thread) }, consumed_layout);

            // Renewal must retain a forwarded, still-held nonmodifier key.
            assert!(inject(&[(VK_OEM_MINUS, false)]));
            thread::sleep(Duration::from_millis(20));
            renew()?;
            assert!(inject(&app_owned_repeat));
            thread::sleep(Duration::from_millis(20));
            assert_eq!(
                helper_counter(helper, WM_APP + 93)?,
                4 | (2 << 32),
                "held-at-renewal key lost its forwarded repeat/up"
            );
            assert!(
                !receiver.try_iter().any(|event| matches!(event, PlatformEvent::Commands { .. })),
                "an app-owned or unmatched repeat queued a command"
            );
            let next_layout = unsafe { GetKeyboardLayout(helper_thread) };
            assert!(inject(&[
                (VK_LMENU, false),
                (VK_LSHIFT, false),
                (VK_OEM_MINUS, false),
                (VK_OEM_MINUS, true),
                (VK_LSHIFT, true),
                (VK_LMENU, true)
            ]));
            expect(&modified)?;
            thread::sleep(Duration::from_millis(20));
            assert_eq!(unsafe { GetKeyboardLayout(helper_thread) }, next_layout);
            assert!(
                unsafe { GetAsyncKeyState(VK_OEM_MINUS.0 as i32) } >= 0,
                "trigger remained logically held"
            );
            assert_eq!(
                modifier_mask(&keyboard_snapshot(|key| unsafe {
                    GetAsyncKeyState(key.0 as i32) < 0
                })),
                0
            );
            println!(
                "key-pair ownership: held-at-install and held-at-renewal forwarded 4 downs/2 ups; consumed repeats stayed hidden across renewal and modifier release; same-chord repeat and next press routed3commands; consumed phases preserved their HKL"
            );
        } else if defaults {
            let menu_flags = || -> windows::core::Result<GUITHREADINFO_FLAGS> {
                let mut info = GUITHREADINFO {
                    cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                    ..Default::default()
                };
                unsafe {
                    GetGUIThreadInfo(helper_thread, &mut info)?;
                }
                Ok(info.flags & (GUI_INMENUMODE | GUI_POPUPMENUMODE | GUI_SYSTEMMENUMODE))
            };
            let initial_menu = menu_flags()?;
            assert_eq!(
                initial_menu,
                GUITHREADINFO_FLAGS(0),
                "owned helper starts in native menu mode"
            );
            for (index, binding) in default_routes.iter().enumerate() {
                if platform::foreground() != Some(helper) {
                    return Err("foreground left the owned helper".into());
                }
                // Independent physical-key oracle for the stock configuration;
                // do not use the hook compiler to generate its own test inputs.
                let parts: Vec<_> = binding.chord.split('+').collect();
                let key_name = parts.last().ok_or("empty default chord")?;
                let key = VIRTUAL_KEY(match *key_name {
                    "left" => 0x25,
                    "up" => 0x26,
                    "right" => 0x27,
                    "down" => 0x28,
                    "space" => 0x20,
                    "tab" => 0x09,
                    "oem_plus" => 0xbb,
                    "oem_minus" => 0xbd,
                    "oem_question" => 0xbf,
                    value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphanumeric() => {
                        u16::from(value.as_bytes()[0].to_ascii_uppercase())
                    }
                    _ => {
                        return Err(
                            format!("add an independent input mapping for {key_name}").into()
                        );
                    }
                });
                let modifiers: Vec<_> = parts[..parts.len() - 1]
                    .iter()
                    .map(|part| match *part {
                        "alt" => Ok(VK_LMENU),
                        "shift" => Ok(VK_LSHIFT),
                        "ctrl" => Ok(VK_LCONTROL),
                        _ => Err("unexpected default modifier"),
                    })
                    .collect::<std::result::Result<_, _>>()?;
                let mut chord: Vec<_> = modifiers.iter().map(|key| (*key, false)).collect();
                chord.push((key, false));
                if index % 2 == 0 {
                    chord.push((key, true));
                }
                chord.extend(modifiers.iter().rev().map(|key| (*key, true)));
                if index % 2 == 1 {
                    chord.push((key, true));
                }
                assert!(inject(&chord));
                expect(&binding.commands)?;
                thread::sleep(Duration::from_millis(20));
                assert_eq!(
                    platform::foreground(),
                    Some(helper),
                    "{} changed foreground",
                    binding.chord
                );
                assert_eq!(
                    unsafe { GetKeyboardLayout(helper_thread) },
                    initial_layout,
                    "{} changed HKL",
                    binding.chord
                );
                assert_eq!(
                    menu_flags()?,
                    initial_menu,
                    "{} activated a native menu",
                    binding.chord
                );
                assert_eq!(
                    modifier_mask(&keyboard_snapshot(|key| unsafe {
                        GetAsyncKeyState(key.0 as i32) < 0
                    })),
                    0,
                    "{} left a modifier pressed",
                    binding.chord
                );
            }
            println!(
                "{}/{} default chords routed exact payloads; foreground, HKL and menu unchanged; all modifiers released; no daemon effects executed",
                default_routes.len(),
                default_routes.len()
            );
        } else if bursts {
            for trial in 0..128 {
                if platform::foreground() != Some(helper) {
                    return Err("foreground left the owned helper".into());
                }
                let mut chord = [
                    (VK_LMENU, false),
                    (VK_LSHIFT, false),
                    (VK_OEM_MINUS, false),
                    (VK_OEM_MINUS, true),
                    (VK_LSHIFT, true),
                    (VK_LMENU, true),
                ];
                if trial % 2 == 1 {
                    chord[3..].rotate_left(1);
                }
                assert!(inject(&chord));
                // Model a busy main thread: all releases precede command intake.
                thread::sleep(Duration::from_millis(20));
                expect(&modified)?;
                thread::sleep(Duration::from_millis(20));
                let actual = unsafe { GetKeyboardLayout(helper_thread) };
                let counts = helper_counter(helper, WM_APP + 93)?;
                assert_eq!(
                    helper_counter(helper, WM_APP + 94)?,
                    character_baseline,
                    "mask generated an application character message"
                );
                if trial == 0 || actual != initial_layout {
                    println!(
                        "trial {trial}: helper trigger events down={}, up={}",
                        counts as u32,
                        counts >> 32
                    );
                }
                if actual != initial_layout {
                    println!("hook timing: {}", serde_json::to_string(&stats())?);
                    return Err(format!(
                        "contiguous Alt+Shift chord changed HKL in trial {trial}: {:x}->{:x}",
                        initial_layout.0 as usize, actual.0 as usize
                    )
                    .into());
                }
                assert!(inject(&[(VK_OEM_MINUS, false), (VK_OEM_MINUS, true)]));
                expect(&plain)?;
            }
            assert_eq!(
                helper_counter(helper, WM_APP + 93)?,
                0,
                "consumed triggers leaked to the helper"
            );
            assert_eq!(
                helper_counter(helper, WM_APP + 94)?,
                character_baseline,
                "mask generated an application character message"
            );
            println!(
                "128 contiguous chords:256/256 triggers, unchanged HKL; hook timing: {}",
                serde_json::to_string(&stats())?
            );
        } else {
            for trial in 0..11 {
                if trial > 0 {
                    assert!(inject(&[(VK_LMENU, false), (VK_LSHIFT, false)]));
                    let before = TEST_RENEWALS.load(Ordering::Acquire);
                    unsafe {
                        PostThreadMessageW(hook.id, RENEW, WPARAM(0), LPARAM(0))?;
                    }
                    let deadline = Instant::now() + Duration::from_secs(2);
                    while TEST_RENEWALS.load(Ordering::Acquire) == before {
                        if Instant::now() >= deadline {
                            return Err("hook renewal did not complete".into());
                        }
                        thread::sleep(Duration::from_millis(1));
                    }
                }
                if platform::foreground() != Some(helper) {
                    return Err("foreground left the owned helper".into());
                }
                assert!(inject(&[(VK_OEM_MINUS, false), (VK_OEM_MINUS, true)]));
                expect(&modified)?;
                assert!(inject(&[(VK_LSHIFT, true), (VK_LMENU, true)]));
                thread::sleep(Duration::from_millis(40));
                assert_eq!(
                    unsafe { GetKeyboardLayout(helper_thread) },
                    initial_layout,
                    "Alt+Shift changed HKL in trial {trial}"
                );
                assert!(inject(&[(VK_OEM_MINUS, false), (VK_OEM_MINUS, true)]));
                expect(&plain)?;
            }
            println!(
                "initially held modifiers +10 renewals:22/22 triggers; HKL={:x}, available_layouts={available_layouts}",
                initial_layout.0 as usize
            );
        }
        assert_eq!(
            FORWARDED_MASK.load(Ordering::Relaxed),
            0,
            "internal masks reached downstream hooks"
        );
        assert_eq!(
            FORWARDED_F24.load(Ordering::Relaxed),
            0,
            "internal masks became real F24 events"
        );
        assert!(inject(&[(VK_F24, false), (VK_F24, true)]));
        thread::sleep(Duration::from_millis(20));
        assert_eq!(
            FORWARDED_F24.load(Ordering::Relaxed),
            2,
            "ordinary F24 must reach downstream hooks"
        );
        assert_eq!(
            helper_counter(helper, WM_APP + 93)?,
            if pairs { 4 | (2 << 32) } else { 0 },
            "trigger-pair ownership changed"
        );
        if !pairs {
            assert_eq!(
                helper_counter(helper, WM_APP + 94)?,
                character_baseline,
                "unexpected application character message"
            );
        }
        for event in receiver.try_iter() {
            match event {
                PlatformEvent::Commands { .. } => {
                    return Err("an extra command was queued after the expected routes".into());
                }
                PlatformEvent::HookError(error) => return Err(error.into()),
                _ => {}
            }
        }
        println!(
            "downstream ordinary F24=2; internal mask leaks=0; trigger-pair ownership verified"
        );
        drop(hook);
        drop(observer);
        Ok(())
    }

    #[test]
    fn phase_timings_preserve_sample_counts_and_outliers_without_unbounded_storage() {
        let latency = PhaseLatency::new();
        let empty = latency.stats();
        assert_eq!((empty.samples, empty.p50_us, empty.p99_us, empty.max_us), (0, 0, 0, 0));
        for _ in 0..99 {
            latency.record(std::time::Duration::from_nanos(1));
        }
        latency.record(std::time::Duration::from_micros(6000));
        let one_outlier = latency.stats();
        assert_eq!(
            (one_outlier.samples, one_outlier.p50_us, one_outlier.p99_us, one_outlier.max_us),
            (100, 1, 1, 6000)
        );
        latency.record(std::time::Duration::from_micros(5000));
        let two_outliers = latency.stats();
        assert_eq!(
            (two_outliers.samples, two_outliers.p50_us, two_outliers.p99_us, two_outliers.max_us),
            (101, 1, 4096, 6000)
        );
    }

    #[test]
    fn dispatch_profile_counts_wrap_and_use_only_the_interval_wait_bitmap() {
        let counters = InputProfileCounters::default();
        counters.record(
            DispatchSnapshot { switches: 10, wait_reasons: 0x80 },
            DispatchSnapshot { switches: 10, wait_reasons: 0 },
        );
        counters.record(
            DispatchSnapshot { switches: u32::MAX - 1, wait_reasons: 0x80 },
            DispatchSnapshot { switches: 1, wait_reasons: 0x10 },
        );
        counters.record(
            DispatchSnapshot { switches: 1, wait_reasons: 0x10 },
            DispatchSnapshot { switches: 3, wait_reasons: 0x4000_0000 },
        );
        let stats = counters.stats();
        assert_eq!(stats.successful_samples, 3);
        assert_eq!(stats.calls_with_switches, 2);
        assert_eq!((stats.total_switches, stats.max_switches), (5, 3));
        assert_eq!(stats.wait_reason_bitmap, 0x4000_0010);
        assert_eq!((stats.read_failures, stats.nested_calls_skipped), (0, 0));
        assert!(!stats.requested && !stats.available);
    }

    #[test]
    fn mask_dispatch_splits_only_single_callback_calls() {
        use std::time::{Duration, Instant};
        let latency = MaskDispatchLatency::new();
        let started = Instant::now();
        let at = |us| started + Duration::from_micros(us);
        let mut single = MaskCallTiming { started, callbacks: 0, callback: None };
        single.observe(at(10), at(12));
        latency.record(single, at(40));

        // Delivery after return cannot be attributed to a synchronous phase.
        latency.record(MaskCallTiming { started, callbacks: 0, callback: None }, at(20));
        // Multiple callbacks must not accidentally report only the first one.
        let mut multiple = single;
        multiple.observe(at(15), at(18));
        latency.record(multiple, at(50));

        let stats = latency.stats();
        assert_eq!(stats.send_input.samples, 3);
        assert_eq!((stats.send_input.p50_us, stats.send_input.max_us), (40, 50));
        assert_eq!(stats.calls_without_callback, 1);
        assert_eq!(stats.calls_with_multiple_callbacks, 1);
        assert_eq!(stats.callbacks_outside_call, 0);
        for (phase, expected_us) in
            [(stats.before_callback, 10), (stats.callback_handler, 2), (stats.after_callback, 28)]
        {
            assert_eq!(phase.samples, 1);
            assert_eq!(
                (phase.p50_us, phase.p99_us, phase.max_us),
                (expected_us, expected_us, expected_us)
            );
        }
    }

    #[test]
    fn only_the_exact_internal_injected_keyup_bypasses_other_hooks() {
        let mask = KBDLLHOOKSTRUCT {
            vkCode: u32::from(ALT_MASK.0),
            flags: LLKHF_INJECTED | LLKHF_UP,
            dwExtraInfo: MASK_TAG,
            ..Default::default()
        };
        assert!(is_internal_mask(&mask));
        for ordinary in [
            KBDLLHOOKSTRUCT { vkCode: u32::from(VK_F24.0), ..mask },
            KBDLLHOOKSTRUCT { flags: LLKHF_UP, ..mask },
            KBDLLHOOKSTRUCT { flags: LLKHF_INJECTED, ..mask },
            KBDLLHOOKSTRUCT { flags: mask.flags | LLKHF_LOWER_IL_INJECTED, ..mask },
            KBDLLHOOKSTRUCT { scanCode: 1, ..mask },
            KBDLLHOOKSTRUCT { dwExtraInfo: 0, ..mask },
        ] {
            assert!(!is_internal_mask(&ordinary));
        }
    }

    #[test]
    fn renewed_modifiers_clear_on_generic_and_sided_release() {
        // Documented hardware scan codes, independent of the active layout's
        // reverse mapping (Korean layouts have no VK_RCONTROL/VK_RMENU mapping).
        for (generic, side, expected_mask, scan) in [
            (VK_SHIFT, VK_LSHIFT, 2, 0x2a),
            (VK_SHIFT, VK_RSHIFT, 2, 0x36),
            (VK_CONTROL, VK_LCONTROL, 4, 0x1d),
            (VK_CONTROL, VK_RCONTROL, 4, 0xe01d),
            (VK_MENU, VK_LMENU, 1, 0x38),
            (VK_MENU, VK_RMENU, 1, 0xe038),
        ] {
            for event_key in [generic, side] {
                // Windows reports both the generic and the actual side as held
                // during renewal. Only the side may survive in our snapshot.
                let mut pressed = keyboard_snapshot(|key| key == generic || key == side);
                assert_eq!(modifier_mask(&pressed), expected_mask);
                assert!(!pressed[generic.0 as usize]);
                let event = KBDLLHOOKSTRUCT {
                    vkCode: u32::from(event_key.0),
                    scanCode: scan & 0xff,
                    flags: if scan & 0xff00 != 0 {
                        LLKHF_EXTENDED
                    } else {
                        KBDLLHOOKSTRUCT_FLAGS(0)
                    },
                    ..Default::default()
                };
                assert_eq!(record_key(&mut pressed, &event, true).0, u32::from(side.0));
                assert_eq!(modifier_mask(&pressed), 0);
            }
        }
    }

    #[test]
    fn releasing_one_modifier_side_preserves_the_other_side() {
        let mut pressed = keyboard_snapshot(|key| key == VK_LMENU || key == VK_RMENU);
        for (side, expected_mask) in [(VK_LMENU, 1), (VK_RMENU, 0)] {
            record_key(
                &mut pressed,
                &KBDLLHOOKSTRUCT { vkCode: u32::from(side.0), ..Default::default() },
                true,
            );
            assert_eq!(modifier_mask(&pressed), expected_mask);
        }
    }

    #[test]
    fn every_built_in_binding_has_a_native_key_mapping() {
        let config = moon_core::config::Config::default();
        let count = config.bindings.len();
        let bindings = compile(
            config
                .bindings
                .into_iter()
                .map(|(chord, commands)| HookBinding { chord, commands })
                .collect(),
        )
        .unwrap();
        assert_eq!(bindings.len(), count);
        assert!(
            bindings
                .iter()
                .any(|binding| binding.key == VK_OEM_2.0 as u32 && binding.modifiers == 1)
        );
    }
    #[test]
    fn malformed_keybindings_are_rejected_before_installing_a_hook() {
        for chord in ["alt", "alt+banana", "alt+h+j", "ctrl+f25"] {
            assert!(
                compile(vec![HookBinding { chord: chord.into(), commands: vec!["close".into()] }])
                    .is_err()
            );
        }
    }
}
