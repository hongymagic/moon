//! Control the owning user's sign-in entry for either installation type.
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
fn optional<T>(result: windows_registry::Result<T>) -> Result<Option<T>, String> {
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error)
            if [ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND]
                .iter()
                .any(|code| code.to_hresult().0 == error.code().0) =>
        {
            Ok(None)
        }
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(windows)]
fn user_startup(enabled: Option<bool>) -> Result<Option<Value>, String> {
    use windows_registry::CURRENT_USER;
    const INSTALL: &str = r"Software\MoonWM\UserInstall";
    let Some(directory) =
        optional(CURRENT_USER.open(INSTALL).and_then(|k| k.get_string("Directory")))?
    else {
        return Ok(None);
    };
    let installed = std::path::Path::new(&directory).join("moon-settings.exe");
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    if !installed.is_file()
        || std::fs::canonicalize(&installed).map_err(|e| e.to_string())?
            != std::fs::canonicalize(current).map_err(|e| e.to_string())?
    {
        return Ok(None);
    }
    let expected = format!(r#""{}" --start-moon"#, installed.display());
    let active =
        sign_in_preference(r"Software\Microsoft\Windows\CurrentVersion\Run", &expected, enabled)?;
    Ok(Some(json!({"available":true,"enabled":active,"message": if active {
        "Moon will launch when you sign in."
    } else { "Moon will stay closed at sign-in." }})))
}

#[cfg(windows)]
fn sign_in_preference(run: &str, expected: &str, requested: Option<bool>) -> Result<bool, String> {
    use windows_registry::{CURRENT_USER, Transaction};
    const NAME: &str = "Moon Window Manager";
    let transaction =
        requested.map(|_| Transaction::new()).transpose().map_err(|e| e.to_string())?;
    let mut options = CURRENT_USER.options();
    options.read();
    if let Some(tx) = &transaction {
        options.transaction(tx).write();
        if requested == Some(true) {
            options.create();
        }
    }
    let Some(key) = optional(options.open(run))? else { return Ok(false) };
    let existing = optional(key.get_string(NAME))?;
    if existing.as_deref().is_some_and(|command| command != expected) {
        return Err(
            "Another command uses Moon's sign-in entry. Check Startup apps in Windows Settings."
                .into(),
        );
    }
    let active = existing.is_some();
    if let (Some(value), Some(transaction)) = (requested, transaction)
        && value != active
    {
        if value { key.set_string(NAME, expected) } else { key.remove_value(NAME) }
            .map_err(|e| e.to_string())?;
        drop(key);
        transaction.commit().map_err(|e| e.to_string())?;
        return Ok(value);
    }
    Ok(active)
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
    if let Some(status) = user_startup(enabled)? {
        return Ok(status);
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
    #[cfg(windows)]
    #[test]
    fn user_sign_in_only_changes_its_own_command() {
        use windows_registry::CURRENT_USER;
        let root =
            format!(r"Software\MoonWM-Test-{}", windows::core::GUID::new().unwrap().to_u128());
        let command = r#""C:\With spaces\moon-settings.exe" --start-moon"#;
        assert!(!sign_in_preference(&root, command, None).unwrap());
        assert!(sign_in_preference(&root, command, Some(true)).unwrap());
        assert!(sign_in_preference(&root, command, None).unwrap());
        assert!(sign_in_preference(&root, "other", Some(false)).is_err());
        assert!(!sign_in_preference(&root, command, Some(false)).unwrap());
        assert!(!sign_in_preference(&root, command, None).unwrap());
        CURRENT_USER.remove_tree(root).unwrap();
    }
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
