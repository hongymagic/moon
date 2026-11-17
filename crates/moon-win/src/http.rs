//! Bounded HTTPS JSON reads for the two fixed Open-Meteo services.
use std::time::{Duration, Instant};

pub fn meteo_json(host: &str, path: &str) -> Result<serde_json::Value, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    if !matches!(host, "api.open-meteo.com" | "geocoding-api.open-meteo.com")
        || !path.starts_with('/')
        || path.contains('\0')
        || path.len() > 4096
    {
        return Err("Invalid location service endpoint.".into());
    }
    use windows::{
        Win32::Networking::WinHttp::*,
        core::{PCWSTR, w},
    };
    struct Internet(*mut std::ffi::c_void);
    impl Internet {
        fn new(handle: *mut std::ffi::c_void) -> std::result::Result<Self, String> {
            if handle.is_null() {
                Err(windows::core::Error::from_thread().to_string())
            } else {
                Ok(Self(handle))
            }
        }
    }
    impl Drop for Internet {
        fn drop(&mut self) {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
    unsafe {
        let session = Internet::new(WinHttpOpen(
            w!("moon/0.1 location and weather"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 3000, 3000, 3000, 3000).map_err(|e| e.to_string())?;
        WinHttpSetOption(
            Some(session.0),
            WINHTTP_OPTION_CONNECT_RETRIES,
            Some(&1u32.to_ne_bytes()),
        )
        .map_err(|e| e.to_string())?;
        let host: Vec<u16> = host.encode_utf16().chain(Some(0)).collect();
        let connect = Internet::new(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), 443, 0))?;
        let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let request = Internet::new(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        for (option, value) in [
            (WINHTTP_OPTION_REDIRECT_POLICY, WINHTTP_OPTION_REDIRECT_POLICY_NEVER),
            (WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT, 3000),
        ] {
            WinHttpSetOption(Some(request.0), option, Some(&value.to_ne_bytes()))
                .map_err(|e| e.to_string())?;
        }
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|e| e.to_string())?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| e.to_string())?;
        if Instant::now() > deadline {
            return Err("Location service response exceeded its time limit".into());
        }
        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|e| e.to_string())?;
        if status != 200 {
            return Err(format!("location service HTTP {status}"));
        }
        let mut body = Vec::new();
        loop {
            let mut buffer = [0u8; 4096];
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read)
                .map_err(|e| e.to_string())?;
            if body.len() + read as usize > 262_144 || Instant::now() > deadline {
                return Err("Location service response exceeded its size or time limit".into());
            }
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        serde_json::from_slice(&body).map_err(|error| error.to_string())
    }
}
