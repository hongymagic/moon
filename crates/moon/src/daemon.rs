use crate::{
    Options, config_path,
    journal::{self, Journal, OriginalWindow},
    load_config,
};
use anyhow::{Context, Result, bail};
use moon_core::{
    ids::WindowId,
    world::{self, Control, Event, Placement, Plan, World},
};
use moon_win::{
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
    key_latencies: VecDeque<u64>,
    dispatch_latencies: VecDeque<u64>,
    ready_us: u128,
    session_suspended: bool,
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
    let mut bindings = world.config.bindings.clone();
    if world.mode != "default"
        && let Some(mode) = world.config.modes.get(&world.mode)
    {
        // A binding mode replaces normal keys; emergency recovery remains independent.
        bindings = mode.clone();
    }
    if world.paused {
        bindings.retain(|_, texts| {
            texts.iter().any(|text| {
                world.config.commands(text).is_ok_and(|commands| {
                    commands.iter().any(|command| {
                        matches!(
                            command,
                            moon_core::command::Command::Pause
                                | moon_core::command::Command::Reload
                                | moon_core::command::Command::Exit
                                | moon_core::command::Command::Restore
                        )
                    })
                })
            })
        });
    }
    bindings.into_iter().map(|(chord, commands)| HookBinding { chord, commands }).collect()
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
    let output = fs::OpenOptions::new().create(true).append(true).open(&path)?;
    let mut command = runtime::internal_command()?;
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
    let mut child =
        command.stdin(Stdio::null()).stdout(output.try_clone()?).stderr(output).spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            bail!("background startup failed ({status}); see {}", path.display());
        }
        if let Ok(reply) = ipc::request(&query)
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
    let mut watchdog = runtime::internal_command()?
        .args(["watchdog", &process::id().to_string(), &birth.to_string()])
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("start recovery watchdog")?;
    record.save(&path)?;
    let journal_writer = journal::JournalWriter::start(path.clone())?;
    let main = runtime::message_thread();
    runtime::install_shutdown_handler(main)?;
    let (tx, rx) = mpsc::channel();
    let (hook_tx, hook_rx) = mpsc::channel();
    let hook = Hooks::start(bindings(&world), hook_tx, !options.no_hook && options.only.is_none())?;
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
        key_latencies: VecDeque::new(),
        dispatch_latencies: VecDeque::new(),
        ready_us: 0,
        session_suspended: !platform::session_interactive().unwrap_or(false),
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
        while !daemon.quit {
            if !runtime::pump(1000) || runtime::shutdown_requested() {
                break;
            }
            let mut changed = BTreeMap::new();
            for input in rx.try_iter().take(4096) {
                match input {
                    Intake::Platform(PlatformEvent::Window { kind, id }) => {
                        changed.insert((id, kind), ());
                    }
                    Intake::Platform(PlatformEvent::Commands { commands, received }) => {
                        daemon.flush_windows(&mut changed);
                        if daemon.pending_input.len() == 256 {
                            daemon.pending_input.pop_front();
                        }
                        daemon.pending_input.push_back(received);
                        hooks::suppress_alt_menu();
                        daemon.dispatch_latencies.push_back(
                            received.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
                        );
                        if daemon.dispatch_latencies.len() > 256 {
                            daemon.dispatch_latencies.pop_front();
                        }
                        if let Err(error) = daemon.commands(commands.to_vec()) {
                            daemon.report(error);
                        }
                    }
                    Intake::Platform(PlatformEvent::EmergencyRestore) => {
                        daemon.quit = true;
                    }
                    Intake::Platform(PlatformEvent::HookError(error)) => {
                        daemon.error = Some(error);
                    }
                    Intake::Platform(PlatformEvent::System(event)) => {
                        if let Err(error) = daemon.system_event(event) {
                            daemon.report(error);
                        }
                    }
                    Intake::Platform(PlatformEvent::PointerMoved) => {
                        let point = daemon.hooks.take_pointer();
                        if daemon.world.config.focus_follows_cursor
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
                        daemon.flush_windows(&mut changed);
                        daemon.request(request);
                    }
                    Intake::Providers(values) => {
                        daemon.values = values;
                    }
                }
            }
            daemon.flush_windows(&mut changed);
            daemon.measure_placement();
            let clicks = daemon.surfaces.as_ref().map(Surfaces::drain_commands).unwrap_or_default();
            if let Err(error) = daemon.commands(clicks) {
                daemon.report(error);
            }
            if maintenance.elapsed() >= Duration::from_secs(1) {
                daemon.maintain()?;
                maintenance = Instant::now();
            }
            daemon.refresh_surfaces()?;
        }
        Ok(())
    }));
    if daemon.restart && matches!(&outcome, Ok(Ok(()))) {
        let mut restart = runtime::internal_command()?;
        restart.args(["start", "--resume", "--wait-parent", &process::id().to_string()]);
        add_options(&mut restart, &daemon.options);
        match restart.stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit()).spawn()
        {
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
            self.apply(plan)?;
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
        if let Some(surfaces) = &mut self.surfaces {
            surfaces.update(&self.world, &self.values, self.error.as_deref())?;
        }
        Ok(())
    }
    fn commit(&mut self, event: Event, kind: &str) -> Result<()> {
        let started = Instant::now();
        let transition = world::reduce(&self.world, event).map_err(anyhow::Error::msg)?;
        self.commit_transition(transition, kind, started)
    }
    fn commit_transition(
        &mut self,
        transition: world::Transition,
        kind: &str,
        started: Instant,
    ) -> Result<()> {
        let reduce_us = started.elapsed().as_micros();
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
        record
            .windows
            .retain(|id, _| transition.world.windows.contains_key(id) || platform::is_window(*id));
        let writer = self.journal_writer.as_ref().expect("writer exists until shutdown");
        writer.submit(record.clone())?;
        if originals_changed || record.windows.len() != previous_originals {
            // Original placements must be durable before any native effect.
            writer.flush()?;
        }
        self.journal = record;
        let journal_us = journal_started.elapsed().as_micros();
        self.world = transition.world;
        let apply_started = Instant::now();
        self.apply(transition.plan)?;
        let apply_us = apply_started.elapsed().as_micros();
        if self.world.config.cursor_jump
            && self.world.active_monitor != previous_monitor
            && let Some(monitor) = self.world.monitor(self.world.active_monitor)
        {
            platform::cursor_jump(monitor.work_area.center())?;
        }
        if self.world.mode != previous_mode || self.world.paused != previous_paused {
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
    fn apply(&mut self, plan: Plan) -> Result<()> {
        if !self.session_suspended {
            let live: BTreeSet<_> = plan.placements.iter().map(|p| p.id).collect();
            self.applied.retain(|id, _| live.contains(id));
            self.placement_started.retain(|id, _| live.contains(id));
            self.refusals.retain(|id, _| live.contains(id));
            let mut changes = Vec::new();
            for placement in plan.placements {
                if self.applied.get(&placement.id) == Some(&placement) {
                    continue;
                }
                if platform::window_on_current_virtual_desktop(placement.id)
                    .is_ok_and(|current| !current)
                {
                    continue;
                }
                let hidden = placement.visible && platform::cloaked(placement.id).unwrap_or(0) != 0;
                changes.push((placement, hidden));
            }
            // Query the shell before posting native moves. Interleaving those
            // synchronous queries with async placement serializes Explorer's updates.
            for (placement, hidden) in changes {
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
                && !platform::is_minimized(id)
                && platform::window_on_current_virtual_desktop(id).unwrap_or(false)
                && self.world.windows.get(&id).is_some_and(|window| !window.suspended)
                && let Err(error) = platform::focus(id)
            {
                self.error = Some(format!("Focus was refused: {error}"));
            }
            for id in plan.close {
                platform::close(id)?;
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
            }
        }
        Ok(())
    }
    fn flush_resizes(&mut self, pending: &mut Option<(world::Transition, Instant)>) -> Result<()> {
        if let Some((transition, started)) = pending.take() {
            self.commit_transition(transition, "command", started)?;
        }
        Ok(())
    }
    fn commands(&mut self, commands: Vec<String>) -> Result<()> {
        let mut pending: Option<(world::Transition, Instant)> = None;
        for text in commands {
            if text == "open-config" {
                self.flush_resizes(&mut pending)?;
                let path = config_path(&self.options);
                if !path.exists() {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&path, "config-version = 1\n")?;
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
                if matches!(&command, moon_core::command::Command::Resize { .. }) {
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
        self.commit(Event::ConfigChanged(Box::new(config)), "config")?;
        self.hooks.update_bindings(bindings(&self.world))?;
        self.hooks.set_pointer_tracking(
            self.world.config.focus_follows_cursor && self.options.only.is_none(),
        )?;
        let _ = self.provider_config.send(self.world.config.clone());
        self.error = None;
        self.commands(self.world.config.hooks.reload.clone())
    }
    fn cheatsheet(&self) -> Result<()> {
        let mut content = String::from(
            "moon — current bindings\r\n\r\nEmergency restore: Ctrl+Alt+Shift+F12\r\n\r\n",
        );
        for binding in bindings(&self.world) {
            content.push_str(&format!("{:<24} {}\r\n", binding.chord, binding.commands.join("; ")));
        }
        let path = journal::data_dir().join("bindings.txt");
        fs::write(&path, content)?;
        runtime::shell_execute("notepad.exe", &format!("\"{}\"", path.display()))?;
        Ok(())
    }
    fn window_event(&mut self, id: WindowId, kind: u32) -> Result<()> {
        if kind == 3 || (kind == 0x800b && platform::foreground() == Some(id)) {
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
        if self.world.windows.contains_key(&id) && platform::is_window(id) {
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
            } else if platform::is_manageable(id) && platform::cloaked(id).unwrap_or(0) == 0 {
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
            return Ok(());
        }
        let current = modified(&config_path(&self.options));
        if current != self.config_modified {
            self.config_modified = current;
            self.reload()?;
        }
        let monitors = platform::monitors()?;
        if monitors != self.world.monitors {
            self.commit(Event::MonitorsChanged(monitors), "display")?;
        }
        self.sync_native_fullscreen()?;
        for id in self.world.windows.keys().copied().collect::<Vec<_>>() {
            self.sync_suspension(id)?;
        }
        if !self.world.paused {
            for placement in self.world.plan().placements {
                if !placement.visible || placement.floating {
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
    fn sync_suspension(&mut self, id: WindowId) -> Result<()> {
        let suspended = platform::is_minimized(id)
            || platform::window_on_current_virtual_desktop(id).is_ok_and(|current| !current);
        if self.world.windows.get(&id).is_some_and(|window| window.suspended != suspended) {
            self.applied.remove(&id);
            self.commit(Event::WindowSuspended { id, suspended }, "visibility")?;
        }
        Ok(())
    }
    fn sync_native_fullscreen(&mut self) -> Result<()> {
        let native = platform::native_fullscreen()?;
        if let Some((_, monitor)) = native
            && self.world.monitor(monitor).is_none()
        {
            // Display and foreground notifications can arrive in either order.
            self.commit(Event::MonitorsChanged(platform::monitors()?), "display")?;
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
        match event {
            SystemEvent::SessionLocked | SystemEvent::Suspended => self.session_suspended = true,
            SystemEvent::SessionEnding => self.quit = true,
            SystemEvent::SessionUnlocked
            | SystemEvent::Resumed
            | SystemEvent::DesktopChanged
            | SystemEvent::DisplayChanged
            | SystemEvent::SettingsChanged
            | SystemEvent::ShellRestarted => {
                if matches!(
                    event,
                    SystemEvent::SessionUnlocked
                        | SystemEvent::Resumed
                        | SystemEvent::DesktopChanged
                ) {
                    self.session_suspended = !platform::session_interactive().unwrap_or(false);
                }
                self.applied.clear();
                if matches!(event, SystemEvent::ShellRestarted) {
                    platform::invalidate_virtual_desktop_manager();
                    if self.surfaces.is_some() {
                        self.surfaces.take();
                        self.surfaces = Some(Surfaces::new()?);
                    }
                }
                self.maintain()?;
                let mut plan = self.world.plan();
                plan.focus = None;
                self.apply(plan)?;
                self.refresh_surfaces()?;
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
            match parsed.action {
                moon_core::protocol::RequestAction::Command { commands } => {
                    self.commands(commands)?;
                    Ok(json!({"revision":self.world.revision}))
                }
                moon_core::protocol::RequestAction::Query { query } => self.query(&query),
                moon_core::protocol::RequestAction::Subscribe {} => {
                    self.subscribers.push((id.clone(), request.reply.clone()));
                    Ok(serde_json::to_value(&self.world)?)
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
            "transactions" => serde_json::to_value(&self.transactions)?,
            "stats" => {
                json!({"process_id":process::id(),"executable":std::env::current_exe()?,"token":platform::token_capabilities(process::id())?,"uptime_seconds":self.started.elapsed().as_secs_f64(),"ready_us":self.ready_us,"revision":self.world.revision,"managed_windows":self.world.windows.len(),"resources":runtime::resource_stats()?,"surface_pixel_bytes":self.surfaces.as_ref().map_or(0, Surfaces::surface_pixel_bytes),"hooks":hooks::stats(),"key_to_placed_us":self.key_latencies,"key_to_dispatch_us":self.dispatch_latencies,"recent_transactions":self.transactions,"last_error":self.error})
            }
            _ => bail!("unknown query {query:?}"),
        })
    }
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
    let request = match first {
        "query" => {
            json!({"version":1,"id":1,"method":"query","query":words.get(1).map(String::as_str).unwrap_or("world")})
        }
        "subscribe" => json!({"version":1,"id":1,"method":"subscribe"}),
        _ => json!({"version":1,"id":1,"method":"command","commands":[words.join(" ")]}),
    };
    if first == "subscribe" {
        ipc::subscribe(&request, |reply| {
            println!("{reply}");
            true
        })?;
    } else {
        let reply =
            ipc::request(&request).context("connect to moon; start it first with `moon start`")?;
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
