use crate::{
    Options, config_path,
    journal::{self, Journal, OriginalWindow},
    load_config,
    workspace_shortcuts::{self, Shortcut, WorkspaceShortcuts},
};
use anyhow::{Context, Result, bail};
use moon_core::{
    ids::WindowId,
    world::{self, Control, Event, Placement, Plan, World},
};
use moon_win::{
    carousel::{Carousel, PageWindow, Transition as CarouselTransition, WindowRef},
    hooks::{self, HookBinding, Hooks, PlatformEvent, SystemEvent},
    ipc::{self, PipeRequest, PipeServer},
    platform,
    providers::Providers,
    runtime,
    surfaces::Surfaces,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::Write,
    path::PathBuf,
    process::{self, Stdio},
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    thread,
    time::{Duration, Instant, SystemTime},
};

enum Intake {
    Platform(PlatformEvent),
    Request(PipeRequest),
    Providers(BTreeMap<String, String>),
}

struct Daemon {
    options: Options,
    world: World,
    journal: Journal,
    journal_writer: Option<journal::JournalWriter>,
    path: PathBuf,
    applied: BTreeMap<WindowId, Placement>,
    hooks: Hooks,
    surfaces: Option<Surfaces>,
    carousel: Carousel,
    values: BTreeMap<String, String>,
    error: Option<String>,
    transactions: VecDeque<Value>,
    subscribers: Vec<(Value, SyncSender<Value>)>,
    started: Instant,
    config_modified: Option<SystemTime>,
    quit: bool,
    restart: bool,
    provider_config: Sender<moon_core::config::Config>,
    refusals: BTreeMap<WindowId, u8>,
    placement_started: BTreeMap<WindowId, Instant>,
    pending_input: VecDeque<Instant>,
    workspace_shortcuts: WorkspaceShortcuts,
    key_latencies: VecDeque<u64>,
    dispatch_latencies: VecDeque<u64>,
    ready_us: u128,
    session_suspended: bool,
    undo: moon_core::desktop::LayoutUndo,
    undo_births: BTreeMap<WindowId, u64>,
}

/// Covers errors and unwinds before the main loop's recovery path exists.
/// Declared after Instance so restoration remains serialized with other starts.
struct StartupRecovery {
    path: PathBuf,
    armed: bool,
}
impl Drop for StartupRecovery {
    fn drop(&mut self) {
        if !self.armed || !self.path.exists() {
            return;
        }
        let ours = Journal::read(&self.path).is_ok_and(|record| record.owner == process::id());
        if let Err(error) = journal::restore(&self.path, ours) {
            eprintln!("moon: startup recovery retained for retry: {error:#}");
        }
    }
}

fn bindings(world: &World) -> Vec<HookBinding> {
    moon_core::help::shortcuts(world, true)
        .bindings
        .into_iter()
        .map(|binding| HookBinding { chord: binding.chord, commands: binding.commands })
        .collect()
}

fn forward<T: Send + 'static>(
    rx: Receiver<T>,
    tx: Sender<Intake>,
    main: u32,
    wrap: fn(T) -> Intake,
) {
    thread::spawn(move || {
        while let Ok(value) = rx.recv() {
            if tx.send(wrap(value)).is_err() {
                break;
            }
            runtime::wake(main);
        }
    });
}

fn add_options(command: &mut process::Command, options: &Options) {
    if let Some(config) = &options.config {
        command.arg("--config").arg(config);
    }
    if let Some(only) = &options.only {
        command.args(["--only", only]);
    }
    if options.no_hook {
        command.arg("--no-hook");
    }
    if options.no_surfaces {
        command.arg("--no-surfaces");
    }
    if options.surfaces {
        command.arg("--surfaces");
    }
}

pub fn background(options: Options) -> Result<()> {
    load_config(&options)?;
    let query = json!({"version":1,"id":1,"method":"query","query":"stats"});
    if ipc::request(&query).is_ok() {
        bail!("moon is already running in this logon session");
    }
    fs::create_dir_all(journal::data_dir())?;
    let path = journal::data_dir().join("moon.log");
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 8 * 1024 * 1024) {
        let previous = path.with_extension("previous.log");
        if previous.exists() {
            fs::remove_file(&previous)?;
        }
        fs::rename(&path, previous)?;
    }
    let mut output = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .context("open background Moon log")?;
    let launch = (|| {
        let mut command = runtime::internal_command().context("prepare background Moon command")?;
        command.arg("start");
        add_options(&mut command, &options);
        if options.resume {
            command.arg("--resume");
        }
        if let Some(parent) = options.wait_parent {
            command.arg("--wait-parent").arg(parent.to_string());
        }
        if options.takeover {
            command.arg("--takeover");
        }
        let stdout = output.try_clone().context("duplicate Moon log for background stdout")?;
        let stderr = output.try_clone().context("duplicate Moon log for background stderr")?;
        command.stdin(Stdio::null()).stdout(stdout).stderr(stderr);
        runtime::spawn_internal(&mut command, runtime::InternalStdio::Log)
            .context("spawn background Moon process")
    })();
    let mut child = record_background_launch(&mut output, launch)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            bail!("background startup failed ({status}); see {}", path.display());
        }
        if let Ok(reply) = ipc::request_to(&query, Some(child.id()), None)
            && reply["result"]["process_id"].as_u64() == Some(u64::from(child.id()))
        {
            println!("{}", json!({"process_id":child.id(),"ready":true,"log":path}));
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "background process {} has not reported ready; inspect {}",
                child.id(),
                path.display()
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn record_background_launch<T>(output: &mut impl Write, result: Result<T>) -> Result<T> {
    if let Err(error) = &result {
        // This runs in the short-lived launcher, before the child's redirected
        // stderr exists. Logging must never replace the original Windows error.
        let _ = writeln!(output, "moon: background launch failed: {error:#}");
    }
    result
}

pub fn run(options: Options) -> Result<()> {
    let started = Instant::now();
    if let Some(pid) = options.wait_parent
        && let Ok(parent) = runtime::Process::open(pid)
    {
        parent.wait();
    }
    let _com = platform::Apartment::new()?;
    let _ = platform::enable_dpi_awareness(); // Manifest may already establish PMv2.
    let _instance = runtime::Instance::acquire()?;
    let path = journal::default_path();
    let mut startup_recovery = StartupRecovery { path: path.clone(), armed: true };
    let competitors = competing_managers()?;
    if options.only.is_none() && !competitors.is_empty() {
        if options.takeover {
            takeover(&competitors)?;
        } else {
            bail!(
                "another window manager is running: {}; exit it or use --takeover",
                competitors.join(", ")
            );
        }
    }
    let config = load_config(&options)?;
    let birth = runtime::process_birth(process::id())?;
    let mut record = Journal::new(birth);
    let mut world =
        World::new(config.clone(), platform::monitors()?).map_err(anyhow::Error::msg)?;
    world.native_fullscreen = platform::native_fullscreen()?;
    if path.exists() {
        if options.resume {
            record = Journal::read(&path)?;
            let mut saved: World =
                serde_json::from_value(record.world.clone()).context("read restart state")?;
            saved.config.validate().map_err(anyhow::Error::msg)?;
            saved.check().map_err(anyhow::Error::msg)?;
            let mut stale = Vec::new();
            for &id in saved.windows.keys() {
                let Some(original) = record.windows.get(&id) else {
                    stale.push(id);
                    continue;
                };
                if platform::process_id(id) != original.process_id
                    || !runtime::process_matches(original.process_id, original.process_birth)
                        .with_context(|| format!("verify recovery identity for window {id}"))?
                {
                    stale.push(id);
                } else if !platform::can_manage_process(original.process_id) {
                    // Preserve the complete handoff for an appropriately signed
                    // successor instead of partially restoring it with this token.
                    startup_recovery.armed = false;
                    bail!(
                        "cannot resume window {id} with this process token; use the signed UIAccess installation"
                    );
                }
            }
            for id in stale {
                saved = world::reduce(&saved, Event::WindowRemoved(id))
                    .map_err(anyhow::Error::msg)?
                    .world;
                record.windows.remove(&id);
            }
            saved = world::reduce(&saved, Event::MonitorsChanged(platform::monitors()?))
                .map_err(anyhow::Error::msg)?
                .world;
            world = world::reduce(&saved, Event::ConfigChanged(Box::new(config)))
                .map_err(anyhow::Error::msg)?
                .world;
            record.owner = process::id();
            record.owner_birth = birth;
            record.successor = None;
        } else {
            journal::restore(&path, false)?;
        }
    }
    if options.no_surfaces || (options.only.is_some() && !options.surfaces) {
        world.config.bar.enabled = false;
        world.config.borders.enabled = false;
    }
    record.world = serde_json::to_value(&world)?;
    // A successor may publish ownership only once its replacement watcher exists.
    let mut watchdog_command = runtime::internal_command()?;
    watchdog_command
        .args(["watchdog", &process::id().to_string(), &birth.to_string()])
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut watchdog = runtime::spawn_internal(&mut watchdog_command, runtime::InternalStdio::Null)
        .context("start recovery watchdog")?;
    record.save(&path)?;
    let journal_writer = journal::JournalWriter::start(path.clone())?;
    let main = runtime::message_thread();
    runtime::install_shutdown_handler(main)?;
    let (tx, rx) = mpsc::channel();
    let (hook_tx, hook_rx) = mpsc::channel();
    let hook = Hooks::start(bindings(&world), hook_tx, options.hook_enabled())?;
    hook.set_pointer_tracking(world.config.focus_follows_cursor && options.only.is_none())?;
    forward(hook_rx, tx.clone(), main, Intake::Platform);
    let (pipe_tx, pipe_rx) = mpsc::channel();
    let _pipe = PipeServer::start(pipe_tx)?;
    forward(pipe_rx, tx.clone(), main, Intake::Request);
    let surfaces = if options.no_surfaces || (options.only.is_some() && !options.surfaces) {
        None
    } else {
        Some(Surfaces::new()?)
    };
    let provider_tx = tx.clone();
    let (provider_config, provider_rx) = mpsc::channel();
    if surfaces.is_some() {
        provider_config.send(world.config.clone())?;
        thread::spawn(move || {
            let mut providers = Providers::new();
            loop {
                if let Some(config) = provider_rx.try_iter().last() {
                    providers.set_config(&config);
                }
                if provider_tx.send(Intake::Providers(providers.sample())).is_err() {
                    break;
                }
                runtime::wake(main);
                thread::sleep(Duration::from_secs(1));
            }
        });
    }
    let config_modified = modified(&config_path(&options));
    let mut daemon = Daemon {
        options,
        world,
        journal: record,
        journal_writer: Some(journal_writer),
        path,
        applied: BTreeMap::new(),
        hooks: hook,
        surfaces,
        carousel: Carousel::new()?,
        values: BTreeMap::new(),
        error: None,
        transactions: VecDeque::new(),
        subscribers: Vec::new(),
        started,
        config_modified,
        quit: false,
        restart: false,
        provider_config,
        refusals: BTreeMap::new(),
        placement_started: BTreeMap::new(),
        pending_input: VecDeque::new(),
        workspace_shortcuts: WorkspaceShortcuts::default(),
        key_latencies: VecDeque::new(),
        dispatch_latencies: VecDeque::new(),
        ready_us: 0,
        session_suspended: !platform::session_interactive().unwrap_or(false),
        undo: moon_core::desktop::LayoutUndo::default(),
        undo_births: BTreeMap::new(),
    };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
        daemon.adopt_existing()?;
        daemon.refresh_surfaces()?;
        daemon.commands(daemon.world.config.hooks.startup.clone())?;
        daemon.ready_us = started.elapsed().as_micros();
        println!(
            "moon ready: {} windows, {} monitors; emergency Ctrl+Alt+Shift+F12",
            daemon.world.windows.len(),
            daemon.world.monitors.len()
        );
        let mut maintenance = Instant::now();
        let mut intake_backlog = false;
        while !daemon.quit {
            let wait_ms = if intake_backlog {
                0
            } else {
                let idle = if daemon.carousel.is_active() { 16 } else { 1000 };
                daemon
                    .workspace_shortcuts
                    .wait_ms(Instant::now())
                    .map_or(idle, |wait| wait.min(idle))
            };
            if !runtime::pump(wait_ms) || runtime::shutdown_requested() {
                break;
            }
            let mut changed = BTreeMap::new();
            let mut received_count = 0;
            let mut workspace_received = false;
            for input in rx.try_iter().take(4096) {
                received_count += 1;
                match input {
                    Intake::Platform(PlatformEvent::Window { kind, id }) => {
                        changed.insert((id, kind), ());
                    }
                    Intake::Platform(PlatformEvent::Commands { commands, received }) => {
                        let shortcut = Shortcut { commands, received };
                        if workspace_shortcuts::is_absolute_workspace(
                            &daemon.world.config,
                            &shortcut.commands,
                        ) {
                            if !daemon.session_suspended && !daemon.world.paused {
                                daemon.workspace_shortcuts.submit(shortcut);
                                if !workspace_received {
                                    // Start promptly; later targets in this drain
                                    // replace one another before the next deadline.
                                    daemon.flush_workspace_shortcut(&mut changed, false);
                                }
                                workspace_received = true;
                            }
                        } else {
                            daemon.flush_workspace_shortcut(&mut changed, true);
                            daemon.flush_windows(&mut changed);
                            daemon.dispatch_shortcut(shortcut);
                        }
                    }
                    Intake::Platform(PlatformEvent::EmergencyRestore) => {
                        daemon.workspace_shortcuts.clear();
                        daemon.quit = true;
                        break;
                    }
                    Intake::Platform(PlatformEvent::HookError(error)) => {
                        daemon.error = Some(error);
                    }
                    Intake::Platform(PlatformEvent::System(event)) => {
                        if matches!(
                            event,
                            SystemEvent::SessionLocked
                                | SystemEvent::Suspended
                                | SystemEvent::SessionEnding
                        ) {
                            daemon.workspace_shortcuts.clear();
                        }
                        if let Err(error) = daemon.system_event(event) {
                            daemon.report(error);
                        }
                        if daemon.session_suspended || daemon.quit {
                            daemon.workspace_shortcuts.clear();
                        }
                    }
                    Intake::Platform(PlatformEvent::PointerMoved) => {
                        if daemon.world.config.focus_follows_cursor {
                            daemon.flush_workspace_shortcut(&mut changed, true);
                        }
                        let point = daemon.hooks.take_pointer();
                        if !daemon.session_suspended
                            && daemon.world.config.focus_follows_cursor
                            && let Some(id) = platform::window_at_point(point)
                            && daemon.world.windows.contains_key(&id)
                            && daemon.world.focused != Some(id)
                            && let Err(error) =
                                daemon.commit(Event::WindowFocused(id), "pointer-focus")
                        {
                            daemon.report(error);
                        }
                    }
                    Intake::Request(request) => {
                        daemon.flush_workspace_shortcut(&mut changed, true);
                        daemon.flush_windows(&mut changed);
                        daemon.request(request);
                    }
                    Intake::Providers(values) => {
                        daemon.values = values;
                    }
                }
            }
            intake_backlog = received_count == 4096;
            if daemon.quit {
                break;
            }
            daemon.flush_windows(&mut changed);
            // A full input batch skips sleeping, never a due workspace dispatch.
            // Otherwise a sustained producer could postpone its final target.
            daemon.flush_workspace_shortcut(&mut changed, false);
            if let Err(error) = daemon.carousel.tick() {
                daemon.carousel.cancel();
                daemon.report(error.into());
            }
            daemon.measure_placement();
            let clicks = daemon.surfaces.as_ref().map(Surfaces::drain_commands).unwrap_or_default();
            if !clicks.is_empty() {
                daemon.flush_workspace_shortcut(&mut changed, true);
                if let Err(error) = daemon.commands(clicks) {
                    daemon.report(error);
                }
            }
            if maintenance.elapsed() >= Duration::from_secs(1) {
                daemon.flush_workspace_shortcut(&mut changed, true);
                daemon.maintain()?;
                maintenance = Instant::now();
            }
            daemon.refresh_surfaces()?;
        }
        Ok(())
    }));
    daemon.carousel.cancel();
    if daemon.restart && matches!(&outcome, Ok(Ok(()))) {
        let mut restart = runtime::internal_command()?;
        restart.args(["start", "--resume", "--wait-parent", &process::id().to_string()]);
        add_options(&mut restart, &daemon.options);
        restart.stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
        match runtime::spawn_internal(&mut restart, runtime::InternalStdio::Log) {
            Ok(mut child) => {
                daemon.journal.successor = Some(child.id());
                let writer = daemon.journal_writer.as_ref().expect("writer exists until shutdown");
                match writer.submit(daemon.journal.clone()).and_then(|_| writer.flush()) {
                    Ok(()) => {
                        daemon.journal_writer.take();
                        startup_recovery.armed = false;
                        return Ok(());
                    }
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        daemon.journal.successor = None;
                        daemon.report(error.context("restart handoff was not saved"));
                    }
                }
            }
            Err(error) => daemon.report(error.into()),
        }
    }
    let _ = daemon.commands(daemon.world.config.hooks.shutdown.clone());
    // Remove AppBars and native border modifications before restoring positions.
    daemon.surfaces.take();
    // No queued checkpoint may recreate the journal after restoration removes it.
    daemon.journal_writer.take();
    let recovery = journal::restore(&daemon.path, true);
    if recovery.is_ok() {
        let _ = watchdog.kill();
        let _ = watchdog.wait();
    }
    recovery?;
    startup_recovery.armed = false;
    match outcome {
        Ok(result) => result,
        Err(_) => bail!("window manager panicked; original windows restored"),
    }
}

impl Daemon {
    fn dispatch_shortcut(&mut self, shortcut: Shortcut) {
        if self.pending_input.len() == 256 {
            self.pending_input.pop_front();
        }
        self.pending_input.push_back(shortcut.received);
        self.dispatch_latencies
            .push_back(shortcut.received.elapsed().as_micros().min(u128::from(u64::MAX)) as u64);
        if self.dispatch_latencies.len() > 256 {
            self.dispatch_latencies.pop_front();
        }
        if let Err(error) = self.run_commands(shortcut.commands.to_vec(), true) {
            self.report(error);
        }
    }
    fn flush_workspace_shortcut(
        &mut self,
        changed: &mut BTreeMap<(WindowId, u32), ()>,
        boundary: bool,
    ) {
        if self.session_suspended || self.world.paused || self.quit {
            self.workspace_shortcuts.clear();
            return;
        }
        let pending = if boundary {
            self.workspace_shortcuts.flush()
        } else {
            self.workspace_shortcuts.take_due(Instant::now())
        };
        if let Some(shortcut) = pending {
            self.flush_windows(changed);
            self.dispatch_shortcut(shortcut);
        }
    }
    fn adopt_existing(&mut self) -> Result<()> {
        let started = Instant::now();
        let foreground = platform::foreground();
        let mut next = self.world.clone();
        let mut effects = Plan::default();
        let mut changed = false;
        for info in platform::enumerate_windows()? {
            if !self.in_scope(&info.process)
                || next.windows.contains_key(&info.id)
                || platform::cloaked(info.id).unwrap_or(0) != 0
                || !platform::can_manage_process(info.process_id)
            {
                continue;
            }
            let transition =
                world::reduce(&next, Event::WindowAdded(info)).map_err(anyhow::Error::msg)?;
            next = transition.world;
            effects.close.extend(transition.plan.close);
            effects.launch.extend(transition.plan.launch);
            changed = true;
            if let Some(control) = transition.plan.control {
                let mut plan = next.plan();
                plan.close = std::mem::take(&mut effects.close);
                plan.launch = std::mem::take(&mut effects.launch);
                plan.control = Some(control);
                self.commit_transition(world::Transition { world: next, plan }, "adopt", started)?;
                next = self.world.clone();
                changed = false;
                if self.quit {
                    return Ok(());
                }
            }
        }
        if changed {
            if !self.options.resume
                && let Some(id) = foreground
                && next.windows.get(&id).is_some_and(|window| !window.suspended)
            {
                next = world::reduce(&next, Event::WindowFocused(id))
                    .map_err(anyhow::Error::msg)?
                    .world;
            }
            let mut plan = next.plan();
            if !self.options.resume {
                plan.focus = foreground.filter(|id| next.windows.contains_key(id));
            }
            plan.close = effects.close;
            plan.launch = effects.launch;
            self.commit_transition(world::Transition { world: next, plan }, "adopt", started)?;
        } else if !self.quit {
            let mut plan = self.world.plan();
            if !self.options.resume {
                plan.focus = foreground.filter(|id| self.world.windows.contains_key(id));
            }
            self.apply(plan, false)?;
        }
        Ok(())
    }
    fn in_scope(&self, process: &str) -> bool {
        self.options.only.as_ref().is_none_or(|only| only.eq_ignore_ascii_case(process))
    }
    fn report(&mut self, error: anyhow::Error) {
        eprintln!("moon: {error:#}");
        self.error = Some(format!("{error:#}"));
    }
    fn refresh_surfaces(&mut self) -> Result<()> {
        if !self.session_suspended
            && let Some(surfaces) = &mut self.surfaces
        {
            surfaces.update(&self.world, &self.values, self.error.as_deref())?;
        }
        Ok(())
    }
    fn commit(&mut self, event: Event, kind: &str) -> Result<()> {
        let started = Instant::now();
        let invalidates_undo = moon_core::desktop::LayoutUndo::invalidates(&event);
        let label = match &event {
            Event::Command(command) => command.layout_label(),
            Event::WindowAction { action, .. } => action.layout_label(),
            _ => None,
        };
        let transition = world::reduce(&self.world, event).map_err(anyhow::Error::msg)?;
        let revision = transition.world.revision;
        let result = self.commit_edit(transition, kind, started, label);
        if invalidates_undo && self.world.revision == revision {
            self.undo.clear();
            self.undo_births.clear();
        }
        result
    }
    fn commit_edit(
        &mut self,
        transition: world::Transition,
        kind: &str,
        started: Instant,
        label: Option<&str>,
    ) -> Result<()> {
        let before = label.map(|_| self.world.clone());
        let revision = transition.world.revision;
        let result = self.commit_transition(transition, kind, started);
        // A failed native effect can follow a successful durable state commit.
        // Keep the inverse for that committed edit, never for a failed journal write.
        if self.world.revision == revision
            && let (Some(before), Some(label)) = (before, label)
            && self.undo.record(&before, &self.world, label)
        {
            self.undo_births = self
                .world
                .windows
                .keys()
                .filter_map(|id| {
                    self.journal.windows.get(id).map(|original| (*id, original.process_birth))
                })
                .collect();
        }
        result
    }
    fn commit_transition(
        &mut self,
        transition: world::Transition,
        kind: &str,
        started: Instant,
    ) -> Result<()> {
        let reduce_us = started.elapsed().as_micros();
        let carousel = self.carousel_transitions(&transition.world, &transition.plan, kind);
        let journal_started = Instant::now();
        let previous_mode = self.world.mode.clone();
        let previous_paused = self.world.paused;
        let previous_monitor = self.world.active_monitor;
        let mut record = Journal {
            version: self.journal.version,
            owner: self.journal.owner,
            owner_birth: self.journal.owner_birth,
            successor: self.journal.successor,
            windows: self.journal.windows.clone(),
            world: serde_json::to_value(&transition.world)?,
        };
        let mut originals_changed = false;
        for (&id, window) in &transition.world.windows {
            if let std::collections::btree_map::Entry::Vacant(entry) = record.windows.entry(id) {
                originals_changed = true;
                entry.insert(OriginalWindow {
                    id,
                    process_id: window.info.process_id,
                    process_birth: runtime::process_birth(window.info.process_id)?,
                    rect: window.original_rect,
                    native: Some(platform::capture_placement(id)?),
                });
            }
        }
        let previous_originals = record.windows.len();
        record.windows.retain(|id, original| {
            transition.world.windows.contains_key(id)
                || platform::process_id(*id) == original.process_id
        });
        let writer = self.journal_writer.as_ref().expect("writer exists until shutdown");
        writer.submit(record.clone())?;
        if originals_changed || record.windows.len() != previous_originals {
            // Original placements must be durable before any native effect.
            writer.flush()?;
        }
        self.journal = record;
        let journal_us = journal_started.elapsed().as_micros();
        let animated = if carousel.is_empty() {
            // Unrelated native echo events must not cut short an active transition.
            if self.carousel.is_active()
                && (matches!(
                    kind,
                    "command"
                        | "foreground"
                        | "pointer-focus"
                        | "config"
                        | "display"
                        | "native-fullscreen"
                        | "visibility"
                        | "stale-window"
                        | "destroy"
                        | "create"
                ) || self.world.plan().placements != transition.plan.placements
                    || transition.world.paused
                    || transition.world.native_fullscreen.is_some()
                    || transition.plan.control.is_some())
            {
                self.carousel.cancel();
            }
            false
        } else {
            match self.carousel.prepare(&carousel) {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.carousel.cancel();
                    self.report(error.into());
                    false
                }
            }
        };
        if matches!(kind, "config" | "display" | "adopt")
            || !transition.plan.close.is_empty()
            || !transition.plan.launch.is_empty()
            || (kind == "window"
                && self.world.windows.iter().any(|(id, old)| {
                    transition
                        .world
                        .windows
                        .get(id)
                        .is_some_and(|new| old.floating_rect != new.floating_rect)
                }))
        {
            self.undo.clear();
        }
        self.undo.reconcile(&transition.world);
        if self.undo.status(&transition.world, false).label.is_none() {
            self.undo_births.clear();
        }
        self.world = transition.world;
        let apply_started = Instant::now();
        if let Err(error) = self.apply(transition.plan, kind == "command") {
            self.carousel.cancel();
            return Err(error);
        }
        if animated && let Err(error) = self.carousel.animate() {
            self.carousel.cancel();
            self.report(error.into());
        }
        let apply_us = apply_started.elapsed().as_micros();
        if !self.session_suspended
            && self.world.config.cursor_jump
            && self.world.active_monitor != previous_monitor
            && let Some(monitor) = self.world.monitor(self.world.active_monitor)
        {
            platform::cursor_jump(monitor.work_area.center())?;
        }
        if kind == "config"
            || self.world.mode != previous_mode
            || self.world.paused != previous_paused
        {
            self.hooks.update_bindings(bindings(&self.world))?;
        }
        self.transactions.push_back(json!({"revision":self.world.revision,"event":kind,"elapsed_us":started.elapsed().as_micros(),"reduce_us":reduce_us,"journal_us":journal_us,"apply_us":apply_us,"windows":self.world.windows.len()}));
        if self.transactions.len() > 256 {
            self.transactions.pop_front();
        }
        let revision = self.world.revision;
        if !self.subscribers.is_empty() {
            let state = serde_json::to_value(&self.world)?;
            self.subscribers.retain(|(id, sender)| {
                sender.try_send(response(id.clone(), Ok(state.clone()), revision)).is_ok()
            });
        }
        Ok(())
    }
    fn carousel_transitions(
        &self,
        next: &World,
        plan: &Plan,
        kind: &str,
    ) -> Vec<CarouselTransition> {
        if self.session_suspended
            || self.world.paused
            || next.paused
            || !self.world.config.workspace_animation
            || !next.config.workspace_animation
            || self.world.native_fullscreen.is_some()
            || next.native_fullscreen.is_some()
            || !matches!(kind, "command" | "foreground" | "pointer-focus")
            || self.world.active == next.active
        {
            return Vec::new();
        }
        let previous = self.world.plan();
        let mut transitions = Vec::new();
        for monitor in &next.monitors {
            let Some(old_monitor) = self.world.monitor(monitor.id) else { continue };
            if old_monitor.bounds != monitor.bounds || old_monitor.work_area != monitor.work_area {
                continue;
            }
            let (Some(&old_workspace), Some(&new_workspace)) =
                (self.world.active.get(&monitor.id), next.active.get(&monitor.id))
            else {
                continue;
            };
            if old_workspace == new_workspace {
                continue;
            }
            // A workspace is one canvas. Preserve tile geometry and place floating
            // windows above tiles; the focused floating window is drawn last.
            let page = |state: &World, placements: &[Placement], workspace| {
                let mut placements: Vec<_> = placements
                    .iter()
                    .filter(|p| {
                        p.visible
                            && state.windows.get(&p.id).is_some_and(|w| w.workspace == workspace)
                    })
                    .collect();
                placements.sort_by_key(|p| (p.floating, state.focused == Some(p.id)));
                placements
                    .into_iter()
                    .map(|placement| {
                        let window = state.windows.get(&placement.id)?;
                        if placement.fullscreen
                            || !self.owns_native_window(placement.id)
                            || platform::process_id(placement.id) != window.info.process_id
                            || !platform::window_on_current_virtual_desktop(placement.id)
                                .unwrap_or(false)
                        {
                            return None;
                        }
                        Some(PageWindow {
                            window: WindowRef {
                                id: placement.id,
                                process_id: window.info.process_id,
                            },
                            rect: placement.rect,
                        })
                    })
                    .collect::<Option<Vec<_>>>()
            };
            let (Some(from), Some(to)) = (
                page(&self.world, &previous.placements, old_workspace),
                page(next, &plan.placements, new_workspace),
            ) else {
                continue;
            };
            if from.is_empty() && to.is_empty() {
                continue;
            }
            let direction = if new_workspace.index() >= old_workspace.index() { 1 } else { -1 };
            let mut viewport = monitor.work_area;
            if next.config.bar_enabled_on(&monitor.name) {
                let bottom = monitor.bounds.y
                    + (f64::from(next.config.bar_height_on(&monitor.name)) * monitor.scale).round()
                        as i32;
                let inset = (bottom - viewport.y).max(0).min(viewport.height - 1);
                viewport.y += inset;
                viewport.height -= inset;
            }
            transitions.push(CarouselTransition {
                from_workspace: old_workspace,
                to_workspace: new_workspace,
                from,
                to,
                viewport,
                direction,
                background_rgb: next
                    .config
                    .color_for("base", self.values.get("system.theme").map(String::as_str))
                    .unwrap_or(0x141417),
            });
        }
        transitions
    }
    fn apply(&mut self, plan: Plan, require_foreground_for_close: bool) -> Result<()> {
        if !self.session_suspended {
            let live: BTreeSet<_> = plan.placements.iter().map(|p| p.id).collect();
            self.applied.retain(|id, _| live.contains(id));
            self.placement_started.retain(|id, _| live.contains(id));
            self.refusals.retain(|id, _| live.contains(id));
            let mut changes = Vec::new();
            for placement in plan.placements {
                if !self.owns_native_window(placement.id) {
                    continue;
                }
                if self.applied.get(&placement.id) == Some(&placement) {
                    continue;
                }
                if !platform::window_on_current_virtual_desktop(placement.id).unwrap_or(false) {
                    continue;
                }
                let hidden = placement.visible && platform::cloaked(placement.id).unwrap_or(0) != 0;
                changes.push((placement, hidden));
            }
            // Query the shell before posting native moves. Interleaving those
            // synchronous queries with async placement serializes Explorer's updates.
            for (placement, hidden) in changes {
                if !self.owns_native_window(placement.id) {
                    continue;
                }
                let result = if placement.visible {
                    let uncloak = if hidden {
                        platform::set_hidden(placement.id, false).map(|_| ())
                    } else {
                        Ok(())
                    };
                    uncloak.and_then(|_| platform::place(placement.id, placement.rect))
                } else {
                    platform::set_hidden(placement.id, true).map(|_| ())
                };
                match result {
                    Ok(()) => {
                        self.placement_started.insert(placement.id, Instant::now());
                        self.refusals.remove(&placement.id);
                        self.applied.insert(placement.id, placement);
                    }
                    Err(error) => {
                        self.error =
                            Some(format!("Window {} could not be placed: {error}", placement.id));
                    }
                }
            }
            if let Some(id) = plan.focus
                && self.owns_native_window(id)
                && !platform::is_minimized(id)
                && platform::window_on_current_virtual_desktop(id).unwrap_or(false)
                && self.world.windows.get(&id).is_some_and(|window| !window.suspended)
                && let Err(error) = platform::focus(id)
            {
                self.error = Some(format!("Focus was refused: {error}"));
            }
            for id in plan.close {
                if require_foreground_for_close && platform::foreground() != Some(id) {
                    bail!("The foreground window changed before it could be closed");
                }
                if self.owns_native_window(id) {
                    platform::close(id)?;
                }
            }
            for command in plan.launch {
                runtime::shell_launch(&command)?;
            }
        }
        if let Some(control) = plan.control {
            match control {
                Control::Exit | Control::Restore => self.quit = true,
                Control::Restart => {
                    self.restart = true;
                    self.quit = true;
                }
                Control::Reload => self.reload()?,
                Control::CheatSheet => self.cheatsheet()?,
                Control::Settings => crate::open_settings(&self.options)?,
                Control::Overview => crate::open_settings_page(&self.options, Some("overview"))?,
                Control::Undo => self.undo_layout()?,
            }
        }
        Ok(())
    }
    fn flush_resizes(&mut self, pending: &mut Option<(world::Transition, Instant)>) -> Result<()> {
        if let Some((transition, started)) = pending.take() {
            self.commit_edit(transition, "command", started, Some("Resize window"))?;
        }
        Ok(())
    }
    fn commands(&mut self, commands: Vec<String>) -> Result<()> {
        self.run_commands(commands, false)
    }

    fn run_commands(&mut self, commands: Vec<String>, from_keyboard: bool) -> Result<()> {
        use moon_core::command::Command;
        let mut pending: Option<(world::Transition, Instant)> = None;
        for text in commands {
            if text == "open-config" {
                self.flush_resizes(&mut pending)?;
                let path = config_path(&self.options);
                if !path.exists() {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                        Ok(mut file) => file.write_all(b"config-version = 1\n")?,
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error.into()),
                    }
                }
                runtime::shell_execute("notepad.exe", &format!("\"{}\"", path.display()))?;
                continue;
            }
            let commands = match self.world.config.commands(&text) {
                Ok(commands) => commands,
                Err(error) => {
                    // Earlier valid commands retain their effects when a later
                    // command fails to parse, as with the unbatched path.
                    self.flush_resizes(&mut pending)?;
                    return Err(anyhow::Error::msg(error));
                }
            };
            for command in commands {
                if command == Command::Undo {
                    self.flush_resizes(&mut pending)?;
                    self.undo_layout()?;
                    continue;
                }
                let needs_foreground = command == Command::Close
                    || (from_keyboard && keyboard_targets_window(&command));
                if needs_foreground
                    && (self.world.focused.is_none()
                        || self.world.focused != platform::foreground())
                {
                    self.flush_resizes(&mut pending)?;
                    bail!("No managed window is in the foreground");
                }
                if matches!(&command, Command::Resize { .. }) {
                    let started =
                        pending.as_ref().map_or_else(Instant::now, |(_, started)| *started);
                    let current = pending.as_ref().map_or(&self.world, |(next, _)| &next.world);
                    match world::reduce(current, Event::Command(command)) {
                        Ok(transition) => pending = Some((transition, started)),
                        Err(error) => {
                            self.flush_resizes(&mut pending)?;
                            return Err(anyhow::Error::msg(error));
                        }
                    }
                } else {
                    self.flush_resizes(&mut pending)?;
                    self.commit(Event::Command(command), "command")?;
                }
            }
        }
        self.flush_resizes(&mut pending)
    }
    fn reload(&mut self) -> Result<()> {
        let path = config_path(&self.options);
        // A manual reload also consumes the file change observed by the watcher.
        // Capture before reading so a concurrent replacement gets another pass.
        self.config_modified = modified(&path);
        let mut config = match load_config(&self.options) {
            Ok(config) => config,
            Err(error) => {
                self.report(error);
                return Ok(());
            }
        };
        if (self.options.only.is_some() && !self.options.surfaces) || self.options.no_surfaces {
            config.bar.enabled = false;
            config.borders.enabled = false;
        }
        let started = Instant::now();
        let transition = match world::reduce(&self.world, Event::ConfigChanged(Box::new(config))) {
            Ok(transition) => transition,
            Err(error) => {
                self.report(
                    anyhow::Error::msg(error).context(format!("config {}", path.display())),
                );
                return Ok(());
            }
        };
        self.commit_transition(transition, "config", started)?;
        self.hooks.set_pointer_tracking(
            self.world.config.focus_follows_cursor && self.options.only.is_none(),
        )?;
        let _ = self.provider_config.send(self.world.config.clone());
        self.error = None;
        self.commands(self.world.config.hooks.reload.clone())
    }
    fn cheatsheet(&self) -> Result<()> {
        if crate::settings_companion_present(&crate::settings_companion()?)? {
            return crate::open_settings_page(&self.options, Some("shortcuts"));
        }
        let path = journal::data_dir().join("bindings.txt");
        fs::write(
            &path,
            moon_core::help::shortcuts(&self.world, self.options.hook_enabled()).text(),
        )?;
        runtime::shell_execute("notepad.exe", &format!("\"{}\"", path.display()))?;
        Ok(())
    }

    fn window_event(&mut self, id: WindowId, kind: u32) -> Result<()> {
        // A queued event can outlive its HWND. Check its current owner before
        // accepting placement echoes, changing suspension, or issuing effects.
        if self.world.windows.contains_key(&id) && !self.owns_native_window(id) {
            self.commit(Event::WindowRemoved(id), "stale-window")?;
        }
        if kind == 3 || (kind == 0x800b && platform::foreground() == Some(id)) {
            if kind == 3
                && platform::foreground() == Some(id)
                && !self.world.windows.contains_key(&id)
            {
                self.carousel.cancel();
            }
            self.sync_native_fullscreen()?;
        }
        if kind == 0x800b
            && self.applied.get(&id).is_some_and(|p| {
                p.visible && platform::window_rect(id).is_ok_and(|rect| rect == p.rect)
            })
        {
            // A completed native move echoes the already committed placement.
            // Name, visibility, desktop and drag-end changes have separate events.
            return Ok(());
        }
        if self.world.windows.contains_key(&id) {
            self.sync_suspension(id)?;
        }
        if kind == 0x8001 || !platform::is_window(id) {
            if self.world.windows.contains_key(&id) {
                self.commit(Event::WindowRemoved(id), "destroy")?;
                self.journal.windows.remove(&id);
            }
        } else if kind == 3 {
            if platform::foreground() == Some(id)
                && self.world.windows.contains_key(&id)
                && self.world.focused != Some(id)
            {
                self.commit(Event::WindowFocused(id), "foreground")?;
            }
        } else if let Ok(info) = platform::window_info(id) {
            if !self.in_scope(&info.process) {
                return Ok(());
            }
            if let Some(current) = self.world.windows.get(&id) {
                let echo = self.applied.get(&id).is_some_and(|p| !p.visible || p.rect == info.rect);
                // Location echoes are verified by maintenance. Interactive drag end
                // invalidates the diff so tiling can reclaim the window immediately.
                let moved = kind == 0x000b && !echo;
                if moved {
                    self.applied.remove(&id);
                }
                if current.info.title != info.title
                    || current.info.class != info.class
                    || moved
                    || (current.floating && !echo)
                {
                    self.commit(Event::WindowChanged(info), "window")?;
                }
            } else if platform::is_manageable(id)
                && platform::cloaked(id).unwrap_or(0) == 0
                && platform::can_manage_process(info.process_id)
            {
                self.commit(Event::WindowAdded(info), "create")?;
            }
        }
        Ok(())
    }
    fn flush_windows(&mut self, changed: &mut BTreeMap<(WindowId, u32), ()>) {
        for ((id, kind), ()) in std::mem::take(changed) {
            if let Err(error) = self.window_event(id, kind) {
                self.report(error);
            }
        }
    }
    fn measure_placement(&mut self) {
        if self.pending_input.is_empty() {
            return;
        }
        let placed = self.applied.values().all(|p| {
            if !p.visible {
                return platform::cloaked(p.id).is_ok_and(|v| v != 0)
                    || platform::window_rect(p.id).is_ok_and(|r| r.x < -30000);
            }
            platform::window_rect(p.id).is_ok_and(|r| {
                (r.x - p.rect.x).abs() <= 1
                    && (r.y - p.rect.y).abs() <= 1
                    && (r.width - p.rect.width).abs() <= 1
                    && (r.height - p.rect.height).abs() <= 1
            })
        });
        if placed {
            for received in self.pending_input.drain(..) {
                self.key_latencies
                    .push_back(received.elapsed().as_micros().min(u128::from(u64::MAX)) as u64);
                if self.key_latencies.len() > 256 {
                    self.key_latencies.pop_front();
                }
            }
        }
    }
    fn maintain(&mut self) -> Result<()> {
        self.journal_writer.as_ref().expect("writer exists until shutdown").check()?;
        if self.session_suspended {
            if !platform::session_interactive().unwrap_or(false) {
                return Ok(());
            }
            self.session_suspended = false;
            self.carousel.cancel();
            self.applied.clear();
            self.reconcile_desktop()?;
            return self.reapply_without_focus();
        }
        self.reconcile_desktop()
    }
    fn reconcile_desktop(&mut self) -> Result<()> {
        let current = modified(&config_path(&self.options));
        if current != self.config_modified
            && let Err(error) = self.reload()
        {
            self.report(error);
        }
        let monitors = platform::monitors()?;
        if monitors != self.world.monitors {
            let started = Instant::now();
            let transition = match world::reduce(&self.world, Event::MonitorsChanged(monitors)) {
                Ok(transition) => transition,
                Err(error) => {
                    self.report(anyhow::Error::msg(error));
                    return Ok(());
                }
            };
            self.commit_transition(transition, "display", started)?;
        }
        self.sync_native_fullscreen()?;
        for id in self.world.windows.keys().copied().collect::<Vec<_>>() {
            self.sync_suspension(id)?;
        }
        if !self.world.paused {
            for placement in self.world.plan().placements {
                if !placement.visible
                    || placement.floating
                    || !self.owns_native_window(placement.id)
                {
                    continue;
                }
                if let Ok(actual) = platform::window_rect(placement.id) {
                    let target = placement.rect;
                    let mismatch = (actual.x - target.x).abs() > 1
                        || (actual.y - target.y).abs() > 1
                        || (actual.width - target.width).abs() > 1
                        || (actual.height - target.height).abs() > 1;
                    if mismatch {
                        // SetWindowPos posts to another thread. Maintenance may
                        // run in the same turn; an unsettled move is not a refusal.
                        if self
                            .placement_started
                            .get(&placement.id)
                            .is_some_and(|started| started.elapsed() < Duration::from_millis(100))
                        {
                            continue;
                        }
                        let attempts = self.refusals.entry(placement.id).or_default();
                        *attempts += 1;
                        if *attempts >= 3 {
                            self.commit(
                                Event::PlacementRejected(placement.id),
                                "placement-refused",
                            )?;
                            self.refusals.remove(&placement.id);
                        } else {
                            platform::place(placement.id, target)?;
                            self.placement_started.insert(placement.id, Instant::now());
                        }
                    } else {
                        self.refusals.remove(&placement.id);
                        self.placement_started.remove(&placement.id);
                    }
                }
            }
        }
        Ok(())
    }
    fn reapply_without_focus(&mut self) -> Result<()> {
        let mut plan = self.world.plan();
        plan.focus = None;
        self.apply(plan, false)?;
        self.refresh_surfaces()
    }
    fn sync_suspension(&mut self, id: WindowId) -> Result<()> {
        if !self.owns_native_window(id) {
            if self.world.windows.contains_key(&id) {
                self.commit(Event::WindowRemoved(id), "stale-window")?;
            }
            return Ok(());
        }
        let suspended = platform::is_minimized(id)
            || !platform::window_on_current_virtual_desktop(id).unwrap_or(false);
        if self.world.windows.get(&id).is_some_and(|window| window.suspended != suspended) {
            self.applied.remove(&id);
            self.commit(Event::WindowSuspended { id, suspended }, "visibility")?;
        }
        Ok(())
    }
    fn owns_native_window(&self, id: WindowId) -> bool {
        self.world
            .windows
            .get(&id)
            .is_some_and(|window| platform::process_id(id) == window.info.process_id)
    }
    fn sync_native_fullscreen(&mut self) -> Result<()> {
        let native = platform::native_fullscreen()?;
        if let Some((_, monitor)) = native
            && self.world.monitor(monitor).is_none()
        {
            // Display and foreground notifications can arrive in either order.
            let monitors = platform::monitors()?;
            if let Err(error) = self.commit(Event::MonitorsChanged(monitors), "display") {
                self.report(error);
                return Ok(());
            }
            if self.world.monitor(monitor).is_none() {
                return Ok(());
            }
        }
        if native != self.world.native_fullscreen {
            self.commit(Event::NativeFullscreen(native), "native-fullscreen")?;
        }
        Ok(())
    }
    fn system_event(&mut self, event: SystemEvent) -> Result<()> {
        let rechecked_suspension = matches!(
            event,
            SystemEvent::SessionUnlocked | SystemEvent::Resumed | SystemEvent::DesktopChanged
        )
        .then(|| !platform::session_interactive().unwrap_or(false));
        if matches!(
            event,
            SystemEvent::SessionLocked | SystemEvent::Suspended | SystemEvent::SessionEnding
        ) || rechecked_suspension == Some(true)
        {
            self.carousel.suspend();
        } else {
            self.carousel.cancel();
        }
        match event {
            SystemEvent::SessionLocked | SystemEvent::Suspended => self.session_suspended = true,
            SystemEvent::SessionEnding => self.quit = true,
            SystemEvent::SessionUnlocked
            | SystemEvent::Resumed
            | SystemEvent::DesktopChanged
            | SystemEvent::DisplayChanged
            | SystemEvent::SettingsChanged
            | SystemEvent::ShellRestarted => {
                if let Some(suspended) = rechecked_suspension {
                    self.session_suspended = suspended;
                }
                self.applied.clear();
                if matches!(event, SystemEvent::ShellRestarted) {
                    platform::invalidate_virtual_desktop_manager();
                    if self.surfaces.is_some() {
                        self.surfaces.take();
                        self.surfaces = Some(Surfaces::new()?);
                    }
                }
                if !self.session_suspended {
                    self.reconcile_desktop()?;
                    self.reapply_without_focus()?;
                }
            }
        }
        Ok(())
    }
    fn request(&mut self, request: PipeRequest) {
        let message = request.message;
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let result = (|| -> Result<Value> {
            let parsed = moon_core::protocol::Request::parse(&message.to_string())
                .map_err(anyhow::Error::msg)?;
            let focus = self.world.focused.filter(|&id| platform::foreground() == Some(id));
            parsed.check_focus(focus).map_err(anyhow::Error::msg)?;
            match parsed.action {
                moon_core::protocol::RequestAction::Command { commands, .. } => {
                    self.commands(commands)?;
                    Ok(json!({"revision":self.world.revision}))
                }
                moon_core::protocol::RequestAction::Query { query } => self.query(&query),
                moon_core::protocol::RequestAction::Subscribe {} => {
                    self.subscribers.push((id.clone(), request.reply.clone()));
                    Ok(serde_json::to_value(&self.world)?)
                }
                moon_core::protocol::RequestAction::Lifecycle { action } => {
                    use moon_core::protocol::LifecycleAction;
                    let command = match action {
                        LifecycleAction::Restart => moon_core::command::Command::Restart,
                        LifecycleAction::Exit => moon_core::command::Command::Exit,
                    };
                    self.commit(Event::Command(command), "command")?;
                    Ok(json!({"revision":self.world.revision}))
                }
                moon_core::protocol::RequestAction::Window {
                    generation,
                    revision,
                    target,
                    action,
                } => {
                    self.check_desktop_guard(&generation, &revision)?;
                    self.check_interactive_action()?;
                    let id = self.check_target(&target)?;
                    self.commit(Event::WindowAction { id, action }, "command")?;
                    Ok(Value::Null)
                }
                moon_core::protocol::RequestAction::Undo { generation, revision } => {
                    self.check_desktop_guard(&generation, &revision)?;
                    self.undo_layout()?;
                    Ok(Value::Null)
                }
                moon_core::protocol::RequestAction::Control { generation, revision, action } => {
                    use moon_core::{command::Command, protocol::DesktopControl};
                    self.check_desktop_guard(&generation, &revision)?;
                    self.check_session_action()?;
                    let command = match action {
                        DesktopControl::Pause => Command::Pause,
                        DesktopControl::Reload => Command::Reload,
                        DesktopControl::Restore => Command::Restore,
                    };
                    self.commit(Event::Command(command), "command")?;
                    Ok(Value::Null)
                }
            }
        })();
        let _ = request.reply.try_send(response(id, result, self.world.revision));
    }
    fn query(&self, query: &str) -> Result<Value> {
        Ok(match query {
            "world" => serde_json::to_value(&self.world)?,
            "windows" => serde_json::to_value(self.world.plan().placements)?,
            "monitors" => serde_json::to_value(&self.world.monitors)?,
            "config" => serde_json::to_value(&self.world.config)?,
            "restart-config" => serde_json::to_value(crate::restart_config(&self.options)?)?,
            "shortcuts" => serde_json::to_value(moon_core::help::shortcuts(
                &self.world,
                self.options.hook_enabled(),
            ))?,
            "overview" => serde_json::to_value(moon_core::help::overview(&self.world))?,
            "desktop" => serde_json::to_value(moon_core::desktop::catalog(
                &self.world,
                &self
                    .journal
                    .windows
                    .iter()
                    .map(|(&id, original)| (id, original.process_birth))
                    .collect(),
                self.generation(),
                self.session_suspended,
                self.undo.status(&self.world, self.session_suspended),
            ))?,
            "undo" => serde_json::to_value(self.undo.status(&self.world, self.session_suspended))?,
            "transactions" => serde_json::to_value(&self.transactions)?,
            "stats" => {
                json!({"process_id":process::id(),"executable":std::env::current_exe()?,"config_path":config_path(&self.options),"config_explicit":self.options.config.is_some(),"carousel_active":self.carousel.is_active(),"session_suspended":self.session_suspended,"paused":self.world.paused,"token":platform::token_capabilities(process::id())?,"uptime_seconds":self.started.elapsed().as_secs_f64(),"ready_us":self.ready_us,"revision":self.world.revision,"managed_windows":self.world.windows.len(),"resources":runtime::resource_stats()?,"surface_pixel_bytes":self.surfaces.as_ref().map_or(0, Surfaces::surface_pixel_bytes),"renderer":self.surfaces.as_ref().map(Surfaces::diagnostics),"hooks":hooks::stats(),"key_to_placed_us":self.key_latencies,"key_to_dispatch_us":self.dispatch_latencies,"recent_transactions":self.transactions,"last_error":self.error})
            }
            _ => bail!("unknown query {query:?}"),
        })
    }
    fn generation(&self) -> String {
        format!("{}:{}", self.journal.owner, self.journal.owner_birth)
    }
    fn check_desktop_guard(&self, generation: &str, revision: &str) -> Result<()> {
        moon_core::protocol::check_generation(
            generation,
            revision,
            &self.generation(),
            self.world.revision,
        )
        .map_err(anyhow::Error::msg)
    }
    fn check_interactive_action(&self) -> Result<()> {
        if self.world.paused {
            bail!("Resume Moon before changing windows");
        }
        self.check_session_action()
    }
    fn check_session_action(&self) -> Result<()> {
        if self.session_suspended || !platform::session_interactive().unwrap_or(false) {
            bail!("Windows session is suspended; try again after unlocking");
        }
        Ok(())
    }
    fn check_target(&self, target: &moon_core::protocol::WindowTarget) -> Result<WindowId> {
        let (id, process, birth) = target.numbers().map_err(anyhow::Error::msg)?;
        let original = self
            .journal
            .windows
            .get(&id)
            .context("Window is no longer managed; refresh the overview")?;
        let native_process = platform::process_id(id);
        if !runtime::process_matches(process, birth).unwrap_or(false) {
            bail!("Window process changed; refresh the overview and try again");
        }
        target
            .check(
                &self.world,
                original.process_birth,
                native_process,
                runtime::process_birth(process)?,
            )
            .map_err(anyhow::Error::msg)?;
        if platform::process_id(id) != process || original.process_id != process {
            bail!("Window identity changed; refresh the overview and try again");
        }
        if self.world.windows[&id].suspended
            || platform::is_minimized(id)
            || !platform::window_on_current_virtual_desktop(id).unwrap_or(false)
        {
            bail!(
                "This window is minimized or on another Windows desktop; restore it before changing its layout"
            );
        }
        Ok(id)
    }
    fn undo_layout(&mut self) -> Result<()> {
        self.check_interactive_action()?;
        let transition = self.undo.transition(&self.world).map_err(anyhow::Error::msg)?;
        // Recheck every affected native lifetime before restoring a saved layout.
        let checked = self.world.windows.iter().try_for_each(|(&id, window)| -> Result<()> {
            let original =
                self.journal.windows.get(&id).context("Original window identity is unavailable")?;
            if self.undo_births.get(&id) != Some(&original.process_birth) {
                bail!("Window lifetime changed; the layout cannot be undone");
            }
            self.check_target(&moon_core::protocol::WindowTarget {
                id: id.0.to_string(),
                process_id: window.info.process_id.to_string(),
                process_birth: original.process_birth.to_string(),
            })?;
            Ok(())
        });
        if let Err(error) = checked {
            self.undo.clear();
            self.undo_births.clear();
            return Err(error);
        }
        let revision = transition.world.revision;
        let result = self.commit_transition(transition, "command", Instant::now());
        if self.world.revision == revision {
            self.undo.clear();
            self.undo_births.clear();
        }
        result
    }
}

fn keyboard_targets_window(command: &moon_core::command::Command) -> bool {
    use moon_core::command::Command;
    matches!(
        command,
        Command::Move(_)
            | Command::Resize { .. }
            | Command::ToggleFullscreen
            | Command::ToggleFloating
            | Command::SetFloating
            | Command::SetTiling
            | Command::MoveToWorkspace { .. }
            | Command::MoveToMonitor(_)
            | Command::Ignore
    )
}

fn modified(path: &std::path::Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}
fn response(id: Value, result: Result<Value>, revision: u64) -> Value {
    match result {
        Ok(result) => {
            json!({"version":1,"id":id,"ok":true,"result":result,"error":null,"revision":revision})
        }
        Err(error) => {
            json!({"version":1,"id":id,"ok":false,"result":null,"error":format!("{error:#}"),"revision":revision})
        }
    }
}

pub fn client(words: &[String]) -> Result<()> {
    let first = words.first().map(String::as_str).unwrap_or("query");
    let mut request = match first {
        "query" => {
            json!({"version":1,"id":1,"method":"query","query":words.get(1).map(String::as_str).unwrap_or("world")})
        }
        "subscribe" => json!({"version":1,"id":1,"method":"subscribe"}),
        _ => json!({"version":1,"id":1,"method":"command","commands":[words.join(" ")]}),
    };
    let executable = std::env::current_exe()?;
    let own_path = executable
        .file_name()
        .filter(|name| !name.eq_ignore_ascii_case("moon.exe"))
        .map(|_| executable.as_path());
    if first == "window" {
        let catalog = ipc::request_to(
            &json!({"version":1,"id":1,"method":"query","query":"desktop"}),
            None,
            own_path,
        )?;
        if catalog.get("ok").and_then(Value::as_bool) != Some(true) {
            bail!(
                "{}",
                catalog.get("error").and_then(Value::as_str).unwrap_or("Desktop query failed")
            );
        }
        request = crate::window_request(&words[1..], &catalog["result"])?;
    }
    if first == "subscribe" {
        ipc::subscribe_to(&request, own_path, |reply| {
            println!("{reply}");
            true
        })?;
    } else {
        let reply = ipc::request_to(&request, None, own_path)
            .context("connect to moon; start it first with `moon start`")?;
        if reply.get("ok").and_then(Value::as_bool) != Some(true) {
            bail!("{}", reply.get("error").and_then(Value::as_str).unwrap_or("IPC request failed"));
        }
        println!("{}", serde_json::to_string_pretty(&reply["result"])?);
    }
    Ok(())
}

pub fn watchdog(words: &[String]) -> Result<()> {
    if words.len() != 3 {
        bail!("watchdog requires PID, creation time and journal path");
    }
    let pid = words[0].parse::<u32>()?;
    let birth = words[1].parse::<u64>()?;
    let path = PathBuf::from(&words[2]);
    if let Ok(parent) = runtime::Process::open(pid)
        && parent.birth()? == birth
    {
        parent.wait();
    }
    let _com = platform::Apartment::new()?;
    if !path.exists() {
        return Ok(());
    }
    let record = Journal::read(&path)?;
    if record.owner != pid || record.owner_birth != birth {
        return Ok(());
    }
    if let Some(successor) = record.successor {
        for _ in 0..50 {
            thread::sleep(Duration::from_millis(100));
            if !path.exists() || Journal::read(&path).is_ok_and(|j| j.owner == successor) {
                return Ok(());
            }
            if !platform::process_alive(successor) {
                break;
            }
        }
    }
    let _instance = match runtime::Instance::acquire() {
        Ok(instance) => instance,
        Err(error) if runtime::instance_is_running(&error) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    // A new daemon may have adopted the journal during the successor grace period.
    if !path.exists() {
        return Ok(());
    }
    let record = Journal::read(&path)?;
    if record.owner == pid && record.owner_birth == birth {
        journal::restore(&path, false)?;
    }
    Ok(())
}

fn competing_managers() -> Result<Vec<String>> {
    let session = runtime::session_id()?;
    Ok(platform::running_processes()?
        .into_iter()
        .filter_map(|(pid, name)| {
            (platform::process_session(pid).ok() == Some(session)
                && ["glazewm.exe", "komorebi.exe", "whim.exe"]
                    .iter()
                    .any(|candidate| name.eq_ignore_ascii_case(candidate)))
            .then_some(name)
        })
        .collect())
}
fn takeover(competitors: &[String]) -> Result<()> {
    for name in competitors {
        if name.eq_ignore_ascii_case("glazewm.exe") {
            let path =
                PathBuf::from(std::env::var_os("ProgramFiles").context("ProgramFiles missing")?)
                    .join("glzr.io/GlazeWM/glazewm.exe");
            runtime::shell_execute(&path.to_string_lossy(), "command wm-exit")?;
        } else {
            bail!("exit {name} before takeover");
        }
    }
    for _ in 0..50 {
        if competing_managers()?.is_empty() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("previous manager did not exit within five seconds")
}

pub fn doctor(options: &Options, words: &[String]) -> Result<()> {
    let _com = platform::Apartment::new()?;
    if words == ["--token"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&platform::token_capabilities(process::id())?)?
        );
        return Ok(());
    }
    let _ = platform::enable_dpi_awareness();
    let config = load_config(options);
    let report = json!({"version":env!("CARGO_PKG_VERSION"),"session":runtime::session_id()?,"token":platform::token_capabilities(process::id())?,"config_path":config_path(options),"config_valid":config.is_ok(),"config_error":config.err().map(|e|format!("{e:#}")),"competing_managers":competing_managers()?,"monitors":platform::monitors()?,"manageable_windows":platform::enumerate_windows()?,"resources":runtime::resource_stats()?,"journal_exists":journal::default_path().exists(),"uiaccess_build":cfg!(feature="uiaccess")});
    if words.is_empty() {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    bail!("unknown doctor arguments: {}", words.join(" "))
}

#[cfg(test)]
mod background_tests {
    use super::*;

    #[test]
    fn launch_failure_logs_context_and_preserves_windows_error() {
        let mut log = Vec::new();
        let failure: Result<()> =
            Err(std::io::Error::from_raw_os_error(740)).context("spawn background Moon process");
        let error = record_background_launch(&mut log, failure).unwrap_err();
        assert_eq!(error.downcast_ref::<std::io::Error>().unwrap().raw_os_error(), Some(740));
        let text = String::from_utf8(log).unwrap();
        assert!(text.contains("background launch failed: spawn background Moon process"));
        assert!(text.contains("740"));
    }

    #[test]
    fn unavailable_log_does_not_replace_launch_failure() {
        struct Unavailable;
        impl Write for Unavailable {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let failure: Result<()> = Err(std::io::Error::from_raw_os_error(740).into());
        let error = record_background_launch(&mut Unavailable, failure).unwrap_err();
        assert_eq!(error.downcast_ref::<std::io::Error>().unwrap().raw_os_error(), Some(740));
    }
}
