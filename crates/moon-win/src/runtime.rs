//! Process lifetime and shell services. All Windows handles have an owning guard.
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HANDLE},
        Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
        System::{
            Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance, IServiceProvider},
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::{
                CreateMutexW, GR_GDIOBJECTS, GetCurrentProcess, GetCurrentThreadId,
                GetGuiResources, GetProcessHandleCount, GetProcessTimes, INFINITE, OpenProcess,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, WaitForSingleObject,
            },
            Variant::VARIANT,
        },
        UI::{
            Shell::{
                IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellWindows,
                SID_STopLevelBrowser, SVGIO_BACKGROUND, SWC_DESKTOP, SWFO_NEEDDISPATCH,
                ShellWindows,
            },
            WindowsAndMessaging::{
                DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
                PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT,
                TranslateMessage, WM_APP, WM_QUIT,
            },
        },
    },
    core::{BSTR, Interface, PCWSTR, Result},
};

fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value.encode_wide().chain(Some(0)).collect()
}

pub struct Process(HANDLE);
impl Process {
    pub fn open(pid: u32) -> Result<Self> {
        unsafe {
            OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .map(Self)
        }
    }
    pub fn wait(&self) {
        unsafe {
            WaitForSingleObject(self.0, INFINITE);
        }
    }
    pub fn birth(&self) -> Result<u64> {
        let (mut created, mut exited, mut kernel, mut user) =
            (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe {
            GetProcessTimes(self.0, &mut created, &mut exited, &mut kernel, &mut user)?;
        }
        Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
    pub fn is_running(&self) -> Result<bool> {
        use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        match unsafe { WaitForSingleObject(self.0, 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            _ => Err(windows::core::Error::from_thread()),
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub fn process_birth(pid: u32) -> Result<u64> {
    Process::open(pid)?.birth()
}

/// Query failures are not evidence that a live recovery target disappeared.
pub fn process_matches(pid: u32, birth: u64) -> Result<bool> {
    match Process::open(pid) {
        Ok(process) => Ok(process.is_running()? && process.birth()? == birth),
        Err(error)
            if error.code() == windows::Win32::Foundation::ERROR_INVALID_PARAMETER.to_hresult() =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;
    use std::os::windows::process::CommandExt;

    #[test]
    fn exited_process_is_not_a_live_owner_while_its_handle_is_retained() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "exit 0"])
            .current_dir(std::env::temp_dir())
            .creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0)
            .spawn()
            .unwrap();
        let process = Process::open(child.id()).unwrap();
        let birth = process.birth().unwrap();
        assert!(child.wait().unwrap().success());
        assert!(!process.is_running().unwrap());
        assert!(!process_matches(child.id(), birth).unwrap());
        // Both handles remain owned here, reproducing a debugger or test harness.
        assert_eq!(process.birth().unwrap(), birth);
    }
}

#[cfg(test)]
mod atomic_replace_tests {
    use super::atomic_replace;
    use std::{
        fs::{self, File, OpenOptions},
        os::windows::fs::OpenOptionsExt,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc,
        },
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use windows::Win32::{
        Foundation::{ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION},
        Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE},
    };

    const OLD: &[u8] = b"previous durable journal\n";
    const NEW: &[u8] = b"complete replacement journal with new state\n";
    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Files {
        directory: PathBuf,
        source: PathBuf,
        destination: PathBuf,
    }
    impl Files {
        fn new() -> Self {
            let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir()
                .join(format!("moon-replace-test-{}-{unique}-{sequence}", std::process::id()));
            fs::create_dir(&directory).unwrap();
            let files = Self {
                source: directory.join("next.tmp"),
                destination: directory.join("journal.json"),
                directory,
            };
            fs::write(&files.source, NEW).unwrap();
            fs::write(&files.destination, OLD).unwrap();
            files
        }
        fn reader_without_delete_sharing(&self) -> File {
            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
                .open(&self.destination)
                .unwrap()
        }
    }
    impl Drop for Files {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn atomic_replace_waits_for_a_brief_reader_without_delete_sharing() {
        let files = Files::new();
        let reader = files.reader_without_delete_sharing();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            drop(reader);
        });
        let started = Instant::now();
        let replaced = atomic_replace(&files.source, &files.destination);
        let elapsed = started.elapsed();
        release.join().unwrap();
        replaced.unwrap();
        assert_eq!(fs::read(&files.destination).unwrap(), NEW);
        assert!(!files.source.exists());
        eprintln!("replacement after brief reader: {:.3} ms", elapsed.as_secs_f64() * 1000.0);
    }

    #[test]
    fn atomic_replace_bounds_a_persistent_reader_and_preserves_both_files() {
        let files = Files::new();
        let reader = files.reader_without_delete_sharing();
        let (send, receive) = mpsc::sync_channel(1);
        thread::scope(|scope| {
            scope.spawn(|| {
                let started = Instant::now();
                let result = atomic_replace(&files.source, &files.destination);
                send.send((result, started.elapsed())).unwrap();
            });
            let received = receive.recv_timeout(Duration::from_secs(2));
            if received.is_err() {
                // Unblock an accidentally unbounded implementation before the
                // scoped join, so the regression reports failure without hanging.
                drop(reader);
                panic!("atomic_replace did not return within its bounded retry window");
            }
            let (result, elapsed) = received.unwrap();
            let error = result.expect_err("a reader still denying deletion must block replacement");
            assert!(
                [ERROR_ACCESS_DENIED.to_hresult(), ERROR_SHARING_VIOLATION.to_hresult(),]
                    .contains(&error.code())
            );
            assert!(elapsed >= Duration::from_millis(450), "retry window was skipped: {elapsed:?}");
            assert!(elapsed < Duration::from_secs(2), "retry window exceeded: {elapsed:?}");
            assert_eq!(fs::read(&files.destination).unwrap(), OLD);
            assert_eq!(fs::read(&files.source).unwrap(), NEW);
            drop(reader);
            eprintln!(
                "bounded persistent-reader failure: {:.3} ms",
                elapsed.as_secs_f64() * 1000.0
            );
        });
    }
}

/// Internal children execute the same trusted binary. Application launches use
/// the Explorer broker instead, so they never inherit the manager's uiAccess.
pub fn internal_command() -> std::io::Result<std::process::Command> {
    use std::os::windows::process::CommandExt;
    use windows::Win32::{
        Foundation::{
            ERROR_INVALID_HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation,
        },
        System::Console::{GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE},
    };
    // Rust's stable Windows Command inherits every inheritable handle, including
    // redirected parent streams that are not the child's selected stdio. Clear
    // inheritance on our original streams; Command duplicates only the explicit
    // child stdio with inheritance enabled. Otherwise `start --background` keeps
    // its caller's output pipe open for the daemon's entire lifetime.
    for stream in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        unsafe {
            if let Ok(handle) = GetStdHandle(stream)
                && !handle.is_invalid()
                && let Err(error) =
                    SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0))
                && error.code() != ERROR_INVALID_HANDLE.to_hresult()
            {
                return Err(std::io::Error::other(error));
            }
        }
    }
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command.creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
    Ok(command)
}

/// ShellExecute cannot inherit the explicit stdio selected on Command. Only
/// our private child entrypoint consumes this mode, before config/startup work.
#[derive(Clone, Copy, Debug)]
pub enum InternalStdio {
    Log,
    Null,
}
impl InternalStdio {
    fn argument(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Null => "null",
        }
    }
}

enum ChildHandle {
    Direct(std::process::Child),
    Broker { process: Process, id: u32 },
}
pub struct InternalChild(ChildHandle);
impl InternalChild {
    pub fn id(&self) -> u32 {
        match &self.0 {
            ChildHandle::Direct(child) => child.id(),
            ChildHandle::Broker { id, .. } => *id,
        }
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        match &mut self.0 {
            ChildHandle::Direct(child) => child.try_wait(),
            ChildHandle::Broker { process, .. } => {
                if process.is_running().map_err(std::io::Error::other)? {
                    return Ok(None);
                }
                use std::os::windows::process::ExitStatusExt;
                let mut code = 0;
                unsafe {
                    windows::Win32::System::Threading::GetExitCodeProcess(process.0, &mut code)
                        .map_err(std::io::Error::other)?;
                }
                // Wait state, not STILL_ACTIVE(259), determines whether it exited.
                Ok(Some(std::process::ExitStatus::from_raw(code)))
            }
        }
    }
    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        match &mut self.0 {
            ChildHandle::Direct(child) => child.wait(),
            ChildHandle::Broker { process, .. } => {
                if unsafe { WaitForSingleObject(process.0, INFINITE) }
                    != windows::Win32::Foundation::WAIT_OBJECT_0
                {
                    return Err(std::io::Error::last_os_error());
                }
                self.try_wait()?.ok_or_else(|| std::io::Error::other("Child did not exit"))
            }
        }
    }
    pub fn kill(&mut self) -> std::io::Result<()> {
        match &mut self.0 {
            ChildHandle::Direct(child) => child.kill(),
            ChildHandle::Broker { process, .. } => {
                if !process.is_running().map_err(std::io::Error::other)? {
                    return Ok(());
                }
                if let Err(error) =
                    unsafe { windows::Win32::System::Threading::TerminateProcess(process.0, 1) }
                    && process.is_running().map_err(std::io::Error::other)?
                {
                    return Err(std::io::Error::other(error));
                }
                Ok(())
            }
        }
    }
}

fn shell_parameters(arguments: &[std::ffi::OsString]) -> std::io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let mut result = Vec::new();
    for (index, argument) in arguments.iter().enumerate() {
        if index != 0 {
            result.push(32);
        }
        result.push(34);
        let mut slashes = 0;
        for unit in argument.encode_wide() {
            match unit {
                0 => return Err(std::io::Error::other("Internal argument contains a NUL")),
                92 => slashes += 1,
                34 => {
                    result.extend(std::iter::repeat_n(92, slashes * 2 + 1));
                    result.push(34);
                    slashes = 0;
                }
                _ => {
                    result.extend(std::iter::repeat_n(92, slashes));
                    slashes = 0;
                    result.push(unit);
                }
            }
        }
        result.extend(std::iter::repeat_n(92, slashes * 2));
        result.push(34);
    }
    result.push(0);
    Ok(result)
}

fn spawn_brokered(file: &Path, arguments: &[std::ffi::OsString]) -> std::io::Result<InternalChild> {
    use windows::Win32::UI::{
        Shell::{
            SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
            ShellExecuteExW,
        },
        WindowsAndMessaging::SW_HIDE,
    };
    let _apartment = crate::platform::Apartment::new().map_err(std::io::Error::other)?;
    let file = wide(file.as_os_str());
    let parameters = shell_parameters(arguments)?;
    if file.len() + parameters.len() + 3 > 32767 {
        return Err(std::io::Error::other("Internal command exceeds the Windows argument limit"));
    }
    let mut launch = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: windows::core::w!("open"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut launch) }.map_err(std::io::Error::other)?;
    if launch.hProcess.is_invalid() {
        return Err(std::io::Error::other("Windows did not return the internal child process"));
    }
    // Take ownership before any fallible PID lookup; never reopen by a recycled PID.
    let process = Process(launch.hProcess);
    let id = unsafe { windows::Win32::System::Threading::GetProcessId(process.0) };
    if id == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(InternalChild(ChildHandle::Broker { process, id }))
}

/// Same-image children only. Ordinary launches retain Command's exact stdio,
/// flags and inheritance behavior. UIAccess must go through the Windows broker.
pub fn spawn_internal(
    command: &mut std::process::Command,
    stdio: InternalStdio,
) -> std::io::Result<InternalChild> {
    let image = std::env::current_exe()?;
    if command.get_program() != image.as_os_str() {
        return Err(std::io::Error::other("Internal children must execute the current image"));
    }
    let token =
        crate::platform::token_capabilities(std::process::id()).map_err(std::io::Error::other)?;
    if !token.ui_access {
        return command.spawn().map(|child| InternalChild(ChildHandle::Direct(child)));
    }
    if command.get_current_dir().is_some() || command.get_envs().next().is_some() {
        return Err(std::io::Error::other(
            "Brokered internal children cannot override environment or directory",
        ));
    }
    let mut arguments: Vec<_> = command.get_args().map(std::ffi::OsStr::to_owned).collect();
    arguments.extend(["--internal-stdio".into(), stdio.argument().into()]);
    spawn_brokered(&image, &arguments)
}

/// Owns replacements for the standard handles until main has printed any final
/// error. Rust 1.99's Windows stdio reads GetStdHandle on each read/write.
pub struct InternalStdioGuard {
    files: [std::fs::File; 3],
    previous: [HANDLE; 3],
    installed: usize,
}
impl Drop for InternalStdioGuard {
    fn drop(&mut self) {
        use std::io::Write;
        use windows::Win32::System::Console::{
            STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        };
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        for (index, stream) in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE]
            .into_iter()
            .enumerate()
            .take(self.installed)
        {
            unsafe {
                let _ = SetStdHandle(stream, self.previous[index]);
            }
        }
    }
}
pub fn redirect_internal_stdio(log: Option<&Path>) -> std::io::Result<InternalStdioGuard> {
    use std::{fs::OpenOptions, os::windows::io::AsRawHandle};
    use windows::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };
    let input = OpenOptions::new().read(true).open("NUL")?;
    let output = if let Some(path) = log {
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        OpenOptions::new().create(true).append(true).open(path)?
    } else {
        OpenOptions::new().write(true).open("NUL")?
    };
    let error = output.try_clone()?;
    let mut guard = InternalStdioGuard {
        files: [input, output, error],
        previous: [HANDLE::default(); 3],
        installed: 0,
    };
    for (index, stream) in
        [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE].into_iter().enumerate()
    {
        unsafe {
            guard.previous[index] = GetStdHandle(stream).unwrap_or_default();
            SetStdHandle(stream, HANDLE(guard.files[index].as_raw_handle()))
                .map_err(std::io::Error::other)?;
        }
        guard.installed += 1;
    }
    Ok(guard)
}

#[cfg(test)]
mod internal_child_tests {
    use super::*;
    use std::{
        ffi::{OsStr, OsString},
        os::windows::ffi::OsStringExt,
    };

    #[test]
    fn shell_arguments_roundtrip_spaces_quotes_unicode_and_trailing_slashes() {
        use windows::Win32::{
            Foundation::{HLOCAL, LocalFree},
            UI::Shell::CommandLineToArgvW,
        };
        let expected: Vec<OsString> = [
            "",
            "start",
            "--config",
            "C:\\a folder\\moon.toml",
            "quote\"and\\\"slash",
            "C:\\trailing\\",
            "日本語 & $literal",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        let mut line = wide(OsStr::new("fixture.exe"));
        line.pop();
        line.push(32);
        line.extend(shell_parameters(&expected).unwrap());
        let mut count = 0;
        let parsed = unsafe { CommandLineToArgvW(PCWSTR(line.as_ptr()), &mut count) };
        assert!(!parsed.is_null());
        let actual: Vec<OsString> = (1..count)
            .map(|i| unsafe { OsString::from_wide((*parsed.add(i as usize)).as_wide()) })
            .collect();
        unsafe {
            let _ = LocalFree(Some(HLOCAL(parsed.cast())));
        }
        assert_eq!(actual, expected);
        assert!(shell_parameters(&[OsString::from_wide(&[65, 0, 66])]).is_err());
    }

    struct OwnedTestChild(InternalChild);
    impl Drop for OwnedTestChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn await_exit(child: &mut InternalChild) -> std::process::ExitStatus {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            assert!(std::time::Instant::now() < deadline, "owned child did not exit");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn brokered_live_child_can_be_killed_and_waited_without_reopening_its_pid() {
        let image = Path::new(&std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut child = OwnedTestChild(
            spawn_brokered(
                &image,
                &[
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    "Start-Sleep -Seconds 30".into(),
                ],
            )
            .unwrap(),
        );
        assert_ne!(child.0.id(), 0);
        assert!(child.0.try_wait().unwrap().is_none());
        child.0.kill().unwrap();
        assert_eq!(await_exit(&mut child.0).code(), Some(1));
        assert_eq!(child.0.wait().unwrap().code(), Some(1));
    }

    #[test]
    fn brokered_child_retains_lifetime_and_exit_259_is_not_running() {
        // cmd.exe has its own command-string quoting rules; this fixture needs
        // ordinary argv parsing, like Moon, and an explicit full-width exit code.
        let image = Path::new(&std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut child = OwnedTestChild(
            spawn_brokered(
                &image,
                &[
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-Command".into(),
                    "exit 259".into(),
                ],
            )
            .unwrap(),
        );
        let id = child.0.id();
        assert_ne!(id, 0);
        assert_eq!(await_exit(&mut child.0).code(), Some(259));
        assert_eq!(child.0.wait().unwrap().code(), Some(259));
        assert_eq!(child.0.try_wait().unwrap().unwrap().code(), Some(259));
        child.0.kill().unwrap(); // An exited owned child is already terminated.
    }
}

pub fn session_id() -> Result<u32> {
    let mut session = 0;
    unsafe {
        windows::Win32::System::RemoteDesktop::ProcessIdToSessionId(
            std::process::id(),
            &mut session,
        )?;
    }
    Ok(session)
}

pub struct Instance(HANDLE);
impl Instance {
    pub fn acquire() -> Result<Self> {
        let name = wide(std::ffi::OsStr::new(&format!("Local\\moon-v1-{}", session_id()?)));
        unsafe {
            let handle = CreateMutexW(None, false, PCWSTR(name.as_ptr()))?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return Err(windows::core::Error::new(
                    ERROR_ALREADY_EXISTS.to_hresult(),
                    "moon is already running in this session",
                ));
            }
            Ok(Self(handle))
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn instance_is_running(error: &windows::core::Error) -> bool {
    error.code() == ERROR_ALREADY_EXISTS.to_hresult()
}

pub fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    let from = wide(from.as_os_str());
    let to = wide(to.as_os_str());
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    loop {
        let result = unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        match result {
            Err(error)
                if std::time::Instant::now() < deadline
                    && [
                        windows::Win32::Foundation::ERROR_ACCESS_DENIED.to_hresult(),
                        windows::Win32::Foundation::ERROR_SHARING_VIOLATION.to_hresult(),
                    ]
                    .contains(&error.code()) =>
            {
                // Antivirus and readers that omit FILE_SHARE_DELETE can briefly
                // hold the old inode. Keep the durable old journal until replace succeeds.
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => return result,
        }
    }
}

/// Ask the existing desktop shell to start the command with its ordinary user token.
/// Never fall back to a direct child: that could inherit uiAccess or elevation.
pub fn shell_launch(command: &str) -> Result<()> {
    shell_execute("cmd.exe", &format!("/d /s /c \"{command}\""))
}

pub fn shell_execute(file: &str, arguments: &str) -> Result<()> {
    unsafe {
        let shells: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
        let mut hwnd = 0;
        let dispatch = shells.FindWindowSW(
            &VARIANT::default(),
            &VARIANT::default(),
            SWC_DESKTOP,
            &mut hwnd,
            SWFO_NEEDDISPATCH,
        )?;
        let service: IServiceProvider = dispatch.cast()?;
        let browser: IShellBrowser = service.QueryService(&SID_STopLevelBrowser)?;
        let view = browser.QueryActiveShellView()?;
        let background: windows::Win32::System::Com::IDispatch =
            view.GetItemObject(SVGIO_BACKGROUND)?;
        let folder: IShellFolderViewDual = background.cast()?;
        let shell: IShellDispatch2 = folder.Application()?.cast()?;
        shell.ShellExecute(
            &BSTR::from(file),
            &VARIANT::from(arguments),
            &VARIANT::default(),
            &VARIANT::from("open"),
            &VARIANT::from(1i32),
        )
    }
}

pub fn message_thread() -> u32 {
    unsafe {
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        GetCurrentThreadId()
    }
}
pub fn wake(thread: u32) {
    unsafe {
        let _ = PostThreadMessageW(thread, WM_APP + 91, Default::default(), Default::default());
    }
}

/// One timer per second services clocks/config checks. Input wakes this immediately.
pub fn pump(wait_ms: u32) -> bool {
    unsafe {
        MsgWaitForMultipleObjectsEx(None, wait_ms, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    true
}

#[derive(serde::Serialize)]
pub struct ResourceStats {
    pub working_set_bytes: usize,
    pub handles: u32,
    pub gdi_objects: u32,
}
pub fn resource_stats() -> Result<ResourceStats> {
    unsafe {
        let process = GetCurrentProcess();
        let mut memory = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        GetProcessMemoryInfo(
            process,
            &mut memory,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )?;
        let mut handles = 0;
        GetProcessHandleCount(process, &mut handles)?;
        Ok(ResourceStats {
            working_set_bytes: memory.WorkingSetSize,
            handles,
            gdi_objects: GetGuiResources(process, GR_GDIOBJECTS),
        })
    }
}

static QUIT: AtomicBool = AtomicBool::new(false);
pub fn shutdown_requested() -> bool {
    QUIT.load(Ordering::Relaxed)
}

pub fn install_shutdown_handler(main_thread: u32) -> Result<()> {
    use std::sync::atomic::AtomicU32;
    static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);
    unsafe extern "system" fn handler(_: u32) -> windows::core::BOOL {
        QUIT.store(true, Ordering::Relaxed);
        wake(MAIN_THREAD.load(Ordering::Relaxed));
        true.into()
    }
    MAIN_THREAD.store(main_thread, Ordering::Relaxed);
    unsafe { windows::Win32::System::Console::SetConsoleCtrlHandler(Some(handler), true) }
}

/// A shell-launched medium-integrity worker joins a kill-on-close job before it
/// is allowed to execute the configured command. Output and lifetime are bounded.
pub fn capture_unprivileged(
    command: &str,
    timeout: std::time::Duration,
    max_bytes: usize,
) -> Result<String> {
    use std::{fs, io::Read, sync::atomic::AtomicU64, time::Instant};
    use windows::Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        System::{
            JobObjects::*,
            Threading::{PROCESS_SET_QUOTA, PROCESS_TERMINATE},
        },
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    fn failure(message: impl AsRef<str>) -> windows::core::Error {
        windows::core::Error::new(E_FAIL, message)
    }
    if max_bytes == 0 || max_bytes > 1_048_576 || timeout > std::time::Duration::from_secs(60) {
        return Err(windows::core::Error::new(
            E_INVALIDARG,
            "invalid exec output or timeout bound",
        ));
    }
    let directory = std::env::temp_dir().join(format!(
        "moon-exec-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).map_err(|e| failure(e.to_string()))?;
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _directory = Directory(directory.clone());
    let script = directory.join("capture.ps1");
    fs::write(directory.join("command.txt"), command).map_err(|e| failure(e.to_string()))?;
    fs::write(
        &script,
        r#"$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
[IO.File]::WriteAllText((Join-Path $root 'pid'), [string]$PID)
$deadline = [DateTime]::UtcNow.AddSeconds(10)
while (-not (Test-Path -LiteralPath (Join-Path $root 'go'))) {
    if ([DateTime]::UtcNow -gt $deadline) { exit 124 }
    Start-Sleep -Milliseconds 20
}
$command = [IO.File]::ReadAllText((Join-Path $root 'command.txt'))
$output = Join-Path $root 'stdout'
$errors = Join-Path $root 'stderr'
& $env:ComSpec /d /s /c "chcp 65001 >nul & ($command) 1>`"$output`" 2>`"$errors`""
[IO.File]::WriteAllText((Join-Path $root 'done'), [string]$LASTEXITCODE)
"#,
    )
    .map_err(|e| failure(e.to_string()))?;
    shell_execute(
        "powershell.exe",
        &format!(
            "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File \"{}\"",
            script.display()
        ),
    )?;
    let start = Instant::now();
    let pid = loop {
        if let Ok(text) = fs::read_to_string(directory.join("pid"))
            && let Ok(pid) = text.parse::<u32>()
        {
            break pid;
        }
        if start.elapsed() > std::time::Duration::from_secs(10) {
            return Err(failure("exec shell worker did not start"));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    struct Job(HANDLE, Option<HANDLE>);
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
                if let Some(worker) = self.1 {
                    WaitForSingleObject(worker, 1000);
                    let _ = CloseHandle(worker);
                }
            }
        }
    }
    let mut job = unsafe { Job(CreateJobObjectW(None, None)?, None) };
    unsafe {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            std::mem::size_of_val(&limits) as u32,
        )?;
        let worker =
            OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid)?;
        job.1 = Some(worker);
        AssignProcessToJobObject(job.0, worker)?;
    }
    fs::write(directory.join("go"), []).map_err(|e| failure(e.to_string()))?;
    let start = Instant::now();
    loop {
        if start.elapsed() > timeout {
            return Err(failure("exec widget timed out"));
        }
        for name in ["stdout", "stderr"] {
            if fs::metadata(directory.join(name)).is_ok_and(|m| m.len() > max_bytes as u64) {
                return Err(failure("exec widget exceeded output limit"));
            }
        }
        if let Ok(code) = fs::read_to_string(directory.join("done")) {
            if code.trim() != "0" {
                return Err(failure(format!("exec widget exited with {}", code.trim())));
            }
            let mut bytes = Vec::new();
            fs::File::open(directory.join("stdout"))
                .map_err(|e| failure(e.to_string()))?
                .take(max_bytes as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| failure(e.to_string()))?;
            if bytes.len() > max_bytes {
                return Err(failure("exec widget exceeded output limit"));
            }
            return Ok(String::from_utf8_lossy(&bytes).trim().to_owned());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
