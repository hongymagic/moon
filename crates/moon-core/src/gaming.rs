//! A session survives an Alt+Tab round trip. Keyboard and display ownership differ.
use crate::{
    config::GameRule,
    ids::{MonitorId, WindowId},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GameSource {
    Rule,
    Windows,
    Manual,
}

impl GameSource {
    pub fn detect(
        rule: Option<GameRule>,
        automatic: bool,
        game_power: bool,
        native_fullscreen: bool,
        moon_fullscreen: bool,
    ) -> Option<Self> {
        match rule {
            Some(GameRule::Always) => Some(Self::Rule),
            Some(GameRule::Never) => None,
            None if automatic && game_power && native_fullscreen && !moon_fullscreen => {
                Some(Self::Windows)
            }
            None => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub process: String,
    pub process_id: u32,
    pub process_birth: u64,
    pub monitor: MonitorId,
    pub presenting: bool,
    pub source: GameSource,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Games {
    pub sessions: BTreeMap<WindowId, Game>,
    /// Session receiving input, including through its owned dialogs or overlay.
    pub foreground: Option<WindowId>,
}
impl Games {
    pub fn covers(&self, monitor: MonitorId) -> bool {
        self.sessions.values().any(|game| game.presenting && game.monitor == monitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        geometry::Rect,
        scene,
        world::{self, Event, Monitor, WindowInfo, World},
    };

    #[test]
    fn rules_override_windows_and_application_fullscreen_is_required() {
        let config = Config::load("[[rules]]\ngame-mode='always'\nmatches=[{process={equals='runner'}}]\n[[rules]]\ngame-mode='never'\nmatches=[{process={equals='runner'},class={equals='Launcher'}}]").unwrap();
        assert_eq!(config.game_rule("RUNNER", "Play", "Game"), Some(GameRule::Always));
        let launcher = config.game_rule("Runner", "Launcher", "");
        assert_eq!(GameSource::detect(launcher, true, true, true, false), None);
        assert_eq!(
            GameSource::detect(Some(GameRule::Always), false, false, false, false),
            Some(GameSource::Rule)
        );
        assert_eq!(GameSource::detect(None, true, true, true, false), Some(GameSource::Windows));
        for (auto, power, full, manual_full) in [
            (false, true, true, false),
            (true, false, true, false),
            (true, true, false, false),
            (true, true, true, true),
        ] {
            assert_eq!(GameSource::detect(None, auto, power, full, manual_full), None);
        }
    }

    #[test]
    fn game_monitor_yields_its_bar_and_placements_without_losing_the_tree() {
        let monitors = [1, 2].map(|n| Monitor {
            id: MonitorId(n),
            name: n.to_string(),
            bounds: Rect::new((n as i32 - 1) * 1280, 0, 1280, 720),
            work_area: Rect::new((n as i32 - 1) * 1280, 0, 1280, 680),
            scale: 1.0,
        });
        let mut w = World::new(Config::default(), monitors.to_vec()).unwrap();
        for m in monitors {
            w = world::reduce(
                &w,
                Event::WindowAdded(WindowInfo {
                    id: WindowId(m.id.0 as u64),
                    process_id: m.id.0,
                    process: "fixture".into(),
                    class: "fixture".into(),
                    title: "fixture".into(),
                    rect: m.work_area,
                    monitor: m.id,
                }),
            )
            .unwrap()
            .world;
        }
        let baseline = w.plan();
        let workspaces = w.workspaces.clone();
        let game = WindowId(1);
        let mut games = Games { foreground: Some(game), ..Default::default() };
        games.sessions.insert(
            game,
            Game {
                process: "game".into(),
                process_id: 1,
                process_birth: 10,
                monitor: MonitorId(1),
                presenting: true,
                source: GameSource::Rule,
            },
        );
        w = world::reduce(&w, Event::GamesChanged(games.clone())).unwrap().world;
        let sc = scene::derive(&w, &BTreeMap::new(), None);
        assert!(!sc.bars[0].visible && sc.bars[1].visible);
        assert!(sc.borders.iter().all(|b| b.monitor == MonitorId(2)));
        assert_eq!(w.plan().placements.iter().map(|p| p.id).collect::<Vec<_>>(), [WindowId(2)]);
        assert_eq!(w.plan().focus, None);
        games.foreground = None;
        w = world::reduce(&w, Event::GamesChanged(games)).unwrap().world;
        assert!(w.games.covers(MonitorId(1)));
        assert_eq!(w.workspaces, workspaces);
        w = world::reduce(&w, Event::GamesChanged(Games::default())).unwrap().world;
        assert_eq!(w.plan(), baseline);
        assert!(scene::derive(&w, &BTreeMap::new(), None).bars.iter().all(|b| b.visible));
    }
}
