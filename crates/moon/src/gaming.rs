//! Game classification runs on the daemon thread, outside input callbacks.
use anyhow::{Context, Result};
use moon_core::{
    config::{Config, GameRule},
    gaming::{Game, GameSource, Games},
    ids::{MonitorId, WindowId},
    world::{WindowInfo, World},
};
use moon_win::{platform, runtime};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Identity {
    pid: u32,
    birth: u64,
}
impl Identity {
    fn current(id: WindowId, pid: u32) -> Option<Self> {
        let birth = runtime::process_birth(pid).ok()?;
        (platform::process_id(id) == pid).then_some(Self { pid, birth })
    }
    fn live(self, id: WindowId) -> bool {
        platform::process_id(id) == self.pid
            && runtime::process_matches(self.pid, self.birth).unwrap_or(false)
    }
}
pub(crate) type Overrides = BTreeMap<WindowId, (Identity, bool)>;

#[derive(Default)]
pub struct Tracker {
    pub power: bool,
    last_app: Option<(WindowId, Identity)>,
    last_game: Option<WindowId>,
    pub(crate) manual: Overrides,
    leaving_fullscreen: BTreeMap<WindowId, Instant>,
}
impl Tracker {
    pub fn resume(&mut self, games: &Games, overrides: Overrides) {
        self.manual = overrides;
        for (&id, game) in &games.sessions {
            let who = Identity { pid: game.process_id, birth: game.process_birth };
            if game.source == GameSource::Manual && who.live(id) {
                self.manual.insert(id, (who, true));
            }
        }
        self.last_game = games.foreground;
    }
    pub fn forget(&mut self, id: WindowId) {
        self.manual.remove(&id);
        self.leaving_fullscreen.remove(&id);
        if self.last_app.is_some_and(|(app, _)| app == id) {
            self.last_app = None;
        }
        if self.last_game == Some(id) {
            self.last_game = None;
        }
    }
    pub fn start(&mut self) -> Result<()> {
        let (id, identity) = self
            .last_app
            .filter(|(id, who)| who.live(*id))
            .context("Focus the game window before starting game mode from the tray")?;
        self.manual.insert(id, (identity, true));
        Ok(())
    }
    pub fn end(&mut self, games: &Games) -> Result<WindowId> {
        let id = games
            .foreground
            .or(self.last_game.filter(|id| games.sessions.contains_key(id)))
            .or_else(|| games.sessions.keys().next().copied())
            .context("No game session is active")?;
        let game = &games.sessions[&id];
        self.manual
            .insert(id, (Identity { pid: game.process_id, birth: game.process_birth }, false));
        Ok(id)
    }
    fn rule(&self, config: &Config, info: &WindowInfo) -> Option<GameRule> {
        if let Some(&(who, on)) = self.manual.get(&info.id)
            && who.pid == info.process_id
            && who.live(info.id)
        {
            return Some(if on { GameRule::Always } else { GameRule::Never });
        }
        config.game_rule(&info.process, &info.class, &info.title)
    }
    pub fn excluded(&self, world: &World, info: &WindowInfo) -> bool {
        world.games.sessions.contains_key(&info.id)
            || self.rule(&world.config, info) == Some(GameRule::Always)
    }

    pub fn observe(
        &mut self,
        world: &World,
        foreground: Option<WindowId>,
        native: Option<(WindowId, MonitorId)>,
        only: Option<&str>,
    ) -> Games {
        self.manual.retain(|id, (who, _)| who.live(*id));
        // Ordinary foreground location and title events do not need process
        // metadata again when no recognition input can start a session.
        if world.games.sessions.is_empty()
            && self.manual.is_empty()
            && !(self.power && world.config.gaming.automatic && native.is_some())
            && !world.config.rules.iter().any(|r| r.game_mode.is_some())
            && self.last_app.is_some_and(|(id, who)| {
                foreground == Some(id) && platform::process_id(id) == who.pid
            })
        {
            return Games::default();
        }
        let mut games = world.games.clone();
        let front = foreground.and_then(|id| platform::window_info(id).ok());
        let front_monitor = front
            .as_ref()
            .map(|w| w.monitor)
            .or_else(|| foreground.and_then(platform::window_monitor));
        // A switcher, notification surface or owned popup can briefly take focus.
        let overlay = front.as_ref().is_some_and(|w| {
            ["GameBar", "GameBarFTServer", "GameOverlayUI", "GameBarWidgets"]
                .iter()
                .any(|s| w.process.eq_ignore_ascii_case(s))
                || matches!(
                    w.class.as_str(),
                    "MultitaskingViewFrame" | "XamlExplorerHostIslandWindow"
                )
        });
        let front_root = foreground.map(platform::root_owner);
        games.foreground = None;
        games.sessions.retain(|id, game| {
            let who = Identity { pid: game.process_id, birth: game.process_birth };
            if !who.live(*id) {
                return false;
            }
            let observed;
            let info = if let Some(info) = front.as_ref().filter(|w| w.id == *id) {
                info
            } else if let Ok(info) = platform::window_info(*id) {
                observed = info;
                &observed
            } else {
                return false;
            };
            let rule = self.rule(&world.config, info);
            let mine = foreground == Some(*id);
            let fullscreen = native.is_some_and(|(h, _)| h == *id);
            let minimized = platform::is_minimized(*id);
            // Exclusive games can leave fullscreen before Windows reports the
            // next foreground or finishes changing the monitor resolution.
            let stays_automatic = if !mine || fullscreen || minimized {
                self.leaving_fullscreen.remove(id);
                true
            } else if game.source == GameSource::Windows {
                self.leaving_fullscreen.entry(*id).or_insert_with(Instant::now).elapsed()
                    < Duration::from_millis(500)
            } else {
                false
            };
            let manual = self.manual.get(id).is_some_and(|(_, on)| *on);
            game.source = match (manual, rule, game.source) {
                (true, _, _) => GameSource::Manual,
                (_, Some(GameRule::Always), _) => GameSource::Rule,
                (_, None, GameSource::Windows)
                    if world.config.gaming.automatic && stays_automatic =>
                {
                    GameSource::Windows
                }
                _ => return false,
            };
            game.monitor = info.monitor;
            let input = mine
                || front_root == Some(*id)
                || (overlay && front_monitor == Some(game.monitor) && self.last_game == Some(*id));
            let visible = !minimized
                && !platform::is_hidden_by_app(*id)
                && platform::cloaked(*id).unwrap_or(0) == 0
                && (input || platform::window_on_current_virtual_desktop(*id).unwrap_or(false));
            game.presenting = visible
                && (input
                    || (foreground.is_none() && game.presenting)
                    || (front_monitor != Some(game.monitor)
                        && platform::window_at_point(info.rect.center())
                            .is_some_and(|hit| hit == *id || platform::root_owner(hit) == *id)));
            if input {
                games.foreground = Some(*id);
            }
            true
        });
        self.leaving_fullscreen.retain(|id, _| games.sessions.contains_key(id));
        // The tray takes foreground itself. A manual start targets the external
        // app captured before that happened, even if another app now has focus.
        let manual_targets: Vec<_> = self
            .manual
            .iter()
            .filter_map(|(&id, &(_, on))| {
                (on && !games.sessions.contains_key(&id) && foreground != Some(id)).then_some(id)
            })
            .collect();
        for id in manual_targets {
            if let Ok(info) = platform::window_info(id) {
                let who = self.manual[&id].0;
                games.sessions.insert(
                    id,
                    Game {
                        process: info.process,
                        process_id: who.pid,
                        process_birth: who.birth,
                        monitor: info.monitor,
                        presenting: false,
                        source: GameSource::Manual,
                    },
                );
            }
        }
        if let Some(info) = front
            && !overlay
            && games.foreground.is_none()
            && (platform::is_manageable(info.id) || native.is_some_and(|(id, _)| id == info.id))
            && only.is_none_or(|name| name.eq_ignore_ascii_case(&info.process))
            && !info.process.eq_ignore_ascii_case("moon-settings")
            && let Some(who) = Identity::current(info.id, info.process_id)
        {
            self.last_app = Some((info.id, who));
            let source = GameSource::detect(
                self.rule(&world.config, &info),
                world.config.gaming.automatic,
                self.power && games.sessions.is_empty(),
                native.is_some_and(|(id, _)| id == info.id),
                world.windows.get(&info.id).is_some_and(|w| w.fullscreen),
            );
            if let Some(mut source) = source {
                if self.manual.get(&info.id).is_some_and(|(_, on)| *on) {
                    source = GameSource::Manual;
                }
                games.foreground = Some(info.id);
                games.sessions.insert(
                    info.id,
                    Game {
                        process: info.process,
                        process_id: who.pid,
                        process_birth: who.birth,
                        monitor: info.monitor,
                        presenting: true,
                        source,
                    },
                );
            }
        }
        if let Some(id) = games.foreground {
            self.last_game = Some(id);
        }
        games
    }
}
