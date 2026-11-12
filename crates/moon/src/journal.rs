//! Write-ahead recovery record. Persisted before the first effect on any window.
use anyhow::{Context, Result, bail};
use moon_core::{geometry::Rect, ids::WindowId};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::path::PathBuf;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

const JOURNAL_VERSION: u32 = 1;
const MAX_JOURNAL_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OriginalWindow {
    pub id: WindowId,
    pub process_id: u32,
    pub process_birth: u64,
    pub rect: Rect,
    #[cfg(windows)]
    #[serde(default)]
    pub native: Option<moon_win::platform::OriginalPlacement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Journal {
    pub version: u32,
    pub owner: u32,
    pub owner_birth: u64,
    pub successor: Option<u32>,
    pub windows: BTreeMap<WindowId, OriginalWindow>,
    pub world: serde_json::Value,
}

impl Journal {
    pub fn new(owner_birth: u64) -> Self {
        Self {
            version: JOURNAL_VERSION,
            owner: std::process::id(),
            owner_birth,
            successor: None,
            windows: BTreeMap::new(),
            world: serde_json::Value::Null,
        }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("open recovery journal {}", path.display()))?;
        let mut content = Vec::new();
        file.take(MAX_JOURNAL_BYTES + 1).read_to_end(&mut content)?;
        if content.len() as u64 > MAX_JOURNAL_BYTES {
            bail!("recovery journal exceeds 16 MiB");
        }
        let journal: Self = serde_json::from_slice(&content).context("parse recovery journal")?;
        if journal.version != JOURNAL_VERSION {
            bail!("unsupported recovery journal version {}", journal.version);
        }
        Ok(journal)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let parent = path.parent().context("journal has no parent directory")?;
        fs::create_dir_all(parent)?;
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        let content = serde_json::to_vec(self)?;
        if content.len() as u64 > MAX_JOURNAL_BYTES {
            bail!("recovery journal exceeds 16 MiB");
        }
        let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
        file.write_all(&content)?;
        file.sync_all()?;
        drop(file);
        #[cfg(windows)]
        moon_win::runtime::atomic_replace(&tmp, path)?;
        #[cfg(not(windows))]
        fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[derive(Default)]
struct WriterState {
    pending: Option<(u64, Journal)>,
    submitted: u64,
    completed: u64,
    stopping: bool,
    error: Option<String>,
}

/// One active write and at most one pending snapshot. Original-window changes
/// require submit + flush before native effects; ordinary world checkpoints may
/// coalesce. Drop joins the worker before callers remove the recovery journal.
pub struct JournalWriter {
    state: Arc<(Mutex<WriterState>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl JournalWriter {
    pub fn start(path: std::path::PathBuf) -> Result<Self> {
        Self::spawn(move |journal| journal.save(&path))
    }

    fn spawn(mut save: impl FnMut(Journal) -> Result<()> + Send + 'static) -> Result<Self> {
        let state = Arc::new((Mutex::new(WriterState::default()), Condvar::new()));
        let shared = state.clone();
        let worker = thread::Builder::new().name("moon-journal".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                loop {
                    let (sequence, journal) = {
                        let (lock, wake) = &*shared;
                        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                        while state.pending.is_none() && !state.stopping {
                            state = wake.wait(state).unwrap_or_else(|error| error.into_inner());
                        }
                        if state.stopping {
                            return;
                        }
                        state.pending.take().expect("pending snapshot was checked")
                    };
                    let result = save(journal);
                    let (lock, wake) = &*shared;
                    let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                    match result {
                        Ok(()) => state.completed = sequence,
                        Err(error) => {
                            state.error = Some(format!("journal checkpoint failed: {error:#}"));
                            state.pending = None;
                            state.stopping = true;
                        }
                    }
                    wake.notify_all();
                }
            }));
            if result.is_err() {
                let (lock, wake) = &*shared;
                let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                state.error = Some("journal checkpoint worker panicked".into());
                state.pending = None;
                state.stopping = true;
                wake.notify_all();
            }
        })?;
        Ok(Self { state, worker: Some(worker) })
    }

    pub fn submit(&self, journal: Journal) -> Result<()> {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        Self::check_state(&state)?;
        state.submitted = state.submitted.checked_add(1).context("journal sequence exhausted")?;
        state.pending = Some((state.submitted, journal));
        wake.notify_all();
        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        let target = state.submitted;
        loop {
            Self::check_state(&state)?;
            if state.completed >= target {
                return Ok(());
            }
            state = wake.wait(state).unwrap_or_else(|error| error.into_inner());
        }
    }

    pub fn check(&self) -> Result<()> {
        let (lock, _) = &*self.state;
        Self::check_state(&lock.lock().unwrap_or_else(|error| error.into_inner()))
    }

    fn check_state(state: &WriterState) -> Result<()> {
        if let Some(error) = &state.error {
            bail!("{error}");
        }
        if state.stopping {
            bail!("journal checkpoint worker has stopped");
        }
        Ok(())
    }
}

impl Drop for JournalWriter {
    fn drop(&mut self) {
        let (lock, wake) = &*self.state;
        {
            let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
            state.stopping = true;
            state.pending = None;
            wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(windows)]
pub fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("moon-local"))
        .join("moon")
}

#[cfg(windows)]
pub fn default_path() -> PathBuf {
    let session = moon_win::runtime::session_id().expect("current process has a Windows session");
    data_dir().join(format!("journal-v1-session{session}.json"))
}

#[cfg(windows)]
pub fn restore(path: &Path, force: bool) -> Result<usize> {
    use moon_win::{platform, runtime};
    if !path.exists() {
        return Ok(0);
    }
    let mut journal = Journal::read(path)?;
    if !force && runtime::process_matches(journal.owner, journal.owner_birth)? {
        bail!("moon is still running; use `moon exit` before restoring its windows");
    }
    let mut restored = 0;
    let mut failures = Vec::new();
    journal.windows.retain(|_, window| {
        if platform::process_id(window.id) != window.process_id {
            return false;
        }
        match runtime::process_matches(window.process_id, window.process_birth) {
            Ok(false) => return false,
            Ok(true) => {}
            Err(error) => {
                failures.push(format!("{}: cannot verify process identity: {error}", window.id));
                return true;
            }
        }
        if !platform::can_manage_process(window.process_id) {
            failures.push(format!(
                "{}: restoration requires the signed UIAccess installation or matching integrity",
                window.id
            ));
            return true;
        }
        let result = match &window.native {
            Some(native) => platform::restore_placement(window.id, native),
            None => platform::restore(window.id, window.rect),
        };
        match result {
            Ok(()) => {
                restored += 1;
                false
            }
            Err(error) => {
                failures.push(format!("{}: {error}", window.id));
                true
            }
        }
    });
    if failures.is_empty() {
        fs::remove_file(path)?;
    } else {
        journal.save(path)?;
        bail!("restored {restored} windows; recovery remains for {}", failures.join(", "));
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    fn checkpoint(revision: u64) -> Journal {
        let mut journal = Journal::new(42);
        journal.world = serde_json::json!({"revision":revision});
        journal
    }

    #[test]
    fn checkpoint_barrier_persists_latest_snapshot_before_returning() {
        let directory =
            std::env::temp_dir().join(format!("moon-journal-barrier-{}", std::process::id()));
        let path = directory.join("journal.json");
        let writer = JournalWriter::start(path.clone()).unwrap();
        writer.flush().unwrap();
        writer.submit(checkpoint(1)).unwrap();
        writer.submit(checkpoint(2)).unwrap();
        writer.flush().unwrap();
        writer.check().unwrap();
        assert_eq!(Journal::read(&path).unwrap().world["revision"], 2);
        drop(writer);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn pending_checkpoints_coalesce_while_an_older_write_is_in_flight() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let writes = Arc::new(Mutex::new(Vec::new()));
        let observed = writes.clone();
        let writer = JournalWriter::spawn(move |journal| {
            let revision = journal.world["revision"].as_u64().unwrap();
            if revision == 1 {
                entered_tx.send(())?;
                release_rx.recv()?;
            }
            observed.lock().unwrap().push(revision);
            Ok(())
        })
        .unwrap();
        writer.submit(checkpoint(1)).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        for revision in 2..=100 {
            writer.submit(checkpoint(revision)).unwrap();
        }
        release_tx.send(()).unwrap();
        writer.flush().unwrap();
        assert_eq!(*writes.lock().unwrap(), [1, 100]);
    }

    #[test]
    fn checkpoint_failure_reaches_barrier_health_and_future_submitters() {
        let writer = JournalWriter::spawn(|_| bail!("simulated disk full")).unwrap();
        writer.submit(checkpoint(1)).unwrap();
        for error in [
            writer.flush().unwrap_err(),
            writer.check().unwrap_err(),
            writer.submit(checkpoint(2)).unwrap_err(),
        ] {
            assert!(error.to_string().contains("simulated disk full"));
        }
    }

    #[test]
    fn checkpoint_worker_panic_wakes_the_barrier() {
        let writer = JournalWriter::spawn(|_| panic!("simulated checkpoint panic")).unwrap();
        writer.submit(checkpoint(1)).unwrap();
        assert!(writer.flush().unwrap_err().to_string().contains("worker panicked"));
    }

    #[test]
    fn dropping_writer_finishes_active_write_and_discards_pending_snapshot() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (stopped_tx, stopped_rx) = mpsc::channel();
        let writes = Arc::new(Mutex::new(Vec::new()));
        let observed = writes.clone();
        let writer = JournalWriter::spawn(move |journal| {
            entered_tx.send(())?;
            release_rx.recv()?;
            observed.lock().unwrap().push(journal.world["revision"].as_u64().unwrap());
            Ok(())
        })
        .unwrap();
        writer.submit(checkpoint(1)).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        writer.submit(checkpoint(2)).unwrap();
        let shared = writer.state.clone();
        let dropping = thread::spawn(move || {
            drop(writer);
            stopped_tx.send(()).unwrap();
        });
        let (lock, wake) = &*shared;
        let (state, timeout) = wake
            .wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(2), |s| !s.stopping)
            .unwrap();
        assert!(!timeout.timed_out());
        assert!(stopped_rx.try_recv().is_err(), "drop must wait for the active save");
        drop(state);
        release_tx.send(()).unwrap();
        stopped_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        dropping.join().unwrap();
        assert_eq!(*writes.lock().unwrap(), [1]);
    }

    #[test]
    fn recovery_record_round_trips_and_rejects_future_versions() {
        let directory =
            std::env::temp_dir().join(format!("moon-journal-test-{}", std::process::id()));
        let path = directory.join("journal.json");
        let mut journal = Journal::new(42);
        journal.windows.insert(
            WindowId(3),
            OriginalWindow {
                id: WindowId(3),
                process_id: 8,
                process_birth: 12,
                rect: Rect::new(1, 2, 300, 400),
                #[cfg(windows)]
                native: None,
            },
        );
        journal.save(&path).unwrap();
        let read = Journal::read(&path).unwrap();
        assert_eq!(read.windows[&WindowId(3)].rect, Rect::new(1, 2, 300, 400));
        journal.version += 1;
        journal.save(&path).unwrap();
        assert!(Journal::read(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}
