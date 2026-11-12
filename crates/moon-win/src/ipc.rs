//! Local-session NDJSON transport with a logon-SID ACL and medium integrity floor.
//! Reads, writes, connections, request size and concurrent clients are bounded.

use crate::platform::wide;
use serde_json::Value;
use std::{
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
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
        System::{IO::*, Pipes::*, Threading::*},
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

fn create_pipe(name: &str, sid: &str, first: bool) -> Result<Handle> {
    // No inherited ACEs. Low integrity clients can neither read nor write up.
    let descriptor = wide(&format!("D:P(A;;GA;;;SY)(A;;GRGW;;;{sid})S:(ML;;NRNW;;;ME)"));
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
        let flags = PIPE_ACCESS_DUPLEX
            | FILE_FLAG_OVERLAPPED
            | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { FILE_FLAGS_AND_ATTRIBUTES(0) };
        let handle = CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            MAX_CLIENTS,
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
    thread: Option<JoinHandle<()>>,
}
impl PipeServer {
    pub fn start(sender: Sender<PipeRequest>) -> Result<Self> {
        let (name, sid) = identity()?;
        let first = create_pipe(&name, &sid, true)?;
        let shutdown = Arc::new(event()?);
        let stop = shutdown.clone();
        let active = Arc::new(AtomicUsize::new(0));
        let thread = thread::Builder::new()
            .name("moon-pipe".into())
            .spawn(move || {
                let mut pipe = first;
                loop {
                    let Ok(ready) = event() else {
                        break;
                    };
                    let mut overlapped = OVERLAPPED { hEvent: ready.0, ..Default::default() };
                    let connected = unsafe { ConnectNamedPipe(pipe.0, Some(&mut overlapped)) };
                    let connected = match connected {
                        Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => true,
                        result => {
                            finish_io(pipe.0, &overlapped, result, Some(stop.0), INFINITE).is_ok()
                        }
                    };
                    if !connected {
                        break;
                    }
                    // Keep another instance open before handing the connected one off.
                    // This preserves the first-instance protection across idle periods.
                    let next = match create_pipe(&name, &sid, false) {
                        Ok(value) => value,
                        Err(_) => {
                            let _ = unsafe { DisconnectNamedPipe(pipe.0) };
                            continue;
                        }
                    };
                    let connected_pipe = std::mem::replace(&mut pipe, next);
                    if active.load(Ordering::Acquire) >= MAX_CLIENTS as usize - 1 {
                        drop(connected_pipe);
                        continue;
                    }
                    active.fetch_add(1, Ordering::AcqRel);
                    struct Count(Arc<AtomicUsize>);
                    impl Drop for Count {
                        fn drop(&mut self) {
                            self.0.fetch_sub(1, Ordering::AcqRel);
                        }
                    }
                    // Construct before spawning so a failed spawn also releases the slot.
                    let count = Count(active.clone());
                    let stop = stop.clone();
                    let sender = sender.clone();
                    let _ =
                        thread::Builder::new().name("moon-pipe-client".into()).spawn(move || {
                            let _count = count;
                            let _ = serve_client(connected_pipe.raw(), stop.0, sender);
                            let _ = unsafe { DisconnectNamedPipe(connected_pipe.raw()) };
                        });
                }
            })
            .map_err(|e| failure(e.to_string()))?;
        Ok(Self { shutdown, thread: Some(thread) })
    }
}
impl Drop for PipeServer {
    fn drop(&mut self) {
        let _ = unsafe { SetEvent(self.shutdown.0) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
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

fn connect() -> Result<Handle> {
    connect_named(&pipe_name()?)
}

fn connect_named(name: &str) -> Result<Handle> {
    let name = wide(name);
    unsafe {
        if !WaitNamedPipeW(PCWSTR(name.as_ptr()), 5000).as_bool() {
            return Err(Error::from_thread());
        }
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
    let pipe = connect()?;
    write_value(pipe.0, None, message)?;
    let bytes =
        read_line(pipe.0, None, MAX_RESPONSE_BYTES, Duration::from_secs(10), &mut Vec::new())?;
    serde_json::from_slice(&bytes).map_err(|e| failure(e.to_string()))
}

pub fn subscribe(message: &Value, mut receive: impl FnMut(Value) -> bool) -> Result<()> {
    let pipe = connect()?;
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
        let pipe = create_pipe(&name, &sid, true).unwrap();
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
        let client = connect_named(&name).unwrap();
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
        let client = connect_named(&name).unwrap();
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
        let client = connect_named(&name).unwrap();
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
