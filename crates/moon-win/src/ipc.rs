//! Local-session NDJSON transport with a logon-SID ACL and medium integrity floor.
//! Reads, writes, connections, request size and concurrent clients are bounded.

use crate::platform::wide;
use serde_json::Value;
use std::{
    mem::size_of,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Sender, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::{Com::CoTaskMemFree, IO::*, Pipes::*, Threading::*},
        UI::Shell::{FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    },
    core::{Error, PCWSTR, PWSTR, Result},
};

pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_CLIENTS: u32 = 16;

struct Handle(HANDLE);
impl Handle {
    fn raw(&self) -> HANDLE {
        self.0
    }
}
// A kernel handle may be used by independent threads. Each owned handle closes once.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Drop for Handle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn failure(message: impl AsRef<str>) -> Error {
    Error::new(E_FAIL, message.as_ref())
}
fn event() -> Result<Handle> {
    unsafe { CreateEventW(None, true, false, None).map(Handle) }
}

fn identity() -> Result<(String, String)> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
        let token = Handle(token);
        identity_for_token(token.0)
    }
}

fn identity_for_token(token: HANDLE) -> Result<(String, String)> {
    unsafe {
        let mut length = 0;
        let _ = GetTokenInformation(token, TokenLogonSid, None, 0, &mut length);
        let mut groups = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        GetTokenInformation(
            token,
            TokenLogonSid,
            Some(groups.as_mut_ptr().cast()),
            length,
            &mut length,
        )?;
        let groups = &*(groups.as_ptr() as *const TOKEN_GROUPS);
        if groups.GroupCount != 1 {
            return Err(failure("Expected a single Windows logon SID"));
        }
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(groups.Groups[0].Sid, &mut text)?;
        let sid = text.to_string();
        let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        let sid = sid?;
        // UAC's linked administrator and filtered tokens can have different
        // AuthenticationIds. Their shared logon SID is also our pipe ACL boundary.
        Ok((format!(r"\\.\pipe\moon-v1-{sid}"), sid))
    }
}

pub fn pipe_name() -> Result<String> {
    identity().map(|value| value.0)
}

fn create_pipe(name: &str, sid: &str) -> Result<Handle> {
    // No inherited ACEs. Low integrity clients can neither read nor write up.
    let descriptor =
        wide(&format!("D:P(A;;GA;;;SY)(A;;RC;;;OW)(A;;GRGW;;;{sid})S:(ML;;NRNW;;;ME)"));
    unsafe {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(descriptor.as_ptr()),
            1,
            &mut sd,
            None,
        )?;
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        let name = wide(name);
        let flags = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE;
        let handle = CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            5000,
            Some(&security),
        );
        let error = Error::from_thread();
        let _ = LocalFree(Some(HLOCAL(sd.0)));
        if handle.is_invalid() { Err(error) } else { Ok(Handle(handle)) }
    }
}

fn finish_io(
    handle: HANDLE,
    overlapped: &OVERLAPPED,
    started: Result<()>,
    shutdown: Option<HANDLE>,
    timeout: u32,
) -> Result<u32> {
    unsafe {
        if let Err(error) = started {
            if error.code() != ERROR_IO_PENDING.to_hresult() {
                return Err(error);
            }
            let signaled = if let Some(shutdown) = shutdown {
                WaitForMultipleObjects(&[overlapped.hEvent, shutdown], false, timeout)
            } else {
                WaitForSingleObject(overlapped.hEvent, timeout)
            };
            if signaled != WAIT_OBJECT_0 {
                let _ = CancelIoEx(handle, Some(overlapped));
                let mut transferred = 0;
                let _ = GetOverlappedResult(handle, overlapped, &mut transferred, true);
                return Err(failure(if signaled == WAIT_TIMEOUT {
                    "Pipe operation timed out"
                } else {
                    "Pipe server stopped"
                }));
            }
        }
        let mut transferred = 0;
        GetOverlappedResult(handle, overlapped, &mut transferred, false)?;
        Ok(transferred)
    }
}

fn accept_connection(pipe: &Handle, shutdown: &Handle) -> Result<bool> {
    let ready = event()?;
    let mut overlapped = OVERLAPPED { hEvent: ready.0, ..Default::default() };
    let connected = unsafe { ConnectNamedPipe(pipe.0, Some(&mut overlapped)) };
    let result = match connected {
        Err(error) if error.code() == ERROR_PIPE_CONNECTED.to_hresult() => Ok(0),
        result => finish_io(pipe.0, &overlapped, result, Some(shutdown.0), INFINITE),
    };
    match result {
        Ok(_) => Ok(true),
        Err(error)
            if error.code() == ERROR_NO_DATA.to_hresult()
                || error.code() == ERROR_BROKEN_PIPE.to_hresult() =>
        {
            unsafe { DisconnectNamedPipe(pipe.0) }?;
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn read_line(
    handle: HANDLE,
    shutdown: Option<HANDLE>,
    limit: usize,
    timeout: Duration,
    pending: &mut Vec<u8>,
) -> Result<Vec<u8>> {
    let started = Instant::now();
    let ready = event()?;
    let mut line = std::mem::take(pending);
    loop {
        if let Some(end) = line.iter().position(|byte| *byte == b'\n') {
            if end > limit {
                return Err(failure("Pipe frame exceeds the size limit"));
            }
            *pending = line.split_off(end + 1);
            line.truncate(end);
            return Ok(line);
        }
        if line.len() > limit {
            return Err(failure("Pipe frame exceeds the size limit"));
        }
        let remaining = timeout
            .checked_sub(started.elapsed())
            .ok_or_else(|| failure("Pipe request timed out"))?;
        let mut buffer = [0u8; 4096];
        let mut overlapped = OVERLAPPED { hEvent: ready.0, ..Default::default() };
        unsafe {
            ResetEvent(ready.0)?;
        }
        let result = unsafe { ReadFile(handle, Some(&mut buffer), None, Some(&mut overlapped)) };
        let read = finish_io(
            handle,
            &overlapped,
            result,
            shutdown,
            remaining.as_millis().min(u32::MAX as u128) as u32,
        )? as usize;
        if read == 0 {
            return Err(failure("Pipe peer disconnected"));
        }
        line.extend_from_slice(&buffer[..read]);
    }
}

fn write_value(handle: HANDLE, shutdown: Option<HANDLE>, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| failure(e.to_string()))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(failure("Pipe response exceeds the size limit"));
    }
    bytes.push(b'\n');
    let ready = event()?;
    while !bytes.is_empty() {
        unsafe {
            ResetEvent(ready.0)?;
        }
        let mut overlapped = OVERLAPPED { hEvent: ready.0, ..Default::default() };
        let result = unsafe { WriteFile(handle, Some(&bytes), None, Some(&mut overlapped)) };
        let count = finish_io(handle, &overlapped, result, shutdown, 5000)? as usize;
        if count == 0 {
            return Err(failure("Pipe write made no progress"));
        }
        bytes.drain(..count);
    }
    Ok(())
}

pub struct PipeRequest {
    pub message: Value,
    pub reply: SyncSender<Value>,
}
pub struct PipeServer {
    shutdown: Arc<Handle>,
    threads: Vec<JoinHandle<()>>,
}

fn slot_name(base: &str, slot: u32) -> String {
    if slot == 0 { base.to_owned() } else { format!("{base}-{slot}") }
}

impl PipeServer {
    pub fn start(sender: Sender<PipeRequest>) -> Result<Self> {
        let (name, sid) = identity()?;
        Self::start_named(&name, &sid, sender)
    }

    fn start_named(name: &str, sid: &str, sender: Sender<PipeRequest>) -> Result<Self> {
        // Windows reports the first process to create a pipe name for every
        // instance of that name. A permanent one-instance handle per name binds
        // the reported process identity to the instance that serves the request.
        let pipes = (0..MAX_CLIENTS)
            .map(|slot| create_pipe(&slot_name(name, slot), sid))
            .collect::<Result<Vec<_>>>()?;
        let mut server = Self { shutdown: Arc::new(event()?), threads: Vec::new() };
        for pipe in pipes {
            let stop = server.shutdown.clone();
            let sender = sender.clone();
            let worker = thread::Builder::new()
                .name("moon-pipe".into())
                .spawn(move || {
                    while unsafe { WaitForSingleObject(stop.0, 0) } != WAIT_OBJECT_0 {
                        match accept_connection(&pipe, &stop) {
                            Ok(true) => {
                                let _ = serve_client(pipe.raw(), stop.0, sender.clone());
                                let _ = unsafe { DisconnectNamedPipe(pipe.raw()) };
                            }
                            Ok(false) => {}
                            Err(error) => {
                                if unsafe { WaitForSingleObject(stop.0, 0) } != WAIT_OBJECT_0 {
                                    eprintln!("moon: pipe listener stopped: {error}");
                                }
                                break;
                            }
                        }
                    }
                })
                .map_err(|e| failure(e.to_string()))?;
            server.threads.push(worker);
        }
        Ok(server)
    }
}
impl Drop for PipeServer {
    fn drop(&mut self) {
        let _ = unsafe { SetEvent(self.shutdown.0) };
        for worker in self.threads.drain(..) {
            let _ = worker.join();
        }
    }
}

fn serve_client(handle: HANDLE, shutdown: HANDLE, sender: Sender<PipeRequest>) -> Result<()> {
    let bytes = read_line(
        handle,
        Some(shutdown),
        MAX_REQUEST_BYTES,
        Duration::from_secs(5),
        &mut Vec::new(),
    )?;
    let message: Value = serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string()))?;
    let subscribe = message
        .get("type")
        .or_else(|| message.get("method"))
        .or_else(|| message.get("op"))
        .and_then(Value::as_str)
        == Some("subscribe");
    let (reply, receive) = mpsc::sync_channel(16);
    sender.send(PipeRequest { message, reply }).map_err(|_| failure("Window manager stopped"))?;
    let started = Instant::now();
    loop {
        match receive.recv_timeout(Duration::from_secs(1)) {
            Ok(response) => {
                write_value(handle, Some(shutdown), &response)?;
                if !subscribe {
                    // WriteFile completing means buffered, not consumed. Disconnect
                    // discards unread data. The client closes after reading its frame;
                    // wait for that EOF with a deadline instead of blocking in Flush.
                    wait_for_peer_close(handle, shutdown);
                    return Ok(());
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                wait_for_peer_close(handle, shutdown);
                return Ok(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if unsafe { WaitForSingleObject(shutdown, 0) } == WAIT_OBJECT_0 {
                    return Ok(());
                }
                if !subscribe && started.elapsed() > Duration::from_secs(10) {
                    return Err(failure("Window manager did not answer within ten seconds"));
                }
                unsafe {
                    PeekNamedPipe(handle, None, 0, None, None, None)?;
                }
            }
        }
    }
}

fn wait_for_peer_close(handle: HANDLE, shutdown: HANDLE) {
    let _ = read_line(handle, Some(shutdown), 1, Duration::from_secs(5), &mut Vec::new());
}

fn installed_executable() -> Result<PathBuf> {
    // The client may be a CLI with no COM apartment, or a worker that already
    // initialized its own apartment model.
    use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    struct ComScope;
    impl Drop for ComScope {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }
    let _com = match unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok() {
        Ok(()) => Some(ComScope),
        Err(error) if error.code() == RPC_E_CHANGED_MODE => None,
        Err(error) => return Err(error),
    };
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None)?;
        let directory = PathBuf::from(std::ffi::OsString::from_wide(path.as_wide()));
        CoTaskMemFree(Some(path.0.cast()));
        Ok(directory.join("moon").join("moon.exe"))
    }
}

fn same_path(first: &Path, second: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
    let first: Vec<_> = first.as_os_str().encode_wide().collect();
    let second: Vec<_> = second.as_os_str().encode_wide().collect();
    unsafe { CompareStringOrdinal(&first, &second, true) == CSTR_EQUAL }
}

fn verify_server(
    pipe: &Handle,
    sid: &str,
    expected_pid: Option<u32>,
    executable: &Path,
    exact_image: bool,
) -> Result<Handle> {
    unsafe {
        let mut instances = 0;
        GetNamedPipeInfo(pipe.0, None, None, None, Some(&mut instances))?;
        if instances != 1 {
            return Err(failure("Moon IPC requires one server instance per pipe name"));
        }
        let mut pid = 0;
        GetNamedPipeServerProcessId(pipe.0, &mut pid)?;
        if expected_pid.is_some_and(|expected| pid != expected) {
            return Err(failure("The pipe belongs to a different Moon process"));
        }
        let peer = Handle(OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            false,
            pid,
        )?);
        let mut token = HANDLE::default();
        OpenProcessToken(peer.0, TOKEN_QUERY, &mut token)?;
        let token = Handle(token);
        if identity_for_token(token.0)?.1 != sid {
            return Err(failure("The pipe server belongs to a different Windows logon"));
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        QueryFullProcessImageNameW(
            peer.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )?;
        let path = PathBuf::from(std::ffi::OsString::from_wide(&path[..length as usize]));
        let matches_image = same_path(&path, executable);
        if exact_image && !matches_image {
            return Err(failure("The pipe server is not the expected Moon executable"));
        }
        let mut session = 0;
        GetNamedPipeServerSessionId(pipe.0, &mut session)?;
        if session != crate::runtime::session_id()?
            || WaitForSingleObject(peer.0, 0) != WAIT_TIMEOUT
        {
            return Err(failure("The pipe server is no longer available in this session"));
        }
        let token_flag = |token: HANDLE, class| -> Result<bool> {
            let mut value = 0u32;
            let mut length = 0;
            GetTokenInformation(
                token,
                class,
                Some((&mut value as *mut u32).cast()),
                4,
                &mut length,
            )?;
            Ok(value != 0)
        };
        let installed = same_path(&path, &installed_executable()?);
        if token_flag(token.0, TokenIsAppContainer)? {
            return Err(failure("The pipe server is running in an AppContainer"));
        }
        if installed && token_flag(token.0, TokenUIAccess)? {
            return Ok(peer);
        }
        let caller = crate::platform::token_capabilities(std::process::id())?;
        if !matches_image
            || caller.ui_access
            || (caller.elevated && !token_flag(token.0, TokenElevation)?)
            || installed
        {
            return Err(failure("The pipe server is not the expected Moon executable"));
        }
        Ok(peer)
    }
}

fn connect(expected_pid: Option<u32>, executable: Option<&Path>) -> Result<(Handle, Handle)> {
    let (name, sid) = identity()?;
    let current = std::env::current_exe().map_err(|error| failure(error.to_string()))?;
    let expected = executable.map(Path::to_owned).unwrap_or_else(|| {
        if expected_pid.is_some()
            || current
                .file_stem()
                .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("moon"))
        {
            current
        } else {
            current.with_file_name("moon.exe")
        }
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut busy = false;
        let mut failure = None;
        for slot in 0..MAX_CLIENTS {
            let pipe = match open_pipe(&slot_name(&name, slot)) {
                Ok(pipe) => pipe,
                Err(error) => {
                    busy |= error.code() == ERROR_PIPE_BUSY.to_hresult();
                    if slot == 0 {
                        failure = Some(error);
                    }
                    continue;
                }
            };
            match verify_server(&pipe, &sid, expected_pid, &expected, executable.is_some()) {
                Ok(peer) => return Ok((pipe, peer)),
                Err(error) => failure = Some(error),
            }
        }
        if !busy || Instant::now() >= deadline {
            return Err(
                failure.unwrap_or_else(|| Error::from_hresult(ERROR_PIPE_BUSY.to_hresult()))
            );
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn open_pipe(name: &str) -> Result<Handle> {
    let name = wide(name);
    unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            None,
        )
        .map(Handle)
    }
}

pub fn request(message: &Value) -> Result<Value> {
    request_to(message, None, None)
}

pub fn request_to(message: &Value, pid: Option<u32>, executable: Option<&Path>) -> Result<Value> {
    let (pipe, _peer) = connect(pid, executable)?;
    write_value(pipe.0, None, message)?;
    let bytes =
        read_line(pipe.0, None, MAX_RESPONSE_BYTES, Duration::from_secs(10), &mut Vec::new())?;
    serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string()))
}

/// Check whether an instance is present without treating it as a trusted peer.
pub fn endpoint_present() -> bool {
    let Ok(name) = pipe_name() else { return false };
    let name = wide(&name);
    unsafe {
        WaitNamedPipeW(PCWSTR(name.as_ptr()), 1).as_bool() || GetLastError() == ERROR_SEM_TIMEOUT
    }
}

pub fn subscribe(message: &Value, mut receive: impl FnMut(Value) -> bool) -> Result<()> {
    subscribe_to(message, None, &mut receive)
}

pub fn subscribe_to(
    message: &Value,
    executable: Option<&Path>,
    mut receive: impl FnMut(Value) -> bool,
) -> Result<()> {
    let (pipe, _peer) = connect(None, executable)?;
    write_value(pipe.0, None, message)?;
    let mut pending = Vec::new();
    loop {
        let bytes =
            read_line(pipe.0, None, MAX_RESPONSE_BYTES, Duration::from_secs(86400), &mut pending)?;
        if !receive(serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string()))?) {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_disconnect_leaves_the_same_instance_available() {
        let (name, sid) = identity().unwrap();
        let name = format!("{name}-early-disconnect-{}", std::process::id());
        let pipe = create_pipe(&name, &sid).unwrap();
        let stop = event().unwrap();
        drop(open_pipe(&name).unwrap());
        assert!(!accept_connection(&pipe, &stop).unwrap());
        thread::scope(|scope| {
            let accept = scope.spawn(|| accept_connection(&pipe, &stop).unwrap());
            let native_name = wide(&name);
            let ready = unsafe { WaitNamedPipeW(PCWSTR(native_name.as_ptr()), 1000) }.as_bool();
            let client = ready.then(|| open_pipe(&name));
            if !matches!(client, Some(Ok(_))) {
                unsafe { SetEvent(stop.0) }.unwrap();
            }
            let _client = client.expect("pipe was listening").unwrap();
            assert!(accept.join().unwrap());
        });
    }

    #[test]
    fn connected_instance_must_match_the_expected_image_and_pid() {
        let (name, sid) = identity().unwrap();
        let name = format!("{name}-peer-{}", std::process::id());
        let _server = create_pipe(&name, &sid).unwrap();
        let client = open_pipe(&name).unwrap();
        let current = std::env::current_exe().unwrap();
        assert!(verify_server(&client, &sid, Some(std::process::id()), &current, true).is_ok());
        assert!(verify_server(&client, &sid, Some(0), &current, true).is_err());
        assert!(
            verify_server(&client, &sid, None, &current.with_file_name("moon.exe"), true).is_err()
        );
        assert!(verify_server(&client, "S-1-5-5-0-0", None, &current, true).is_err());
    }

    #[test]
    fn an_owned_pipe_name_cannot_have_a_second_instance() {
        let (name, sid) = identity().unwrap();
        let name = format!("{name}-one-instance-{}", std::process::id());
        let server = create_pipe(&name, &sid).unwrap();
        let wide_name = wide(&name);
        let make_second_server = || unsafe {
            CreateNamedPipeW(
                PCWSTR(wide_name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65536,
                65536,
                5000,
                None,
            )
        };
        assert!(make_second_server().is_invalid());
        assert_eq!(unsafe { GetLastError() }, ERROR_PIPE_BUSY);
        let client = open_pipe(&name).unwrap();
        let stop = event().unwrap();
        assert!(accept_connection(&server, &stop).unwrap());
        drop(client);
        unsafe { DisconnectNamedPipe(server.0) }.unwrap();
        assert!(make_second_server().is_invalid());
        assert_eq!(unsafe { GetLastError() }, ERROR_PIPE_BUSY);
    }

    #[test]
    fn a_subscription_leaves_other_slots_available_for_requests() {
        let (name, sid) = identity().unwrap();
        let name = format!("{name}-concurrent-{}", std::process::id());
        let (send, receive) = mpsc::channel();
        let _server = PipeServer::start_named(&name, &sid, send).unwrap();
        let subscription = open_pipe(&name).unwrap();
        write_value(subscription.0, None, &serde_json::json!({"method":"subscribe"})).unwrap();
        let subscribed = receive.recv_timeout(Duration::from_secs(2)).unwrap();

        let client = open_pipe(&slot_name(&name, 1)).unwrap();
        write_value(client.0, None, &serde_json::json!({"method":"query"})).unwrap();
        let request = receive.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(subscribed.message["method"], "subscribe");
        assert_eq!(request.message["method"], "query");
        request.reply.send(serde_json::json!({"ok":true})).unwrap();
        let reply =
            read_line(client.0, None, 128, Duration::from_secs(2), &mut Vec::new()).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&reply).unwrap()["ok"], true);
    }

    #[test]
    fn linked_uac_tokens_share_pipe_identity() {
        unsafe {
            let mut current = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut current).unwrap();
            let current = Handle(current);
            let mut length = 0;
            let mut elevation = TOKEN_ELEVATION_TYPE::default();
            GetTokenInformation(
                current.0,
                TokenElevationType,
                Some((&mut elevation as *mut TOKEN_ELEVATION_TYPE).cast()),
                size_of::<TOKEN_ELEVATION_TYPE>() as u32,
                &mut length,
            )
            .unwrap();
            if elevation == TokenElevationTypeDefault {
                eprintln!("No linked UAC token exists for this account; split-token check skipped");
                return;
            }
            let mut linked = TOKEN_LINKED_TOKEN::default();
            GetTokenInformation(
                current.0,
                TokenLinkedToken,
                Some((&mut linked as *mut TOKEN_LINKED_TOKEN).cast()),
                size_of::<TOKEN_LINKED_TOKEN>() as u32,
                &mut length,
            )
            .unwrap();
            // Query only: never impersonate or launch a process with this handle.
            let linked = Handle(linked.LinkedToken);
            let current_identity = identity_for_token(current.0).unwrap();
            let linked_identity = identity_for_token(linked.0).unwrap();
            assert_eq!(current_identity, linked_identity);
            assert_eq!(pipe_name().unwrap(), current_identity.0);
            assert_eq!(current_identity.0, format!(r"\\.\pipe\moon-v1-{}", current_identity.1));
            eprintln!("Current and linked tokens share {}", current_identity.0);
        }
    }

    fn server(
        tag: &str,
    ) -> (String, Arc<Handle>, mpsc::Receiver<PipeRequest>, JoinHandle<Result<()>>) {
        let (name, sid) = identity().unwrap();
        let name = format!("{name}-test-{}-{tag}", std::process::id());
        let pipe = create_pipe(&name, &sid).unwrap();
        let stop = Arc::new(event().unwrap());
        let stop_thread = stop.clone();
        let (sender, receive) = mpsc::channel();
        let worker = thread::spawn(move || {
            let ready = event()?;
            let mut overlapped = OVERLAPPED { hEvent: ready.0, ..Default::default() };
            let connected = unsafe { ConnectNamedPipe(pipe.raw(), Some(&mut overlapped)) };
            if !matches!(&connected,Err(error) if error.code()==ERROR_PIPE_CONNECTED.to_hresult()) {
                finish_io(pipe.raw(), &overlapped, connected, Some(stop_thread.0), 5000)?;
            }
            let result = serve_client(pipe.raw(), stop_thread.0, sender);
            let _ = unsafe { DisconnectNamedPipe(pipe.raw()) };
            result
        });
        (name, stop, receive, worker)
    }

    #[test]
    fn large_response_survives_delayed_reader_and_disconnect() {
        let (name, _stop, receive, worker) = server("large");
        let client = open_pipe(&name).unwrap();
        write_value(client.0, None, &serde_json::json!({"method":"query"})).unwrap();
        let request = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        let body = "x".repeat(128 * 1024);
        request.reply.send(serde_json::json!({"body":body})).unwrap();
        thread::sleep(Duration::from_millis(40));
        let frame =
            read_line(client.0, None, MAX_RESPONSE_BYTES, Duration::from_secs(5), &mut Vec::new())
                .unwrap();
        let response: Value = serde_json::from_slice(&frame).unwrap();
        assert_eq!(response["body"].as_str().unwrap(), body);
        drop(client);
        worker.join().unwrap().unwrap();
    }

    #[test]
    fn stalled_reader_does_not_prevent_server_shutdown() {
        let (name, stop, receive, worker) = server("cancel");
        let client = open_pipe(&name).unwrap();
        write_value(client.0, None, &serde_json::json!({"method":"query"})).unwrap();
        let request = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        request.reply.send(serde_json::json!({"body":"x".repeat(2*1024*1024)})).unwrap();
        thread::sleep(Duration::from_millis(40));
        let start = Instant::now();
        unsafe {
            SetEvent(stop.0).unwrap();
        }
        assert!(worker.join().unwrap().is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
        drop(client);
    }

    #[test]
    fn subscription_frames_survive_coalesced_pipe_reads() {
        let (name, _stop, receive, worker) = server("frames");
        let client = open_pipe(&name).unwrap();
        write_value(client.0, None, &serde_json::json!({"method":"subscribe"})).unwrap();
        let request = receive.recv_timeout(Duration::from_secs(5)).unwrap();
        let producer = thread::spawn(move || {
            for index in 0..200 {
                request.reply.send(serde_json::json!({"index":index})).unwrap();
            }
        });
        let mut pending = Vec::new();
        for index in 0..200 {
            let frame =
                read_line(client.0, None, MAX_RESPONSE_BYTES, Duration::from_secs(5), &mut pending)
                    .unwrap();
            let response: Value = serde_json::from_slice(&frame).unwrap();
            assert_eq!(response["index"], index);
        }
        drop(client);
        producer.join().unwrap();
        worker.join().unwrap().unwrap();
    }
}
