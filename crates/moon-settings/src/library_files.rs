//! Native, owner-modal dialogs for sharing preset data; never executes a selected file.
use crate::store::{resolve_path, write_atomically};
use std::{
    fs::File,
    io::{ErrorKind, Read},
    path::{Component, Path, PathBuf, Prefix},
};
use windows::{
    Win32::{
        Foundation::{ERROR_CANCELLED, HWND},
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree},
        UI::Shell::{
            Common::COMDLG_FILTERSPEC, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR,
            FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FOS_STRICTFILETYPES, FileOpenDialog,
            FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, SIGDN_FILESYSPATH,
        },
    },
    core::{HRESULT, HSTRING, w},
};

const LIMIT: usize = 65_536;

/// Called on the Settings UI's existing COM apartment.
pub fn import_file(owner: HWND) -> Result<Option<String>, String> {
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(native_error)?;
    configure(&dialog, false)?;
    unsafe { dialog.SetTitle(w!("Import a Moon theme or profile")) }.map_err(native_error)?;
    let Some(path) = choose(&dialog, owner)? else {
        return Ok(None);
    };
    let text = read_text(&path)?.ok_or("The selected file no longer exists.")?;
    Ok(Some(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()))
}

/// Save confirmation is provided by Windows; comparison detects changes observed
/// between selection and atomic replacement, as with Settings' ordinary saves.
pub fn export_file(owner: HWND, suggested_name: &str, document: &str) -> Result<bool, String> {
    if document.len() > LIMIT {
        return Err("A shared preset must be at most 64 KiB.".into());
    }
    let dialog: IFileSaveDialog =
        unsafe { CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER) }
            .map_err(native_error)?;
    configure(&dialog, true)?;
    let name: String = suggested_name
        .chars()
        .take(180)
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim().trim_end_matches(['.', ' ']);
    unsafe {
        dialog.SetTitle(w!("Export a Moon theme or profile")).map_err(native_error)?;
        dialog
            .SetFileName(&HSTRING::from(if name.is_empty() { "Moon preset.json" } else { name }))
            .map_err(native_error)?;
        dialog.SetDefaultExtension(w!("json")).map_err(native_error)?;
    }
    let Some(path) = choose(&dialog, owner)? else {
        return Ok(false);
    };
    // Atomic replacement follows the selected file alias, just like Settings.
    // Recheck the resolved target so an alias cannot select a network/device file.
    let path = resolve_path(&path).map_err(|error| error.message)?;
    validate_path(&path)?;
    let expected = read_text(&path)?;
    write_atomically(&path, expected.as_deref(), document).map_err(|error| error.message)?;
    Ok(true)
}

fn configure(dialog: &IFileDialog, save: bool) -> Result<(), String> {
    unsafe {
        let options = dialog.GetOptions().map_err(native_error)?
            | FOS_FORCEFILESYSTEM
            | FOS_NOCHANGEDIR
            | FOS_PATHMUSTEXIST;
        dialog
            .SetOptions(
                options
                    | if save {
                        FOS_OVERWRITEPROMPT | FOS_STRICTFILETYPES
                    } else {
                        FOS_FILEMUSTEXIST
                    },
            )
            .map_err(native_error)?;
        dialog
            .SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: w!("Moon themes and profiles (*.json)"),
                pszSpec: w!("*.moon-theme.json;*.moon-profile.json;*.json"),
            }])
            .map_err(native_error)?;
    }
    Ok(())
}
fn choose(dialog: &IFileDialog, owner: HWND) -> Result<Option<PathBuf>, String> {
    unsafe {
        if let Err(error) = dialog.Show(Some(owner)) {
            return if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                Ok(None)
            } else {
                Err(native_error(error))
            };
        }
        let name = dialog
            .GetResult()
            .and_then(|item| item.GetDisplayName(SIGDN_FILESYSPATH))
            .map_err(native_error)?;
        let text = name.to_string();
        CoTaskMemFree(Some(name.0.cast()));
        let path = PathBuf::from(text.map_err(|_| "The selected file name is not valid Unicode.")?);
        validate_path(&path)?;
        Ok(Some(path))
    }
}
fn validate_path(path: &Path) -> Result<(), String> {
    // Force a local drive path, not a URL, device or network namespace.
    if !path.is_absolute()
        || !matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
    {
        return Err("Choose a JSON file on a local drive.".into());
    }
    if !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json")) {
        return Err("Choose a JSON theme or profile file.".into());
    }
    Ok(())
}
fn read_text(path: &Path) -> Result<Option<String>, String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not read the selected file: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read the selected file: {error}"))?;
    if bytes.len() > LIMIT {
        return Err("A shared preset must be at most 64 KiB.".into());
    }
    String::from_utf8(bytes).map(Some).map_err(|_| "Choose a UTF-8 JSON file.".into())
}
fn native_error(error: windows::core::Error) -> String {
    format!("Windows could not open the file dialog: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "moon-library-file-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn selected_file_reads_enforce_utf8_and_exact_byte_limit_without_writes() {
        let fixture = Fixture::new();
        let path = fixture.0.join("import.json");
        assert!(read_text(&path).unwrap().is_none());
        for bytes in [vec![b' '; LIMIT], "é".repeat(LIMIT / 2).into_bytes()] {
            fs::write(&path, &bytes).unwrap();
            assert_eq!(read_text(&path).unwrap().unwrap().as_bytes(), bytes);
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        for bytes in [vec![b' '; LIMIT + 1], vec![0xff, 0xfe, b'{', b'}']] {
            fs::write(&path, &bytes).unwrap();
            assert!(read_text(&path).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn dialog_path_validation_rejects_nonfilesystem_and_unselected_namespaces() {
        for path in [r"C:\Themes\My theme.JSON", r"\\?\C:\Themes\My theme.moon-theme.json"] {
            assert!(validate_path(Path::new(path)).is_ok(), "rejected {path}");
        }
        for path in [
            "https://example.com/theme.json",
            r"\\server\share\theme.json",
            r"\\?\UNC\server\share\theme.json",
            r"\\.\pipe\theme.json",
            "theme.json",
            r"C:theme.json",
            r"C:\Themes\program.exe",
        ] {
            assert!(validate_path(Path::new(path)).is_err(), "accepted {path}");
        }
    }

    #[test]
    fn selected_dot_alias_resolves_before_atomic_export_and_preserves_other_files() {
        let fixture = Fixture::new();
        let original = fixture.0.join("theme.json");
        let other = fixture.0.join("untouched.json");
        fs::write(&original, "old").unwrap();
        fs::write(&other, "leave this file").unwrap();
        let resolved = resolve_path(&fixture.0.join(".").join("theme.json")).unwrap();
        validate_path(&resolved).unwrap();
        let expected = read_text(&resolved).unwrap();
        write_atomically(&resolved, expected.as_deref(), "{\"version\":1}").unwrap();
        assert_eq!(fs::read_to_string(&original).unwrap(), "{\"version\":1}");
        assert_eq!(fs::read_to_string(&other).unwrap(), "leave this file");
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
    }
}
