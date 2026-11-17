//! Read or toggle only the installed owner's validated Limited startup task.
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Status {
    available: bool,
    enabled: bool,
    message: String,
}

fn decode(output: &[u8], success: bool) -> Result<Value, String> {
    if output.len() > 16_384 {
        return Err("Windows returned an oversized startup response.".into());
    }
    let value: Value = serde_json::from_slice(output)
        .map_err(|_| "Windows did not return a readable startup status.".to_owned())?;
    if !success {
        return Err(value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("Windows could not change the startup preference.")
            .to_owned());
    }
    let status: Status = serde_json::from_value(value)
        .map_err(|_| "Windows returned an invalid startup status.".to_owned())?;
    if status.message.len() > 4096 || (!status.available && status.enabled) {
        return Err("Windows returned an inconsistent startup status.".into());
    }
    Ok(json!({"available":status.available,"enabled":status.enabled,"message":status.message}))
}

#[cfg(windows)]
pub fn status() -> Result<Value, String> {
    run(None)
}
#[cfg(windows)]
pub fn set(enabled: bool) -> Result<Value, String> {
    run(Some(enabled))
}

#[cfg(windows)]
fn run(enabled: Option<bool>) -> Result<Value, String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::{
        io::Read,
        os::windows::process::CommandExt,
        path::PathBuf,
        process::{Command, Stdio},
        sync::mpsc,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            System::Com::CoTaskMemFree,
            UI::Shell::{
                FOLDERID_ProgramFiles, FOLDERID_System, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
            },
        },
        core::GUID,
    };
    fn folder(id: &GUID) -> Result<PathBuf, String> {
        let raw = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }
            .map_err(|e| e.to_string())?;
        let path = unsafe { raw.to_string() };
        unsafe { CoTaskMemFree(Some(raw.0.cast())) };
        path.map(PathBuf::from).map_err(|e| e.to_string())
    }
    let helper = folder(&FOLDERID_ProgramFiles)?.join("moon/install-common.ps1");
    if !helper.is_file() {
        return Ok(
            json!({"available":false,"enabled":false,"message":"Install Moon to choose whether it launches when you sign in."}),
        );
    }
    let call = match enabled {
        Some(true) => "Set-MoonStartupEnabled -Enabled $true",
        Some(false) => "Set-MoonStartupEnabled -Enabled $false",
        None => "Get-MoonStartupStatus",
    };
    let script = format!(
        r#"$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
try {{
 . '{}'
 if (!(Get-Command Get-MoonStartupStatus -ErrorAction SilentlyContinue)) {{
  @{{available=$false;enabled=$false;message='Update Moon to change startup from Settings.'}}|ConvertTo-Json -Compress
 }} else {{ {call}|ConvertTo-Json -Compress }}
}} catch {{ @{{error=$_.Exception.Message}}|ConvertTo-Json -Compress; exit 1 }}
"#,
        helper.to_string_lossy().replace('\'', "''")
    );
    let encoded =
        STANDARD.encode(script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>());
    let powershell = folder(&FOLDERID_System)?.join("WindowsPowerShell/v1.0/powershell.exe");
    let mut child = Command::new(powershell)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ])
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not read the Windows startup task: {e}"))?;
    let output =
        child.stdout.take().ok_or("Windows did not provide the startup response stream.")?;
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = output.take(16_385).read_to_end(&mut bytes).map(|_| bytes);
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => break Err("Windows took too long to check the startup task. Refresh its status before trying again.".to_owned()),
            Err(error) => break Err(format!("Could not wait for Windows startup settings: {error}")),
        }
    };
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let status = result?;
    let bytes = receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| "The Windows startup response did not finish.".to_owned())?
        .map_err(|e| e.to_string())?;
    decode(&bytes, status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_requires_boolean_state_and_preserves_actionable_windows_failure() {
        assert_eq!(
            decode(br#"{"available":true,"enabled":false,"message":"Off"}"#, true).unwrap()["enabled"],
            false
        );
        assert!(decode(br#"{"available":false,"enabled":true,"message":"Wrong"}"#, true).is_err());
        assert!(
            decode(br#"{"available":true,"enabled":"false","message":"Wrong"}"#, true).is_err()
        );
        assert!(
            decode(
                br#"{"available":true,"enabled":false,"message":"Off","path":"external"}"#,
                true
            )
            .is_err()
        );
        assert_eq!(
            decode(br#"{"error":"Access denied. Update Moon to enable this setting."}"#, false)
                .unwrap_err(),
            "Access denied. Update Moon to enable this setting."
        );
        assert!(decode(&vec![b'x'; 16_385], true).is_err());
    }
}
