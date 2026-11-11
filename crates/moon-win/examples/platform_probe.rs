//! Runs destructive-to-test-windows-only Win32 capability checks.
//! Usage: platform_probe.exe C:\path\testwin.exe
#[cfg(windows)]
mod native {
    use moon_core::geometry::Rect;
    use moon_win::{hooks, ipc, platform};
    use serde_json::json;
    use std::{
        process::{Child, Command},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };

    struct TestWindows(Child);
    impl Drop for TestWindows {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn low_client() -> Result<(), Box<dyn std::error::Error>> {
        use windows::{
            Win32::{
                Foundation::*,
                Security::{Authorization::*, *},
                System::Threading::*,
            },
            core::PCWSTR,
        };
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_ADJUST_DEFAULT, &mut token)?;
            let sid = platform::wide("S-1-16-4096");
            let mut pointer = PSID::default();
            ConvertStringSidToSidW(PCWSTR(sid.as_ptr()), &mut pointer)?;
            let label = TOKEN_MANDATORY_LABEL {
                Label: SID_AND_ATTRIBUTES { Sid: pointer, Attributes: 0x20 },
            };
            let result = SetTokenInformation(
                token,
                TokenIntegrityLevel,
                &label as *const _ as *const _,
                std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32 + GetLengthSid(pointer),
            );
            let _ = LocalFree(Some(HLOCAL(pointer.0)));
            let _ = CloseHandle(token);
            result?;
        }
        match ipc::request(&json!({"method":"query"})) {
            Err(error) if error.code() == windows::Win32::Foundation::E_ACCESSDENIED => {
                println!("low-integrity-client-refused");
                Ok(())
            }
            result => Err(format!(
                "Low integrity pipe request was not refused with access denied: {result:?}"
            )
            .into()),
        }
    }

    fn run() -> Result<(), Box<dyn std::error::Error>> {
        if std::env::args().nth(1).as_deref() == Some("--low-client") {
            return low_client();
        }
        let testwin = std::env::args().nth(1).ok_or("Expected testwin.exe path")?;
        let _apartment = platform::Apartment::new()?;
        struct RestoreFocus(Option<moon_core::ids::WindowId>);
        impl Drop for RestoreFocus {
            fn drop(&mut self) {
                if let Some(id) = self.0 {
                    let _ = platform::focus(id);
                }
            }
        }
        let _focus = RestoreFocus(platform::foreground());
        let _ = platform::enable_dpi_awareness();
        let monitors = platform::monitors()?;
        let area = monitors.first().ok_or("No monitor")?.work_area;
        let child = TestWindows(Command::new(testwin).arg("8").spawn()?);
        let deadline = Instant::now() + Duration::from_secs(10);
        let windows = loop {
            if Instant::now() > deadline {
                return Err("testwin did not create eight manageable windows".into());
            }
            let windows: Vec<_> = platform::enumerate_windows()?
                .into_iter()
                .filter(|w| w.process_id == child.0.id())
                .collect();
            if windows.len() == 8 {
                break windows;
            }
            thread::sleep(Duration::from_millis(50));
        };
        let expected: Vec<_> = windows
            .iter()
            .enumerate()
            .map(|(index, window)| {
                let width = (area.width - 80) / 4;
                let height = (area.height - 48) / 2;
                (
                    window.id,
                    Rect::new(
                        area.x + 16 + (index as i32 % 4) * (width + 16),
                        area.y + 16 + (index as i32 / 4) * (height + 16),
                        width,
                        height,
                    ),
                )
            })
            .collect();
        let start = Instant::now();
        for &(id, rect) in &expected {
            platform::place(id, rect)?;
        }
        for _ in 0..100 {
            if expected
                .iter()
                .all(|&(id, target)| platform::window_rect(id).is_ok_and(|r| r == target))
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let elapsed = start.elapsed().as_micros();
        for &(id, target) in &expected {
            let actual = platform::window_rect(id)?;
            if actual != target {
                return Err(format!("Placement mismatch: {actual:?} != {target:?}").into());
            }
        }
        let method = platform::set_hidden(windows[0].id, true)?;
        let cloaked = platform::cloaked(windows[0].id)?;
        platform::set_hidden(windows[0].id, false)?;
        platform::place(expected[0].0, expected[0].1)?;
        if platform::cloaked(windows[0].id)? != 0 {
            return Err("Uncloak verification failed".into());
        }
        let mut focus_pass = 0;
        for index in 0..100 {
            let id = windows[index % windows.len()].id;
            if platform::focus(id).is_ok() && platform::foreground() == Some(id) {
                focus_pass += 1;
            }
        }
        let (event_sender, event_receiver) = mpsc::channel();
        let hook = hooks::Hooks::start(
            vec![
                hooks::HookBinding { chord: "alt+h".into(), commands: vec!["probe-h".into()] },
                hooks::HookBinding {
                    chord: "alt+shift+1".into(),
                    commands: vec!["probe-one".into()],
                },
                hooks::HookBinding { chord: "alt+tab".into(), commands: vec!["probe-tab".into()] },
            ],
            event_sender,
            true,
        )?;
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        fn inject(keys: &[(VIRTUAL_KEY, bool)]) {
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
            unsafe {
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            }
        }
        struct ReleaseModifiers;
        impl Drop for ReleaseModifiers {
            fn drop(&mut self) {
                inject(&[(VK_LSHIFT, true), (VK_LMENU, true)]);
            }
        }
        let _release = ReleaseModifiers;
        let mut matched = 0;
        for _ in 0..10 {
            for (key, shift, command) in [
                (VIRTUAL_KEY(b'H' as u16), false, "probe-h"),
                (VIRTUAL_KEY(b'1' as u16), true, "probe-one"),
                (VK_TAB, false, "probe-tab"),
            ] {
                platform::focus(windows[0].id)?;
                inject(&[(VK_LMENU, false)]);
                if shift {
                    inject(&[(VK_LSHIFT, false)]);
                }
                inject(&[(key, false), (key, true)]);
                let deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    if Instant::now() > deadline {
                        return Err(format!("Hotkey did not match {command}").into());
                    }
                    if let Ok(hooks::PlatformEvent::Commands { commands, .. }) =
                        event_receiver.recv_timeout(Duration::from_millis(10))
                        && commands.first().is_some_and(|value| value == command)
                    {
                        matched += 1;
                        break;
                    }
                }
                hooks::suppress_alt_menu();
                if shift {
                    inject(&[(VK_LSHIFT, true)]);
                }
                inject(&[(VK_LMENU, true)]);
                thread::sleep(Duration::from_millis(5));
                if platform::foreground() != Some(windows[0].id) {
                    return Err("Bound Alt chord escaped into the system task switcher".into());
                }
            }
        }
        let hook_stats = hooks::stats();
        drop(hook);
        for window in &windows {
            platform::restore(window.id, window.rect)?;
        }

        let (sender, receiver) = mpsc::channel();
        let server = ipc::PipeServer::start(sender)?;
        let worker = thread::spawn(move || {
            while let Ok(request) = receiver.recv() {
                let _ = request.reply.try_send(json!({"ok":true,"echo":request.message}));
            }
        });
        let response = ipc::request(&json!({"method":"query","text":"hello"}))?;
        if response["echo"]["text"] != "hello" {
            return Err("Pipe round trip failed".into());
        }
        let (second_sender, _) = mpsc::channel();
        if ipc::PipeServer::start(second_sender).is_ok() {
            return Err("Second pipe server was accepted".into());
        }
        let low = Command::new(std::env::current_exe()?).arg("--low-client").output()?;
        if !low.status.success() {
            return Err(format!(
                "Low-integrity test failed: {} {}",
                String::from_utf8_lossy(&low.stdout),
                String::from_utf8_lossy(&low.stderr)
            )
            .into());
        }
        drop(server);
        let _ = worker.join();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"monitor_count":monitors.len(),"test_windows":windows.len(),"placement_exact":true,"placement_us":elapsed,"hide_method":method,"cloaked_attribute":cloaked,"focus_successes":focus_pass,"focus_trials":100,"hook_chords_matched":matched,"hook":hook_stats,"ipc_round_trip":true,"second_server_refused":true,"low_integrity_refused":true})
            )?
        );
        Ok(())
    }
    pub fn main() {
        if let Err(error) = run() {
            eprintln!("platform-probe: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn main() {
    native::main();
}
#[cfg(not(windows))]
fn main() {
    eprintln!("The platform probe requires Windows 11.");
}
