//! Surfaces are immutable projections of the world and provider values.
use crate::{command::workspace_command, geometry::Rect, ids::MonitorId, world::World};
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
    /// Center the visible glyphs on both axes within the item's rectangle.
    #[serde(default)]
    pub center_content: bool,
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
/// Keys used by optional-widget visibility. Keep its existing split semantics:
/// nested braces may expose a key even when substitution treats it differently.
/// Provider selection must preserve that conservative dependency too.
pub fn template_placeholders(template: &str) -> impl Iterator<Item = &str> {
    template.split('{').skip(1).filter_map(|s| s.split_once('}').map(|(key, _)| key))
}
pub fn derive(world: &World, providers: &BTreeMap<String, String>, error: Option<&str>) -> Scene {
    derive_with_bar_bounds(world, providers, error, |_, rect, _| {
        Ok::<_, std::convert::Infallible>(rect)
    })
    .unwrap_or_else(|never| match never {})
}

/// Derive the scene using native bar bounds where the platform has negotiated them.
pub fn derive_with_bar_bounds<E>(
    world: &World,
    providers: &BTreeMap<String, String>,
    error: Option<&str>,
    mut bar_bounds: impl FnMut(MonitorId, Rect, bool) -> Result<Rect, E>,
) -> Result<Scene, E> {
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
        let visible = config.bar_enabled_on(&monitor.name) && !fullscreen;
        let requested = Rect::new(monitor.bounds.x, monitor.bounds.y, monitor.bounds.width, height);
        let rect = bar_bounds(monitor.id, requested, visible)?;
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
                    let text = style.icon.clone().unwrap_or_else(|| ws.name.clone());
                    // Reserve a full em per displayed character plus the native
                    // renderer's rounded seven-DIP inset on each side. Long names
                    // remain bounded; a smaller configured cap cannot erase the
                    // existing minimum target size.
                    let width = (text.chars().count() as f32 * config.bar.font_size * scale
                        + 2.0 * (7.0 * scale).round())
                    .clamp(34.0 * scale, widget.max_width.max(34) as f32 * scale);
                    items.push(BarItem {
                        rect: Rect::default(),
                        label: format!(
                            "Workspace {}{}",
                            ws.name,
                            if selected { " · Current" } else { "" }
                        ),
                        selected,
                        center_content: true,
                        text,
                        foreground: color(if selected { "crust" } else { accent }),
                        background: Some(color(if selected {
                            accent
                        } else {
                            widget.background.as_deref().unwrap_or("surface0")
                        })),
                        command: Some(workspace_command(&ws.name)),
                        font_size: config.bar.font_size * scale,
                    });
                    alignments.push((widget.align.as_str(), width, 0.0, 6.0 * scale));
                }
                continue;
            }
            let text = if widget.exec.is_some() {
                providers.get(&widget.id).cloned().unwrap_or_default()
            } else {
                format_template(&widget.format, &values)
            };
            // Optional providers do not reserve blank slots or display dangling units.
            let placeholders = template_placeholders(&widget.format).collect::<Vec<_>>();
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
                    center_content: false,
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
                center_content: false,
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
            // Keep navigation available while an error needs attention. Replace
            // central status widgets, rather than taking over the whole bar.
            for index in (0..items.len()).rev() {
                let navigation = items[index].command.as_deref().is_some_and(|command| {
                    command == "settings" || command.starts_with("workspace ")
                });
                if alignments[index].0 == "center" && !navigation {
                    items.remove(index);
                    alignments.remove(index);
                }
            }
            items.push(BarItem {
                rect,
                label: format!("Moon: {error}. Click to reload configuration."),
                selected: false,
                center_content: false,
                text: format!(
                    "moon: {}",
                    error.lines().map(str::trim).collect::<Vec<_>>().join(" ")
                ),
                foreground: color("ember"),
                background: None,
                command: Some("reload".into()),
                font_size: config.bar.font_size * scale,
            });
            alignments.push(("center", rect.width as f32 / 3.0, 1.0, 0.0));
        }
        place_items(&mut items, &alignments, rect, scale);
        scene.bars.push(BarScene {
            monitor: monitor.id,
            rect,
            visible,
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
    Ok(scene)
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
    let alignments_in_order = ["left", "center", "right"];
    let minimum_widths = alignments_in_order.map(|alignment| {
        alignments
            .iter()
            .filter(|(align, ..)| *align == alignment)
            .map(|&(_, width, shrink, gap)| {
                gap + if shrink > 0.0 { 0.0 } else { width.min(24.0 * scale) }
            })
            .sum::<f32>()
    });
    let nominal_width = available / 3.0;
    let minimum_total = minimum_widths.iter().sum::<f32>();
    let compressed = minimum_total > available;
    let group_widths = if compressed {
        minimum_widths.map(|width| available * width / minimum_total)
    } else if minimum_widths.iter().any(|&width| width > nominal_width) {
        let spare = minimum_widths.map(|width| (nominal_width - width).max(0.0));
        let spare_total = spare.iter().sum::<f32>();
        std::array::from_fn(|index| {
            minimum_widths[index] + (available - minimum_total) * spare[index] / spare_total
        })
    } else {
        [nominal_width; 3]
    };
    // Three flex groups share the bar; title/media shrink first when space is tight.
    for (group_index, alignment) in alignments_in_order.into_iter().enumerate() {
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
                        width: LengthPercentageAuto::from_length(if compressed {
                            if items[index]
                                .command
                                .as_ref()
                                .is_some_and(|c| c.starts_with("workspace "))
                            {
                                1.0
                            } else {
                                0.0
                            }
                        } else if shrink > 0.0 {
                            0.0
                        } else {
                            width.min(24.0 * scale)
                        }),
                        height: LengthPercentageAuto::AUTO,
                    },
                    margin: taffy::geometry::Rect {
                        right: LengthPercentageAuto::from_length(if compressed {
                            0.0
                        } else {
                            gap
                        }),
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
                            width: Dimension::from_length(group_widths[group_index]),
                            height: Dimension::AUTO,
                        },
                        flex_grow: 0.0,
                        flex_shrink: 0.0,
                        min_size: taffy::geometry::Size {
                            width: LengthPercentageAuto::ZERO,
                            height: LengthPercentageAuto::AUTO,
                        },
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
        let group_left = (padding + group.location.x).round() as i32;
        let group_right = (padding + group.location.x + group.size.width).round() as i32;
        let left = (padding + group.location.x + layout.location.x).round() as i32;
        let right = left.saturating_add(layout.size.width.round() as i32);
        let left = left.clamp(group_left, group_right).clamp(0, rect.width);
        let right = right.clamp(left, group_right.max(left)).min(rect.width);
        let height = (28.0 * scale).round().min(rect.height as f32) as i32;
        items[index].rect =
            Rect::new(rect.x + left, rect.y + (rect.height - height) / 2, right - left, height);
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
        assert!(error.bars[0].items.iter().any(|item| item.text.contains("line 4")));
    }
    #[test]
    fn errors_leave_workspace_navigation_available_and_keep_complete_details() {
        let world = World::new(
            Config::default(),
            vec![Monitor {
                id: MonitorId(1),
                name: "screen".into(),
                bounds: Rect::new(0, 0, 1920, 1080),
                work_area: Rect::new(0, 0, 1920, 1080),
                scale: 1.0,
            }],
        )
        .unwrap();
        let providers = BTreeMap::from([("clock".into(), "12:34".into())]);
        let error = "Window placement failed\nThe application refused the requested position";
        let scene = derive(&world, &providers, Some(error));
        let items = &scene.bars[0].items;
        for workspace in 1..=10 {
            let command = format!("workspace {workspace}");
            assert!(items.iter().any(|item| item.command.as_deref() == Some(&command)));
        }
        assert!(items.iter().any(|item| item.command.as_deref() == Some("cheatsheet")));
        assert!(!items.iter().any(|item| item.text == "12:34"));
        let notice = items.iter().find(|item| item.command.as_deref() == Some("reload")).unwrap();
        assert!(notice.label.contains(error));
        assert!(!notice.label.contains("Configuration error"));
        assert!(!notice.text.contains('\n'));
        assert!(notice.rect.width > 0);
        assert!(
            items
                .iter()
                .filter(|item| item.command != notice.command)
                .all(|item| !item.rect.intersects(&notice.rect))
        );
    }
    #[test]
    fn workspace_ten_reserves_two_ems_and_padding_at_larger_fonts_and_scales() {
        for (font_size, scale) in [(18.0, 1.0), (18.0, 1.25), (18.0, 1.5), (24.0, 1.25)] {
            let mut config = Config::default();
            config.bar.font = "Arial".into();
            config.bar.font_size = font_size;
            let world = World::new(
                config,
                vec![Monitor {
                    id: MonitorId(1),
                    name: "screen".into(),
                    bounds: Rect::new(0, 0, 3840, 2160),
                    work_area: Rect::new(0, 0, 3840, 2160),
                    scale,
                }],
            )
            .unwrap();
            let scene = derive(&world, &BTreeMap::new(), None);
            let ten = scene.bars[0]
                .items
                .iter()
                .find(|item| item.command.as_deref() == Some("workspace 10"))
                .unwrap();
            assert_eq!(ten.text, "10");
            let content_width = ten.rect.width - 2 * (7.0 * scale).round() as i32;
            assert!(content_width >= (2.0 * f64::from(font_size) * scale).ceil() as i32);
            assert!(ten.rect.width >= (34.0 * scale).round() as i32);
        }
    }

    #[test]
    fn workspace_width_follows_displayed_icons_and_font_with_bounded_names() {
        let item = |config: Config| {
            let world = World::new(
                config,
                vec![Monitor {
                    id: MonitorId(1),
                    name: "screen".into(),
                    bounds: Rect::new(0, 0, 3840, 2160),
                    work_area: Rect::new(0, 0, 3840, 2160),
                    scale: 1.25,
                }],
            )
            .unwrap();
            derive(&world, &BTreeMap::new(), None).bars.remove(0).items.remove(0)
        };
        let mut config = Config::default();
        config.bar.font_size = 18.0;
        config.bar.widgets.retain(|widget| widget.id == "workspaces");
        config.bar.widgets[0].max_width = 160;
        config.workspaces.truncate(1);
        config.workspaces[0].name = "A deliberately very long workspace name".into();
        let long = item(config.clone());
        assert_eq!(long.rect.width, 200); // The configured 160-DIP cap at 125%.
        assert!(long.label.contains(&config.workspaces[0].name));
        config.bar.widgets[0].max_width = 80;
        assert_eq!(item(config.clone()).rect.width, 100);
        config.bar.widgets[0].max_width = 12;
        assert_eq!(item(config.clone()).rect.width, 43); // The 34-DIP minimum.
        config.bar.widgets[0].max_width = 300;
        config.workspaces[0].icon = Some("".into());
        let icon = item(config.clone());
        assert!(icon.center_content);
        assert_eq!(icon.text, "");
        assert_eq!(icon.rect.width, 43);
        config.workspaces[0].icon = Some("DEV".into());
        let wider_icon = item(config.clone());
        assert!(wider_icon.rect.width > icon.rect.width);
        config.bar.font_size = 28.0;
        assert!(item(config).rect.width > wider_icon.rect.width);
    }

    #[test]
    fn crowded_secondary_bars_keep_items_inside_their_group_without_overlap() {
        let providers = BTreeMap::from([
            ("clock".into(), "Thursday 13 November 12:34".into()),
            ("media".into(), "A long song title and artist".into()),
            ("keyboard".into(), "Korean".into()),
            ("network".into(), "Wireless network".into()),
            ("memory".into(), "42%".into()),
            ("cpu".into(), "99%".into()),
            ("battery".into(), "100%".into()),
            ("volume".into(), "Muted".into()),
            ("weather".into(), "Cloudy 28°C".into()),
        ]);
        for (width, scale, workspace_count) in [
            (800, 1.5, 10),
            (1280, 1.0, 10),
            (1280, 1.25, 15),
            (1920, 1.5, 10),
            (1920, 1.0, 128),
            (3840, 1.25, 10),
        ] {
            let mut config = Config::default();
            while config.workspaces.len() < workspace_count {
                let mut workspace = config.workspaces[0].clone();
                workspace.name = (config.workspaces.len() + 1).to_string();
                config.workspaces.push(workspace);
            }
            let world = World::new(
                config,
                vec![Monitor {
                    id: MonitorId(1),
                    name: "secondary".into(),
                    bounds: Rect::new(-width, -200, width, 1080),
                    work_area: Rect::new(-width, -200, width, 1080),
                    scale,
                }],
            )
            .unwrap();
            for error in [None, Some("A long runtime error requiring attention")] {
                let scene = derive(&world, &providers, error);
                let bar = &scene.bars[0];
                let workspace_buttons = bar.items.iter().filter(|item| {
                    !item.rect.is_empty()
                        && item.command.as_ref().is_some_and(|c| c.starts_with("workspace "))
                });
                assert_eq!(workspace_buttons.count(), workspace_count);
                for (index, item) in bar.items.iter().enumerate() {
                    assert!(item.rect.x >= bar.rect.x && item.rect.right() <= bar.rect.right());
                    assert!(item.rect.y >= bar.rect.y && item.rect.bottom() <= bar.rect.bottom());
                    for other in &bar.items[index + 1..] {
                        assert!(
                            !item.rect.intersects(&other.rect),
                            "{width}px scale {scale}: {:?} overlaps {:?}",
                            item.text,
                            other.text
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_multiword_workspace_button_uses_the_same_command_as_the_reducer() {
        let config = Config::load("workspaces=[{name='1'},{name='Web Browsing'}]").unwrap();
        let rect = Rect::new(0, 0, 1920, 1080);
        let world = World::new(
            config,
            vec![Monitor {
                id: MonitorId(1),
                name: "screen".into(),
                bounds: rect,
                work_area: rect,
                scale: 1.0,
            }],
        )
        .unwrap();
        let bar = derive(&world, &BTreeMap::new(), None).bars.remove(0);
        let button = bar.items.iter().find(|item| item.text == "Web Browsing").unwrap();
        let command = button.command.as_ref().unwrap().parse().unwrap();
        let switched = reduce(&world, Event::Command(command)).unwrap().world;
        assert_eq!(switched.workspaces[switched.current_workspace().index()].name, "Web Browsing");
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
    #[test]
    fn placeholder_dependencies_preserve_optional_visibility_and_substitution() {
        let values = BTreeMap::from([("cpu".into(), "7%".into())]);
        assert_eq!(
            template_placeholders("fixed {cpu.percent} {volume.error}").collect::<Vec<_>>(),
            ["cpu.percent", "volume.error"]
        );
        assert_eq!(template_placeholders("fixed {cpu").count(), 0);
        assert_eq!(format_template("fixed {cpu", &values), "fixed {cpu");
        // The existing optional visibility scan sees the inner key, whereas
        // substitution looks up the literal key "{cpu". Preserve both rules.
        assert_eq!(template_placeholders("prefix {{cpu}").collect::<Vec<_>>(), ["cpu"]);
        assert_eq!(format_template("prefix {{cpu}", &values), "prefix ");
        assert_eq!(
            template_placeholders("{} {cpu}} {unknown}").collect::<Vec<_>>(),
            ["", "cpu", "unknown"]
        );
    }
    #[test]
    fn exec_text_still_requires_its_optional_format_dependency() {
        let config = Config::load(
            r#"
            [bar]
            widgets=[{id="custom",format="prefix {{memory.percent}",exec="echo fixture"}]
        "#,
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
        let mut values = BTreeMap::from([("custom".into(), "fixture".into())]);
        assert!(derive(&world, &values, None).bars[0].items.is_empty());
        values.insert("memory.percent".into(), "42".into());
        assert_eq!(derive(&world, &values, None).bars[0].items[0].text, "fixture");
    }
}
