//! Settings persistence; no daemon or UI handles are retained here.
use moon_core::{
    config::Config,
    settings::{self, Edit, ErrorCode, MAX_SETTINGS_BYTES, SettingsError, Snapshot},
};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct Store {
    path: PathBuf,
}
impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn load(&self) -> Result<Snapshot, SettingsError> {
        Snapshot::load(read_source(&self.path)?.as_deref())
    }
    pub fn save(
        &self,
        expected_source: Option<&str>,
        edits: &[Edit],
        validate: impl FnOnce(&Config) -> Result<(), SettingsError>,
    ) -> Result<Snapshot, SettingsError> {
        with_locked_path(&self.path, |path| {
            let current = read_source(path)?;
            let prepared = settings::prepare_update(current.as_deref(), expected_source, edits)?;
            if prepared.changed {
                validate(&prepared.config)?;
                let snapshot = Snapshot::load(Some(&prepared.source))?;
                write_atomically(path, current.as_deref(), &prepared.source)?;
                Ok(snapshot)
            } else {
                Snapshot::load(current.as_deref())
            }
        })
    }
}

/// Reuse the same bounded advisory lock and path resolution for Settings sidecars.
pub(crate) fn with_locked_path<T>(
    path: &Path,
    operation: impl FnOnce(&Path) -> Result<T, SettingsError>,
) -> Result<T, SettingsError> {
    let parent =
        path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| io_error("create the configuration folder", e))?;
    let path = resolve_path(path)?;
    let mut lock_name = path.as_os_str().to_owned();
    lock_name.push(".moon-settings.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(lock_name))
        .map_err(|e| io_error("open the settings save lock", e))?;
    lock_for_save(&lock, Duration::from_secs(1))?;
    operation(&path)
    // Closing the sidecar handle releases its OS lock, including on errors.
}

/// Resolve existing aliases without creating files or directories during reads.
pub(crate) fn resolve_path(path: &Path) -> Result<PathBuf, SettingsError> {
    if path.exists() {
        return fs::canonicalize(path).map_err(|e| io_error("resolve the configuration path", e));
    }
    let parent =
        path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let parent = if parent.exists() {
        fs::canonicalize(parent).map_err(|e| io_error("resolve the configuration folder", e))?
    } else if parent.is_absolute() {
        parent.to_owned()
    } else {
        std::env::current_dir().map_err(|e| io_error("resolve the current folder", e))?.join(parent)
    };
    let name = path.file_name().ok_or_else(|| {
        io_error(
            "open the configuration",
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "a file name is required"),
        )
    })?;
    Ok(parent.join(name))
}

fn lock_for_save(lock: &File, timeout: Duration) -> Result<(), SettingsError> {
    let start = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) if start.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(SettingsError::new(
                    ErrorCode::Io,
                    "Another Settings window is saving this configuration.",
                    "Wait for that save to finish and try again.",
                ));
            }
            Err(TryLockError::Error(error)) => {
                return Err(io_error("lock settings for saving", error));
            }
        }
    }
}
fn io_error(action: &str, error: std::io::Error) -> SettingsError {
    SettingsError::new(
        ErrorCode::Io,
        format!("Could not {action}: {error}"),
        "Check the file permissions and try again. Reload settings if another editor changed the file.",
    )
}
pub(crate) fn read_source(path: &Path) -> Result<Option<String>, SettingsError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error("read the configuration", e)),
    };
    let mut bytes = Vec::new();
    file.take(MAX_SETTINGS_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error("read the configuration", e))?;
    if bytes.len() > MAX_SETTINGS_BYTES {
        return Err(SettingsError::new(
            ErrorCode::InvalidConfig,
            "Configuration exceeds 1 MiB.",
            "Reduce the configuration size before editing settings.",
        ));
    }
    String::from_utf8(bytes).map(Some).map_err(|_| {
        SettingsError::new(
            ErrorCode::InvalidConfig,
            "The configuration is not UTF-8 text.",
            "Save the file as UTF-8 in your editor, then reload settings.",
        )
    })
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Ok(metadata) = fs::metadata(&self.0)
            && metadata.permissions().readonly()
        {
            let mut permissions = metadata.permissions();
            // Windows-only: clear the copied temporary file's readonly attribute
            // so cleanup can remove it. No Unix mode bits are changed here.
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = fs::set_permissions(&self.0, permissions);
        }
        let _ = fs::remove_file(&self.0);
    }
}
pub(crate) fn write_atomically(
    path: &Path,
    expected: Option<&str>,
    source: &str,
) -> Result<(), SettingsError> {
    if source.len() > MAX_SETTINGS_BYTES {
        return Err(SettingsError::new(
            ErrorCode::Validation,
            "The updated configuration exceeds 1 MiB.",
            "Reduce the configuration size before saving. The existing file has not been changed.",
        ));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if expected.is_some()
        && fs::metadata(path)
            .map_err(|e| io_error("inspect the configuration", e))?
            .permissions()
            .readonly()
    {
        return Err(io_error(
            "save a read-only configuration",
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the file is marked read-only",
            ),
        ));
    }
    let mut temporary = None;
    for _ in 0..32 {
        let id = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(".moon-settings-{}-{id}.tmp", std::process::id());
        let candidate = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(file) => {
                drop(file);
                temporary = Some(Temporary(candidate));
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_error("create a temporary settings file", e)),
        }
    }
    let temporary = temporary.ok_or_else(|| {
        io_error(
            "create a temporary settings file",
            std::io::Error::new(std::io::ErrorKind::AlreadyExists, "temporary names are in use"),
        )
    })?;
    if expected.is_some() {
        // CopyFileEx on Windows preserves NTFS streams and file security data;
        // Unix copies permission bits. Only this temporary copy is truncated.
        fs::copy(path, &temporary.0)
            .map_err(|e| io_error("preserve configuration permissions", e))?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&temporary.0)
        .map_err(|e| io_error("open the temporary settings file", e))?;
    file.write_all(source.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| io_error("write and sync settings", e))?;
    drop(file);
    // Editors that ignore our advisory lock can still change the file. Detect
    // every change observed before replacement; the final OS rename has no
    // portable compare-and-swap operation for noncooperating writers.
    if read_source(path)?.as_deref() != expected {
        return Err(SettingsError::conflict());
    }
    fs::rename(&temporary.0, path).map_err(|e| io_error("replace the configuration", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use moon_core::settings::PathSegment;
    fn save(
        store: &Store,
        source: Option<&str>,
        edits: &[Edit],
    ) -> Result<Snapshot, SettingsError> {
        store.save(source, edits, |_| Ok(()))
    }
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let id = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("moon-settings-test-{}-{id}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("config.toml")
        }
        fn store(&self) -> Store {
            Store::new(self.path())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn theme(value: &str) -> Edit {
        Edit::Set { path: vec![PathSegment::Key("theme".into())], value: serde_json::json!(value) }
    }
    #[test]
    fn absent_file_stays_absent_until_a_real_valid_edit() {
        let fixture = Fixture::new();
        let store = fixture.store();
        assert!(store.load().unwrap().source.is_none());
        assert!(save(&store, None, &[]).unwrap().source.is_none());
        assert!(!fixture.path().exists());
        let saved = save(&store, None, &[theme("light")]).unwrap();
        assert_eq!(saved.config.theme, "light");
        assert_eq!(read_source(&fixture.path()).unwrap(), saved.source);
    }
    #[test]
    fn stale_invalid_and_failed_edits_do_not_overwrite_file() {
        let fixture = Fixture::new();
        let store = fixture.store();
        fs::write(fixture.path(), "theme='light' # external\n").unwrap();
        let original = fs::read_to_string(fixture.path()).unwrap();
        assert_eq!(
            save(&store, Some("theme='dark'\n"), &[theme("system")]).unwrap_err().code,
            ErrorCode::Conflict
        );
        assert_eq!(
            save(&store, Some(&original), &[theme("invalid")]).unwrap_err().code,
            ErrorCode::Validation
        );
        assert_eq!(fs::read_to_string(fixture.path()).unwrap(), original);
        fs::write(fixture.path(), "[bar\n").unwrap();
        assert_eq!(
            save(&store, Some("[bar\n"), &[theme("dark")]).unwrap_err().code,
            ErrorCode::InvalidConfig
        );
        assert_eq!(fs::read_to_string(fixture.path()).unwrap(), "[bar\n");
    }
    #[test]
    fn oversized_save_never_replaces_the_existing_configuration() {
        let fixture = Fixture::new();
        let original = "theme='dark' # preserve\n";
        fs::write(fixture.path(), original).unwrap();
        let edit = Edit::Set {
            path: vec![PathSegment::Key("bar".into()), PathSegment::Key("font".into())],
            value: serde_json::json!("x".repeat(MAX_SETTINGS_BYTES)),
        };
        assert_eq!(
            save(&fixture.store(), Some(original), &[edit]).unwrap_err().code,
            ErrorCode::Validation
        );
        assert_eq!(fs::read_to_string(fixture.path()).unwrap(), original);
        assert!(
            !fs::read_dir(&fixture.0).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }
    #[test]
    fn final_reread_rejects_external_change_and_cleans_temporary_file() {
        let fixture = Fixture::new();
        fs::write(fixture.path(), "theme='system'\n").unwrap();
        assert_eq!(
            write_atomically(&fixture.path(), Some("theme='dark'\n"), "theme='light'\n")
                .unwrap_err()
                .code,
            ErrorCode::Conflict
        );
        assert_eq!(fs::read_to_string(fixture.path()).unwrap(), "theme='system'\n");
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }
    #[test]
    fn cooperating_instances_cannot_both_commit_the_same_snapshot() {
        let fixture = Fixture::new();
        let initial = "theme='dark'\n";
        fs::write(fixture.path(), initial).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let threads: Vec<_> = ["light", "system"]
            .into_iter()
            .map(|value| {
                let store = fixture.store();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    save(&store, Some(initial), &[theme(value)])
                })
            })
            .collect();
        barrier.wait();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert_eq!(
            results.iter().filter(|r| matches!(r, Err(e) if e.code == ErrorCode::Conflict)).count(),
            1
        );
        assert!(fixture.store().load().is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn atomic_replacement_preserves_private_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        fs::write(fixture.path(), "theme='dark'\n").unwrap();
        fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o600)).unwrap();
        save(&fixture.store(), Some("theme='dark'\n"), &[theme("light")]).unwrap();
        assert_eq!(fs::metadata(fixture.path()).unwrap().permissions().mode() & 0o777, 0o600);
    }
    #[test]
    fn failed_runtime_validation_leaves_the_saved_configuration_intact() {
        let fixture = Fixture::new();
        let original = "theme='dark'\n";
        fs::write(fixture.path(), original).unwrap();
        let error = fixture
            .store()
            .save(Some(original), &[theme("light")], |config| {
                assert_eq!(config.theme, "light");
                Err(SettingsError::new(ErrorCode::Validation, "Rejected by monitor preflight", ""))
            })
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(fs::read_to_string(fixture.path()).unwrap(), original);
    }
    #[test]
    fn waiting_for_a_busy_save_lock_can_end_without_writing() {
        let fixture = Fixture::new();
        let lock_path = fixture.0.join("config.toml.moon-settings.lock");
        let holder = File::create_new(&lock_path).unwrap();
        holder.lock().unwrap();
        let waiting = OpenOptions::new().read(true).write(true).open(lock_path).unwrap();
        let error = lock_for_save(&waiting, Duration::ZERO).unwrap_err();
        assert_eq!(error.code, ErrorCode::Io);
        assert_eq!(error.message, "Another Settings window is saving this configuration.");
        assert!(!fixture.path().exists());
    }
}
