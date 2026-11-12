//! Full-daemon measurements using eight owned windows and the real keyboard hook.
//! Refuses an existing daemon or another window manager; ignores all user windows.
#[cfg(any(windows, test))]
#[derive(Debug, Default)]
#[cfg_attr(windows, derive(serde::Serialize))]
struct ReadbackTiming {
    passes: u64,
    calls: u64,
    short_circuit_skips: u64,
    completed_pass_skips: u64,
    total_ns: u64,
    max_ns: u64,
}

#[cfg(any(windows, test))]
impl ReadbackTiming {
    fn measure<T>(&mut self, read: impl FnOnce() -> T) -> T {
        let started = std::time::Instant::now();
        let result = read();
        let elapsed = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        self.calls += 1;
        self.total_ns += elapsed;
        self.max_ns = self.max_ns.max(elapsed);
        result
    }

    fn finish_pass(&mut self, targets: usize, calls_before: u64) {
        self.passes += 1;
        self.short_circuit_skips += targets as u64 - (self.calls - calls_before);
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug)]
struct PlacementStamp {
    sequence: u64,
    qpc: i64,
    outer: [i32; 4],
}

#[cfg(any(windows, test))]
fn changed_target_ticks(
    before: PlacementStamp,
    after: PlacementStamp,
    expected: [i32; 4],
    key_qpc: i64,
) -> Result<Option<u64>, &'static str> {
    if before.sequence == 0
        || after.sequence < before.sequence
        || !before.sequence.is_multiple_of(2)
        || !after.sequence.is_multiple_of(2)
    {
        return Err("missing, incomplete or regressed helper placement sequence");
    }
    if after.outer != expected {
        return Err("helper placement snapshot does not match the exact expected outer rectangle");
    }
    if before.outer == expected {
        return Ok(None);
    }
    if after.sequence == before.sequence {
        return Err("changed target has no new helper placement callback");
    }
    let elapsed = after.qpc.checked_sub(key_qpc).ok_or("helper placement QPC overflow")?;
    if elapsed < -1 {
        return Err("changed target callback predates the trigger");
    }
    // Microsoft documents +/- one tick ordering uncertainty across threads.
    Ok(Some(elapsed.max(0) as u64))
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_core::{geometry::Rect, ids::WindowId, world::World};
    use moon_win::{ipc, platform};
    use serde_json::{Value, json};
    use std::{
        collections::BTreeMap,
        process::Command,
        thread,
        time::{Duration, Instant},
    };
    use windows::Win32::{Foundation::*, System::Threading::*, UI::Input::KeyboardAndMouse::*};

    fn native_rect(id: WindowId) -> windows::core::Result<Rect> {
        let mut rect = RECT::default();
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowRect(platform::hwnd(id), &mut rect)?;
        }
        Ok(Rect::new(rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top))
    }

    fn qpc() -> windows::core::Result<i64> {
        let mut value = 0;
        unsafe { windows::Win32::System::Performance::QueryPerformanceCounter(&mut value)? };
        Ok(value)
    }

    fn placement_stamp(id: WindowId, owner: u32) -> Result<PlacementStamp, &'static str> {
        use windows::Win32::UI::WindowsAndMessaging::GetPropW;
        for _ in 0..8 {
            if platform::process_id(id) != owner {
                return Err("placement diagnostic window no longer belongs to the owned helper");
            }
            let read = |index| unsafe {
                GetPropW(platform::hwnd(id), platform::TESTWIN_PLACEMENT_PROPERTIES[index]).0 as u64
            };
            let sequence = read(0);
            if sequence == 0 {
                return Err("helper placement instrumentation is missing or failed to publish");
            }
            if !sequence.is_multiple_of(2) {
                continue;
            }
            let counter = read(1);
            let encoded = [read(2), read(3), read(4), read(5)];
            if sequence != read(0) {
                continue;
            }
            if platform::process_id(id) != owner {
                return Err("placement diagnostic owner changed while reading the snapshot");
            }
            if counter == 0 || counter > i64::MAX as u64 {
                return Err("helper placement QPC is missing or invalid");
            }
            if encoded.iter().any(|&value| value == 0 || value > u64::from(u32::MAX) + 1) {
                return Err("helper placement rectangle is missing or invalid");
            }
            return Ok(PlacementStamp {
                sequence,
                qpc: counter as i64,
                outer: encoded.map(|value| (value - 1) as u32 as i32),
            });
        }
        Err("helper placement snapshot remained inconsistent across eight reads")
    }

    fn query(name: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let reply = ipc::request(&json!({"version":1,"id":1,"method":"query","query":name}))?;
        if reply["ok"] != true {
            return Err(format!("query failed: {reply}").into());
        }
        Ok(reply["result"].clone())
    }
    fn inject(keys: &[(VIRTUAL_KEY, bool)]) -> bool {
        let inputs: Vec<_> = keys
            .iter()
            .map(|&(key, up)| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: key,
                        dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                        ..Default::default()
                    },
                },
            })
            .collect();
        unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) == inputs.len() as u32 }
    }
    struct Cleanup {
        daemon: Option<std::process::Child>,
        helper: Option<std::process::Child>,
        config: std::path::PathBuf,
        focus: Option<moon_core::ids::WindowId>,
        injected: bool,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if self.injected {
                inject(&[(VK_LSHIFT, true), (VK_LMENU, true)]);
            }
            if let Some(child) = &mut self.daemon {
                if query("stats").is_ok_and(|s| s["process_id"] == child.id()) {
                    let _ = ipc::request(
                        &json!({"version":1,"id":2,"method":"command","commands":["exit"]}),
                    );
                    for _ in 0..100 {
                        if child.try_wait().ok().flatten().is_some() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(20));
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
            }
            if let Some(child) = &mut self.helper {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = std::fs::remove_file(&self.config);
            if let Some(id) = self.focus {
                let _ = platform::focus(id);
            }
        }
    }
    fn cpu(pid: u32) -> windows::core::Result<u64> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
            let (mut created, mut ended, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            let result = GetProcessTimes(process, &mut created, &mut ended, &mut kernel, &mut user);
            let _ = CloseHandle(process);
            result?;
            let ticks =
                |v: FILETIME| (u64::from(v.dwHighDateTime) << 32) | u64::from(v.dwLowDateTime);
            Ok(ticks(kernel) + ticks(user))
        }
    }
    // External read-only residency classification. Never reads process contents,
    // faults pages in, trims the working set, or reports virtual addresses.
    fn memory_profile(pid: u32) -> windows::core::Result<Value> {
        use windows::Win32::System::{Memory::*, ProcessStatus::*, SystemInformation::*};
        #[derive(Default, serde::Serialize)]
        struct Group {
            regions: u64,
            committed_bytes: u64,
            resident_bytes: u64,
            shareable_resident_bytes: u64,
            private_resident_bytes: u64,
        }
        struct Process(HANDLE);
        impl Drop for Process {
            fn drop(&mut self) {
                let _ = unsafe { CloseHandle(self.0) };
            }
        }
        let started = Instant::now();
        let process = Process(unsafe {
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)?
        });
        let mut system = SYSTEM_INFO::default();
        unsafe {
            GetSystemInfo(&mut system);
        }
        let page_size = system.dwPageSize as usize;
        let mut address = 0usize;
        let mut groups = BTreeMap::<String, Group>::new();
        let mut pages = Vec::with_capacity(4096);
        while address <= system.lpMaximumApplicationAddress as usize {
            let mut region = MEMORY_BASIC_INFORMATION::default();
            if unsafe {
                VirtualQueryEx(
                    process.0,
                    Some(address as *const _),
                    &mut region,
                    size_of::<MEMORY_BASIC_INFORMATION>(),
                )
            } == 0
            {
                return Err(windows::core::Error::from_thread());
            }
            let base = region.BaseAddress as usize;
            let Some(next) = base.checked_add(region.RegionSize).filter(|&next| next > address)
            else {
                return Err(windows::core::Error::new(E_FAIL, "Memory region did not advance"));
            };
            address = next;
            if region.State != MEM_COMMIT {
                continue;
            }
            let kind = match region.Type {
                MEM_IMAGE => "image",
                MEM_MAPPED => "mapped",
                MEM_PRIVATE => "private",
                _ => "other",
            };
            let mut name = kind.to_owned();
            if region.Type == MEM_IMAGE || region.Type == MEM_MAPPED {
                let mut path = [0u16; 32768];
                let length = unsafe { GetMappedFileNameW(process.0, region.BaseAddress, &mut path) }
                    as usize;
                if length > 0 {
                    let path = String::from_utf16_lossy(&path[..length]);
                    name.push(':');
                    name.push_str(path.rsplit('\\').next().unwrap_or("unknown"));
                }
            }
            let group = groups.entry(name).or_default();
            group.regions += 1;
            group.committed_bytes += region.RegionSize as u64;
            let mut offset = 0;
            while offset < region.RegionSize {
                pages.clear();
                for _ in 0..4096 {
                    if offset >= region.RegionSize {
                        break;
                    }
                    pages.push(PSAPI_WORKING_SET_EX_INFORMATION {
                        VirtualAddress: (base + offset) as *mut _,
                        ..Default::default()
                    });
                    offset += page_size;
                }
                unsafe {
                    QueryWorkingSetEx(
                        process.0,
                        pages.as_mut_ptr().cast(),
                        (pages.len() * size_of::<PSAPI_WORKING_SET_EX_INFORMATION>()) as u32,
                    )?;
                }
                for page in &pages {
                    let flags = unsafe { page.VirtualAttributes.Flags };
                    if flags & 1 == 0 {
                        continue;
                    }
                    group.resident_bytes += page_size as u64;
                    if flags & (1 << 15) != 0 {
                        group.shareable_resident_bytes += page_size as u64;
                    } else {
                        group.private_resident_bytes += page_size as u64;
                    }
                }
            }
        }
        let total: u64 = groups.values().map(|group| group.resident_bytes).sum();
        Ok(
            json!({"groups":groups,"resident_bytes":total,"page_size":page_size,"observation_ms":started.elapsed().as_secs_f64()*1000.0,
            "scope":"VirtualQueryEx committed regions and QueryWorkingSetEx valid pages; non-atomic read-only snapshot, shareable is not exclusively owned memory. No page contents or virtual addresses are recorded."}),
        )
    }
    let args: Vec<_> = std::env::args().collect();
    let control = args.get(3).is_some_and(|arg| arg == "--memory-control");
    let target_timing = std::env::var("MOON_PERF_PLACEMENT_TIMING").is_ok_and(|value| value == "1");
    let mut qpc_frequency = 0;
    if target_timing {
        if control {
            return Err(
                "placement timestamps require the resize workload, not memory control".into()
            );
        }
        unsafe {
            windows::Win32::System::Performance::QueryPerformanceFrequency(&mut qpc_frequency)?;
        }
        if qpc_frequency <= 0 {
            return Err("invalid placement diagnostic QPC frequency".into());
        }
    }
    if args.len() != 3 && !(args.len() == 4 && control) {
        return Err("usage: daemon_performance.exe moon.exe testwin.exe [--memory-control]".into());
    }
    let _apartment = platform::Apartment::new()?;
    let _ = platform::enable_dpi_awareness();
    if query("stats").is_ok() {
        return Err("an existing moon daemon must be exited before this probe".into());
    }
    let config = std::env::temp_dir().join(format!("moon-performance-{}.toml", std::process::id()));
    let mut cleanup = Cleanup {
        daemon: None,
        helper: None,
        config,
        focus: platform::foreground(),
        injected: false,
    };
    std::fs::write(
        &cleanup.config,
        "config-version = 1\n[[rules]]\ncommands = [\"ignore\"]\nmatches = [{process={not-regex=\"(?i)^testwin(?:\\\\.exe)?$\"}}]\n",
    )?;
    let mut helper = Command::new(&args[2]);
    helper.arg("8").env_remove("MOON_TESTWIN_PLACEMENT_TIMING");
    if target_timing {
        helper.env("MOON_TESTWIN_PLACEMENT_TIMING", "1");
    }
    cleanup.helper = Some(helper.spawn()?);
    let helper_pid = cleanup.helper.as_ref().unwrap().id();
    let deadline = Instant::now() + Duration::from_secs(10);
    while platform::enumerate_windows()?.iter().filter(|w| w.process_id == helper_pid).count() != 8
    {
        if Instant::now() >= deadline {
            return Err("eight helper windows did not open".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    let mut daemon = Command::new(&args[1]);
    daemon.args(["start", "--config"]).arg(&cleanup.config);
    if std::env::var_os("MOON_PERF_NO_SURFACES").is_some() {
        daemon.arg("--no-surfaces");
    }
    cleanup.daemon = Some(daemon.stdout(std::process::Stdio::null()).spawn()?);
    let daemon_pid = cleanup.daemon.as_ref().unwrap().id();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut world = loop {
        if let Some(status) = cleanup.daemon.as_mut().unwrap().try_wait()? {
            return Err(format!("daemon failed to start: {status}").into());
        }
        if let Ok(value) = query("world") {
            let world: World = serde_json::from_value(value)?;
            if world.windows.len() == 8
                && world.windows.values().all(|w| w.info.process_id == helper_pid)
            {
                break world;
            }
        }
        if Instant::now() >= deadline {
            return Err("daemon did not adopt exactly the eight owned windows".into());
        }
        thread::sleep(Duration::from_millis(20));
    };
    if query("stats")?["process_id"] != daemon_pid {
        return Err("unexpected daemon identity".into());
    }
    platform::focus(world.focused.ok_or("no focused helper")?)?;
    let initial = world.plan().placements;
    let initial_deadline = Instant::now() + Duration::from_secs(2);
    while !initial.iter().all(|p| platform::window_rect(p.id).is_ok_and(|r| r == p.rect)) {
        if Instant::now() >= initial_deadline {
            return Err("initial frames did not settle".into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    let mut frame_insets = BTreeMap::new();
    for p in &initial {
        let outer = native_rect(p.id)?;
        frame_insets.insert(
            p.id,
            (
                p.rect.x - outer.x,
                p.rect.y - outer.y,
                outer.right() - p.rect.right(),
                outer.bottom() - p.rect.bottom(),
            ),
        );
    }
    let before_input = query("stats")?;
    let memory_before = memory_profile(daemon_pid)?;
    if control {
        // Match the nominal duration of the same 128-chord resize workload,
        // including its ten-second idle tail, without sending shortcut input.
        let duration = Duration::from_millis((0..128).map(|index| 95 + index * 37 % 101).sum());
        thread::sleep(duration);
        let active = query("stats")?;
        let memory_after = memory_profile(daemon_pid)?;
        let started = Instant::now();
        let before_cpu = cpu(daemon_pid)?;
        thread::sleep(Duration::from_secs(10));
        let idle_cpu =
            (cpu(daemon_pid)? - before_cpu) as f64 / 10_000_000.0 / started.elapsed().as_secs_f64()
                * 100.0;
        let idle = query("stats")?;
        let memory_idle = memory_profile(daemon_pid)?;
        let final_world: World = serde_json::from_value(query("world")?)?;
        if final_world.plan().placements != initial
            || !initial.iter().all(|p| platform::window_rect(p.id).is_ok_and(|rect| rect == p.rect))
        {
            return Err("no-input control layout changed or native frames did not match".into());
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
            "mode":"no-input-control","windows":8,"chords":0,"unchanged_exact_frames":true,"nominal_workload_seconds":duration.as_secs_f64(),
                "before_input":before_input,"after_input":active,"after_idle":idle,"idle_cpu_one_core_percent":idle_cpu,
                "memory_before":memory_before,"memory_after_input":memory_after,"memory_after_idle":memory_idle,
            }))?
        );
        return Ok(());
    }
    let mut external_latency = Vec::new();
    let mut chord_latency = Vec::new();
    let mut poll_waits = Vec::new();
    let mut native_latency = Vec::new();
    #[derive(Default, serde::Serialize)]
    struct ObserverTrial {
        trial: usize,
        trigger_inject_us: u64,
        native: ReadbackTiming,
        dwm_frame: ReadbackTiming,
        sleeps: u64,
        sleep_us: u64,
        native_placed_us: u64,
        external_placed_us: u64,
        whole_chord_us: u64,
    }
    #[derive(serde::Serialize)]
    struct TargetWindowTiming {
        window: usize,
        sequence_before: u64,
        sequence_after: u64,
        callback_us: Option<u64>,
    }
    #[derive(serde::Serialize)]
    struct TargetTrial {
        trial: usize,
        changed_windows: usize,
        latest_callback_us: u64,
        windows: Vec<TargetWindowTiming>,
    }
    // Exactly one fixed-size record per trial; never retain per-poll histories.
    let mut observer_trials = Vec::with_capacity(128);
    let mut target_trials = Vec::with_capacity(if target_timing { 128 } else { 0 });
    cleanup.injected = true;
    // Each key executes both width and height changes through the built-in binding.
    for index in 0..128 {
        let foreground = platform::foreground().ok_or("foreground left the helper")?;
        if platform::process_id(foreground) != helper_pid {
            return Err("foreground left the helper; input stopped".into());
        }
        let key = if index % 2 == 0 { VK_OEM_MINUS } else { VK_OEM_PLUS };
        let amount = if index % 2 == 0 { "-2%" } else { "+2%" };
        for axis in ["width", "height"] {
            world = moon_core::world::reduce(
                &world,
                moon_core::world::Event::Command(format!("resize {axis} {amount}").parse()?),
            )?
            .world;
        }
        let expected = world.plan().placements;
        let mut before_targets = BTreeMap::new();
        if target_timing {
            for placement in &expected {
                let stamp = placement_stamp(placement.id, helper_pid)?;
                let actual = native_rect(placement.id)?;
                if stamp.outer != [actual.x, actual.y, actual.right(), actual.bottom()] {
                    return Err("helper timestamp does not match its pre-trial actual frame".into());
                }
                before_targets.insert(placement.id, stamp);
            }
        }
        let mut observer = ObserverTrial { trial: index, ..Default::default() };
        let chord_started = Instant::now();
        if !inject(&[(VK_LMENU, false), (VK_LSHIFT, false)]) {
            return Err("modifier preparation was refused".into());
        }
        // The command starts at its trigger key, after its modifiers are held.
        // Retain whole-chord timings separately instead of hiding their cost.
        let key_started = Instant::now();
        let key_qpc = if target_timing { qpc()? } else { 0 };
        if !inject(&[(key, false), (key, true)]) {
            return Err("keyboard injection was refused".into());
        }
        observer.trigger_inject_us = key_started.elapsed().as_micros() as u64;
        let native_settled = |timing: &mut ReadbackTiming| {
            let calls_before = timing.calls;
            let settled = expected.iter().all(|p| {
                let (left, top, right, bottom) = frame_insets[&p.id];
                timing.measure(|| native_rect(p.id)).is_ok_and(|rect| {
                    rect == Rect::new(
                        p.rect.x - left,
                        p.rect.y - top,
                        p.rect.width + left + right,
                        p.rect.height + top + bottom,
                    )
                })
            });
            timing.finish_pass(expected.len(), calls_before);
            settled
        };
        let mut native_placed = None;
        loop {
            if native_placed.is_none() {
                if native_settled(&mut observer.native) {
                    native_placed = Some(key_started.elapsed().as_micros() as u64);
                }
            } else {
                observer.native.completed_pass_skips += 1;
            }
            let calls_before = observer.dwm_frame.calls;
            let frames_settled = expected.iter().all(|placement| {
                observer.dwm_frame.measure(|| platform::window_rect(placement.id)).is_ok_and(
                    |rect| {
                        (rect.x - placement.rect.x).abs() <= 1
                            && (rect.y - placement.rect.y).abs() <= 1
                            && (rect.width - placement.rect.width).abs() <= 1
                            && (rect.height - placement.rect.height).abs() <= 1
                    },
                )
            });
            observer.dwm_frame.finish_pass(expected.len(), calls_before);
            if frames_settled {
                // A move can finish between the native and DWM reads above.
                if native_placed.is_none() {
                    if !native_settled(&mut observer.native) {
                        return Err("DWM frames settled without exact native outer bounds".into());
                    }
                    native_placed = Some(key_started.elapsed().as_micros() as u64);
                }
                break;
            }
            if key_started.elapsed() >= Duration::from_secs(2) {
                eprintln!(
                    "failure diagnostics: {}",
                    serde_json::to_string(&json!({
                        "trial":index,"stats":query("stats").ok(),"world":query("world").ok(),
                        "expected":expected,"actual":expected.iter().map(|p| (p.id, platform::window_rect(p.id).ok())).collect::<Vec<_>>()
                    }))?
                );
                return Err(format!(
                    "actual frames did not reach the expected resize in trial {index}"
                )
                .into());
            }
            let wait_started = Instant::now();
            thread::sleep(Duration::from_millis(1));
            poll_waits.push(wait_started.elapsed().as_micros() as u64);
            observer.sleeps += 1;
            observer.sleep_us += poll_waits.last().copied().unwrap_or_default();
        }
        external_latency.push(key_started.elapsed().as_micros() as u64);
        native_latency
            .push(native_placed.ok_or("DWM frames settled without exact native outer bounds")?);
        chord_latency.push(chord_started.elapsed().as_micros() as u64);
        if !inject(&[(VK_LSHIFT, true), (VK_LMENU, true)]) {
            return Err("modifier release was refused".into());
        }
        // Read helper metadata only after every existing completion timer has
        // been recorded. No diagnostic query is part of the polling loop.
        if target_timing {
            let mut trial = TargetTrial {
                trial: index,
                changed_windows: 0,
                latest_callback_us: 0,
                windows: Vec::with_capacity(expected.len()),
            };
            for (window, placement) in expected.iter().enumerate() {
                let before = before_targets[&placement.id];
                let after = placement_stamp(placement.id, helper_pid)?;
                let (left, top, right, bottom) = frame_insets[&placement.id];
                let outer = [
                    placement.rect.x - left,
                    placement.rect.y - top,
                    placement.rect.right() + right,
                    placement.rect.bottom() + bottom,
                ];
                let callback_us = changed_target_ticks(before, after, outer, key_qpc)?
                    .map(|ticks| (u128::from(ticks) * 1_000_000 / qpc_frequency as u128) as u64);
                if let Some(elapsed) = callback_us {
                    trial.changed_windows += 1;
                    trial.latest_callback_us = trial.latest_callback_us.max(elapsed);
                }
                trial.windows.push(TargetWindowTiming {
                    window,
                    sequence_before: before.sequence,
                    sequence_after: after.sequence,
                    callback_us,
                });
            }
            if trial.changed_windows == 0 {
                return Err("resize diagnostic had no changed target windows".into());
            }
            target_trials.push(trial);
        }
        observer.native_placed_us = native_latency[index];
        observer.external_placed_us = external_latency[index];
        observer.whole_chord_us = chord_latency[index];
        observer_trials.push(observer);
        // Spread samples across display-refresh phases; a fixed 150 ms interval
        // accidentally synchronizes with exactly nine frames on a 60 Hz display.
        let interval = Duration::from_millis((95 + (index * 37 % 101)) as u64);
        thread::sleep(interval.saturating_sub(chord_started.elapsed()));
    }
    let active = query("stats")?;
    let memory_after = memory_profile(daemon_pid)?;
    let mut latency: Vec<u64> = serde_json::from_value(active["key_to_placed_us"].clone())?;
    if latency.len() < 128 {
        return Err(format!("only {} of 128 chords completed placement", latency.len()).into());
    }
    latency.sort_unstable();
    external_latency.sort_unstable();
    chord_latency.sort_unstable();
    poll_waits.sort_unstable();
    native_latency.sort_unstable();
    let started = Instant::now();
    let before_cpu = cpu(daemon_pid)?;
    thread::sleep(Duration::from_secs(10));
    let idle_cpu =
        (cpu(daemon_pid)? - before_cpu) as f64 / 10_000_000.0 / started.elapsed().as_secs_f64()
            * 100.0;
    let idle = query("stats")?;
    let memory_idle = memory_profile(daemon_pid)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "windows":8,"chords":128,"key_to_placed_median_us":latency[latency.len()/2],
            "key_to_placed_p99_us":latency[(latency.len()*99).div_ceil(100)-1],
        "external_key_to_placed_median_us":external_latency[external_latency.len()/2],
        "external_key_to_placed_p99_us":external_latency[(external_latency.len()*99).div_ceil(100)-1],
        "external_key_to_placed_us":external_latency,
        "whole_chord_to_placed_median_us":chord_latency[chord_latency.len()/2],
        "whole_chord_to_placed_p99_us":chord_latency[(chord_latency.len()*99).div_ceil(100)-1],
        "poll_wait_us":poll_waits,
        "native_outer_placed_us":native_latency,
        "observer_trials":observer_trials,
        "target_placement_enabled":target_timing,
        "target_placement_qpc_frequency":qpc_frequency,
        "target_placement_trials":target_trials,
        "target_placement_scope":"Opt-in MOON_PERF_PLACEMENT_TIMING=1. Owned testwin publishes actual outer rectangle and same-system QPC after WM_WINDOWPOSCHANGED default handling, an upper bound on geometry change, not compositor visibility. Stable odd/even sequence and process ownership are checked. Exact unchanged targets contribute no timestamp; changed targets require a fresh sequence and QPC at or after trigger (one-tick cross-thread uncertainty). Metadata reads occur before trial timing or after all existing completion timestamps; only the QPC start read is added inside the existing trigger interval. Helper publication overhead remains real work; these diagnostic timings do not replace acceptance metrics.",
        "observer_scope":"128 fixed-size records in trial order. Readback total/max nanoseconds time only the existing native_rect/platform::window_rect wrappers (the latter retains its DWM-to-GetWindowRect fallback); elapsed time may include descheduling. Trigger microseconds include the existing inject helper. Short-circuit skips count unqueried targets within attempted passes; completed-pass skips count whole native passes omitted after native completion. Input/read order, deadlines, polling and acceptance timing boundaries are unchanged; added measurement overhead remains included. This diagnostic is not an uninstrumented budget comparison.",
        "hook_phase_timings":active["hooks"]["phases"],
        "hook_phase_scope":"Cumulative elapsed microseconds through after_input; diagnostic phases remain included in the unchanged own/whole callback metrics. Percentiles are not additive; 4096 is the terminal bucket lower bound.",
        "idle_cpu_one_core_percent":idle_cpu,"before_input":before_input,"after_input":active,"after_idle":idle,
        "memory_before":memory_before,"memory_after_input":memory_after,"memory_after_idle":memory_idle,
        }))?
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("daemon_performance requires Windows 11");
}

#[cfg(test)]
mod tests {
    use super::{PlacementStamp, ReadbackTiming, changed_target_ticks};

    #[test]
    fn readback_timing_preserves_short_circuit_and_read_errors() {
        let mut timing = ReadbackTiming::default();
        let mut visited = Vec::new();
        let before = timing.calls;
        let settled = (0..8).all(|index| {
            timing
                .measure(|| {
                    visited.push(index);
                    if index == 2 { Err("unavailable") } else { Ok(true) }
                })
                .is_ok_and(|exact| exact)
        });
        timing.finish_pass(8, before);
        assert!(!settled);
        assert_eq!(visited, [0, 1, 2]);
        assert_eq!(timing.calls + timing.short_circuit_skips, 8);
        assert_eq!(timing.short_circuit_skips, 5);
        let before = timing.calls;
        assert!((0..8).all(|_| timing.measure(|| true)));
        timing.finish_pass(8, before);
        assert_eq!(timing.passes, 2);
        assert_eq!(timing.calls, 11);
        assert_eq!(timing.short_circuit_skips, 5);
        assert_eq!(timing.completed_pass_skips, 0);
        assert!(timing.max_ns <= timing.total_ns);
    }

    #[test]
    fn unchanged_targets_do_not_contribute_old_callback_times() {
        let old = PlacementStamp { sequence: 2, qpc: 50, outer: [-400, 20, 0, 320] };
        assert_eq!(changed_target_ticks(old, old, old.outer, 1000), Ok(None));
    }

    #[test]
    fn changed_targets_require_exact_fresh_consistent_snapshots() {
        let before = PlacementStamp { sequence: 2, qpc: 50, outer: [-400, 20, 0, 320] };
        let expected = [-400, 20, 50, 350];
        let after = PlacementStamp { sequence: 4, qpc: 1100, outer: expected };
        assert_eq!(changed_target_ticks(before, after, expected, 1000), Ok(Some(100)));
        assert!(changed_target_ticks(before, before, expected, 1000).is_err());
        assert!(
            changed_target_ticks(before, PlacementStamp { sequence: 2, ..after }, expected, 1000)
                .is_err()
        );
        assert!(
            changed_target_ticks(before, PlacementStamp { sequence: 3, ..after }, expected, 1000)
                .is_err()
        );
        assert!(
            changed_target_ticks(before, PlacementStamp { qpc: 998, ..after }, expected, 1000)
                .is_err()
        );
        assert_eq!(
            changed_target_ticks(before, PlacementStamp { qpc: 999, ..after }, expected, 1000),
            Ok(Some(0))
        );
    }
}
