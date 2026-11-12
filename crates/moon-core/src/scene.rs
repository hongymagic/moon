//! Surfaces are immutable projections of the world and provider values.
use crate::{geometry::Rect, ids::MonitorId, world::World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use taffy::prelude::*;
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub bars: Vec<BarScene>,
    pub borders: Vec<BorderScene>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BarScene {
    pub monitor: MonitorId,
    pub rect: Rect,
    pub visible: bool,
    pub background: String,
    pub items: Vec<BarItem>,
    pub font: String,
    pub opacity: f32,
    pub backdrop: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BarItem {
    pub rect: Rect,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub selected: bool,
    pub text: String,
    pub foreground: String,
    pub background: Option<String>,
    pub command: Option<String>,
    pub font_size: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BorderScene {
    pub monitor: MonitorId,
    pub rect: Rect,
    pub focused: bool,
    pub width: f32,
    pub radius: f32,
    pub colors: [String; 2],
}
/// Substitution has no evaluation or execution. Unknown provider keys disappear.
pub fn format_template(template: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('}') {
            if let Some(value) = values.get(&rest[..end]) {
                out.push_str(value);
            }
            rest = &rest[end + 1..];
        } else {
            out.push('{');
            break;
        }
    }
    out.push_str(rest);
    out
}
pub fn derive(world: &World, providers: &BTreeMap<String, String>, error: Option<&str>) -> Scene {
    let config = &world.config;
    let plan = world.plan();
    let mut scene = Scene::default();
    let color = |value: &str| {
        format!(
            "#{:06x}",
            config
                .color_for(value, providers.get("system.theme").map(String::as_str))
                .unwrap_or(0xf0ebe0)
        )
    };
    for monitor in &world.monitors {
        let fullscreen = world.native_fullscreen.is_some_and(|(_, id)| id == monitor.id)
            || plan.placements.iter().any(|p| {
                p.visible
                    && p.fullscreen
                    && world.windows.get(&p.id).is_some_and(|w| {
                        world.workspaces[w.workspace.index()].monitor == monitor.id
                    })
            });
        let scale = monitor.scale as f32;
        let height = (config.bar_height_on(&monitor.name) as f32 * scale).round() as i32;
        let rect = Rect::new(monitor.bounds.x, monitor.bounds.y, monitor.bounds.width, height);
        let mut values = providers.clone();
        values.insert(
            "title".into(),
            world
                .focused
                .and_then(|id| world.windows.get(&id))
                .map(|w| w.info.title.clone())
                .unwrap_or_default(),
        );
        values.insert("paused".into(), if world.paused { "PAUSED ".into() } else { String::new() });
        values.insert(
            "mode".into(),
            if world.mode == "default" { String::new() } else { format!("{} ", world.mode) },
        );
        let workspace = world.active.get(&monitor.id).copied();
        values.insert(
            "direction".into(),
            workspace
                .map(|id| {
                    world.workspaces[id.index()].tree.direction(world.focused).name().to_string()
                })
                .unwrap_or_default(),
        );
        let mut items = Vec::new();
        let mut alignments = Vec::new();
        for widget in &config.bar.widgets {
            if widget.id == "workspaces" {
                for ws in &world.workspaces {
                    let selected = workspace == Some(ws.id);
                    let style = &config.workspaces[ws.id.index()];
                    let accent = style.color.as_deref().unwrap_or("terracotta");
                    items.push(BarItem {
                        rect: Rect::default(),
                        label: format!(
                            "Workspace {}{}",
                            ws.name,
                            if selected { " · Current" } else { "" }
                        ),
                        selected,
                        text: style.icon.clone().unwrap_or_else(|| ws.name.clone()),
                        foreground: color(if selected { "crust" } else { accent }),
                        background: Some(color(if selected {
                            accent
                        } else {
                            widget.background.as_deref().unwrap_or("surface0")
                        })),
                        command: Some(format!("workspace {}", ws.name)),
                        font_size: config.bar.font_size * scale,
                    });
                    alignments.push((widget.align.as_str(), 34.0 * scale, 0.0, 6.0 * scale));
                }
                continue;
            }
            let text = if widget.exec.is_some() {
                providers.get(&widget.id).cloned().unwrap_or_default()
            } else {
                format_template(&widget.format, &values)
            };
            // Optional providers do not reserve blank slots or display dangling units.
            let placeholders = widget
                .format
                .split('{')
                .skip(1)
                .filter_map(|s| s.split_once('}').map(|(key, _)| key))
                .collect::<Vec<_>>();
            if text.trim().is_empty()
                || (!placeholders.is_empty()
                    && placeholders.iter().all(|key| values.get(*key).is_none_or(String::is_empty)))
            {
                continue;
            }
            let width = ((text.chars().count() as f32 * config.bar.font_size * 0.62 + 20.0)
                * scale)
                .min(widget.max_width as f32 * scale);
            let label = if widget.id == "logo" {
                "Moon".to_owned()
            } else {
                let name = match widget.id.as_str() {
                    "cpu" => "Processor usage",
                    "memory" => "Memory usage",
                    "keyboard" => "Keyboard language",
                    "network" => "Network",
                    "battery" => "Battery",
                    "volume" => "Volume",
                    "weather" => "Weather",
                    "clock" => "Date and time",
                    "title" => "Active window",
                    "media" => "Now playing",
                    "state" => "Layout",
                    _ => &widget.id,
                };
                format!("{name}: {text}")
            };
            if let Some(icon) = &widget.icon {
                items.push(BarItem {
                    rect: Rect::default(),
                    label: label.clone(),
                    selected: false,
                    text: icon.clone(),
                    foreground: color("base"),
                    background: Some(color(widget.icon_color.as_deref().unwrap_or("terracotta"))),
                    command: widget.click.clone(),
                    font_size: config.bar.font_size * scale,
                });
                alignments.push((widget.align.as_str(), 30.0 * scale, 0.0, 0.0));
            }
            items.push(BarItem {
                rect: Rect::default(),
                label,
                selected: false,
                text,
                foreground: color(widget.color.as_deref().unwrap_or(&config.bar.foreground)),
                background: widget.background.as_deref().map(color),
                command: widget.click.clone(),
                font_size: config.bar.font_size * scale,
            });
            alignments.push((
                widget.align.as_str(),
                width,
                if widget.id == "title" || widget.id == "media" { 1.0 } else { 0.0 },
                if widget.align == "center" { 0.0 } else { 6.0 * scale },
            ));
        }
        if let Some(error) = error {
            items.clear();
            alignments.clear();
            items.push(BarItem {
                rect,
                label: format!("Configuration error: {error}. Click to reload."),
                selected: false,
                text: format!(
                    "moon: {}",
                    error.lines().map(str::trim).collect::<Vec<_>>().join(" ")
                ),
                foreground: color("ember"),
                background: None,
                command: Some("reload".into()),
                font_size: config.bar.font_size * scale,
            });
            alignments.push(("left", rect.width as f32, 1.0, 0.0));
        }
        place_items(&mut items, &alignments, rect, scale);
        scene.bars.push(BarScene {
            monitor: monitor.id,
            rect,
            visible: config.bar_enabled_on(&monitor.name) && !fullscreen,
            background: color(&config.bar.background),
            items,
            font: config.bar.font.clone(),
            opacity: config.bar.opacity,
            backdrop: config.bar.backdrop.clone(),
        });
        if config.borders.enabled && !fullscreen && !world.paused {
            for placement in
                plan.placements.iter().filter(|p| p.visible && !p.floating && !p.fullscreen)
            {
                if world
                    .windows
                    .get(&placement.id)
                    .is_none_or(|w| world.workspaces[w.workspace.index()].monitor != monitor.id)
                {
                    continue;
                }
                let focused = world.focused == Some(placement.id);
                let width = config.borders.width as f32 * scale;
                scene.borders.push(BorderScene {
                    monitor: monitor.id,
                    rect: placement.rect.outset(width.ceil() as i32),
                    focused,
                    width,
                    radius: config.borders.radius as f32 * scale,
                    colors: if focused {
                        [color(&config.borders.focused), color(&config.borders.focused_end)]
                    } else {
                        [color(&config.borders.unfocused), color(&config.borders.unfocused)]
                    },
                });
            }
        }
    }
    scene
}
fn place_items(
    items: &mut [BarItem],
    alignments: &[(&str, f32, f32, f32)],
    rect: Rect,
    scale: f32,
) {
    let mut taffy: TaffyTree<()> = TaffyTree::new();
    let mut children = Vec::new();
    let mut nodes = Vec::new();
    let padding = (rect.width as f32 * 0.015).round();
    let available = (rect.width as f32 - 2.0 * padding).max(1.0);
    // Three flex groups share the bar; title/media shrink first when space is tight.
    for alignment in ["left", "center", "right"] {
        let mut group = Vec::new();
        for (index, &(align, width, shrink, gap)) in alignments.iter().enumerate() {
            if align != alignment {
                continue;
            }
            let node = taffy
                .new_leaf(Style {
                    size: taffy::geometry::Size {
                        width: Dimension::from_length(width),
                        height: Dimension::from_length(rect.height as f32),
                    },
                    min_size: taffy::geometry::Size {
                        width: LengthPercentageAuto::from_length(if shrink > 0.0 {
                            0.0
                        } else {
                            width.min(24.0 * scale)
                        }),
                        height: LengthPercentageAuto::AUTO,
                    },
                    margin: taffy::geometry::Rect {
                        right: LengthPercentageAuto::from_length(gap),
                        left: LengthPercentageAuto::ZERO,
                        top: LengthPercentageAuto::ZERO,
                        bottom: LengthPercentageAuto::ZERO,
                    },
                    flex_shrink: shrink.max(0.2),
                    ..Default::default()
                })
                .expect("finite leaf style");
            group.push(node);
            nodes.push((index, node));
        }
        children.push(
            taffy
                .new_with_children(
                    Style {
                        display: Display::Flex,
                        size: taffy::geometry::Size {
                            width: Dimension::from_length(available / 3.0),
                            height: Dimension::AUTO,
                        },
                        flex_grow: 0.0,
                        flex_shrink: 1.0,
                        justify_content: if alignment == "right" {
                            JustifyContent::END
                        } else if alignment == "center" {
                            JustifyContent::CENTER
                        } else {
                            JustifyContent::START
                        },
                        ..Default::default()
                    },
                    &group,
                )
                .expect("new child nodes"),
        );
    }
    let root = taffy
        .new_with_children(
            Style {
                display: Display::Flex,
                size: taffy::geometry::Size {
                    width: Dimension::from_length(available),
                    height: Dimension::from_length(rect.height as f32),
                },
                ..Default::default()
            },
            &children,
        )
        .expect("new group nodes");
    taffy
        .compute_layout(
            root,
            taffy::geometry::Size {
                width: AvailableSpace::Definite(available),
                height: AvailableSpace::Definite(rect.height as f32),
            },
        )
        .expect("valid finite surface dimensions");
    for (index, node) in nodes {
        let layout = taffy.layout(node).unwrap();
        let parent = taffy.parent(node).unwrap();
        let group = taffy.layout(parent).unwrap();
        let left = (padding + group.location.x + layout.location.x).round() as i32;
        let width = (layout.size.width.round() as i32).min((rect.width - left).max(0));
        let height = (28.0 * scale).round().min(rect.height as f32) as i32;
        items[index].rect =
            Rect::new(rect.x + left, rect.y + (rect.height - height) / 2, width, height);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        ids::WindowId,
        world::{Event, Monitor, WindowInfo, reduce},
    };
    #[test]
    fn scenes_follow_focus_workspace_and_fullscreen() {
        let mut world = World::new(
            Config::default(),
            vec![Monitor {
                id: MonitorId(1),
                name: "screen".into(),
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1080),
                scale: 1.25,
            }],
        )
        .unwrap();
        world = reduce(
            &world,
            Event::WindowAdded(WindowInfo {
                id: WindowId(1),
                process_id: 1,
                process: "testwin".into(),
                class: "Test".into(),
                title: "Hello".into(),
                rect: Rect::new(100, 100, 500, 400),
                monitor: MonitorId(1),
            }),
        )
        .unwrap()
        .world;
        let scene = derive(&world, &BTreeMap::new(), None);
        assert_eq!(scene.bars[0].rect.height, 50);
        assert_eq!(scene.borders[0].width, 5.0);
        assert!(scene.borders[0].focused);
        assert!(scene.bars[0].items.iter().any(|i| i.text == "Hello"));
        world = reduce(&world, Event::Command(crate::command::Command::ToggleFullscreen))
            .unwrap()
            .world;
        let scene = derive(&world, &BTreeMap::new(), None);
        assert!(!scene.bars[0].visible);
        assert!(scene.borders.is_empty());
    }
    #[test]
    fn configured_styles_templates_and_clicks_reach_the_scene() {
        let config=Config::load(r##"
            [palette]
            custom="#123456"
            [bar]
            font="Custom font"
            font-size=18.0
            opacity=0.8
            backdrop="mica"
            widgets=[{id="custom",format="Used {memory}",icon="X",icon-color="custom",background="surface1",color="custom",align="right",click="pause",max-width=200}]
            [[monitors]]
            name="screen"
            bar-height=60
        "##).unwrap();
        let world = World::new(
            config,
            vec![Monitor {
                id: MonitorId(1),
                name: "screen".into(),
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1080),
                scale: 1.0,
            }],
        )
        .unwrap();
        let scene = derive(&world, &BTreeMap::from([("memory".into(), "42%".into())]), None);
        let bar = &scene.bars[0];
        assert_eq!(bar.rect.height, 60);
        assert_eq!(bar.font, "Custom font");
        assert_eq!(bar.opacity, 0.8);
        assert_eq!(bar.backdrop, "mica");
        assert_eq!(bar.items[0].background.as_deref(), Some("#123456"));
        assert_eq!(bar.items[1].text, "Used 42%");
        assert_eq!(bar.items[1].foreground, "#123456");
        assert_eq!(bar.items[1].command.as_deref(), Some("pause"));
        assert!(bar.items[1].rect.x > 960);
        assert!(bar.items[1].rect.width <= 200);
        let error = derive(&world, &BTreeMap::new(), Some("line 4: unknown field"));
        assert!(error.bars[0].items[0].text.contains("line 4"));
    }
    #[test]
    fn native_fullscreen_suppresses_only_its_monitor_without_changing_manual_state() {
        let mut world = World::new(
            Config::default(),
            vec![
                Monitor {
                    id: MonitorId(1),
                    name: "first".into(),
                    bounds: Rect::new(0, 0, 1920, 1080),
                    work_area: Rect::new(0, 0, 1920, 1080),
                    scale: 1.0,
                },
                Monitor {
                    id: MonitorId(2),
                    name: "second".into(),
                    bounds: Rect::new(1920, 0, 1920, 1080),
                    work_area: Rect::new(1920, 0, 1920, 1080),
                    scale: 1.0,
                },
            ],
        )
        .unwrap();
        world = reduce(
            &world,
            Event::WindowAdded(WindowInfo {
                id: WindowId(1),
                process_id: 1,
                process: "testwin".into(),
                class: "Test".into(),
                title: "Game".into(),
                rect: Rect::new(100, 100, 600, 400),
                monitor: MonitorId(1),
            }),
        )
        .unwrap()
        .world;
        let tiled = world.plan().placements[0].rect;
        world = reduce(&world, Event::NativeFullscreen(Some((WindowId(1), MonitorId(1)))))
            .unwrap()
            .world;
        assert!(!world.windows[&WindowId(1)].fullscreen);
        assert_eq!(world.plan().placements[0].rect, world.monitors[0].bounds);
        let scene = derive(&world, &BTreeMap::new(), None);
        assert!(!scene.bars[0].visible);
        assert!(scene.bars[1].visible);
        assert!(scene.borders.is_empty());
        world = reduce(&world, Event::NativeFullscreen(None)).unwrap().world;
        assert_eq!(world.plan().placements[0].rect, tiled);
        world = reduce(&world, Event::NativeFullscreen(Some((WindowId(999), MonitorId(2)))))
            .unwrap()
            .world;
        let scene = derive(&world, &BTreeMap::new(), None);
        assert!(scene.bars[0].visible);
        assert!(!scene.bars[1].visible);
        world = reduce(&world, Event::WindowRemoved(WindowId(999))).unwrap().world;
        assert!(world.native_fullscreen.is_none());
    }
    #[test]
    fn system_theme_changes_palette_without_overriding_explicit_colours() {
        let config = Config::load(
            r##"theme="system"
[light-palette]
base="#ffffff"
text="#111111"
[bar]
widgets=[{id="custom",format="Hello",color="#abcdef"}]
"##,
        )
        .unwrap();
        let world = World::new(
            config,
            vec![Monitor {
                id: MonitorId(1),
                name: "screen".into(),
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1080),
                scale: 1.0,
            }],
        )
        .unwrap();
        let dark = derive(&world, &BTreeMap::from([("system.theme".into(), "dark".into())]), None);
        let light =
            derive(&world, &BTreeMap::from([("system.theme".into(), "light".into())]), None);
        assert_eq!(dark.bars[0].background, "#141417");
        assert_eq!(light.bars[0].background, "#ffffff");
        assert_eq!(light.bars[0].items[0].foreground, "#abcdef");
        assert_eq!(world.config.color_for("text", Some("light")).unwrap(), 0x111111);
        let mut explicit = world.clone();
        explicit.config.theme = "light".into();
        assert_eq!(derive(&explicit, &BTreeMap::new(), None).bars[0].background, "#ffffff");
    }
    #[test]
    fn default_provider_pills_have_stable_geometry() {
        let world = World::new(
            Config::default(),
            vec![Monitor {
                id: MonitorId(1),
                name: "screen".into(),
                bounds: Rect::new(0, 0, 3840, 2160),
                work_area: Rect::new(0, 0, 3840, 2160),
                scale: 1.25,
            }],
        )
        .unwrap();
        let providers = BTreeMap::from([
            ("clock".into(), "Wed 11 Nov 22:48".into()),
            ("cpu".into(), "8%".into()),
            ("memory".into(), "42%".into()),
            ("volume".into(), "50%".into()),
            ("network".into(), "Wired".into()),
            ("keyboard".into(), "ENG".into()),
        ]);
        insta::assert_debug_snapshot!(derive(&world, &providers, None));
    }
    #[test]
    fn templates_never_evaluate_input() {
        assert_eq!(
            format_template("CPU {cpu}% {missing}", &BTreeMap::from([("cpu".into(), "7".into())])),
            "CPU 7% "
        );
    }
}
