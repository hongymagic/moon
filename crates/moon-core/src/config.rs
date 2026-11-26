//! Strict, versioned configuration. Defaults are a normal TOML document.
use crate::command::Command;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CONFIG_VERSION: u32 = 1;
pub const MAX_CONFIG_BYTES: usize = 1_048_576;
pub const DEFAULT_CONFIG: &str = include_str!("default.toml");
const MAX_ALIAS_VISITS: usize = 4096;
pub type Bindings = BTreeMap<String, Vec<String>>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub config_version: u32,
    /// Animate workspace switches without changing tiled window arrangements.
    #[serde(default = "enabled")]
    pub workspace_animation: bool,
    #[serde(default = "dark")]
    pub theme: String,
    #[serde(default)]
    pub light_palette: BTreeMap<String, String>,
    pub gaps: Gaps,
    pub workspaces: Vec<WorkspaceConfig>,
    #[serde(default)]
    pub monitors: Vec<MonitorConfig>,
    pub bindings: Bindings,
    #[serde(default)]
    pub modes: BTreeMap<String, Bindings>,
    #[serde(default)]
    pub aliases: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub gaming: GamingConfig,
    pub bar: BarConfig,
    pub borders: BorderConfig,
    pub palette: BTreeMap<String, String>,
    #[serde(default)]
    pub hooks: Hooks,
    pub cursor_jump: bool,
    pub focus_follows_cursor: bool,
    #[serde(default)]
    pub weather: Option<WeatherConfig>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Gaps {
    pub inner: i32,
    pub outer: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct WorkspaceConfig {
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub monitor: Option<String>,
    #[serde(default)]
    pub gaps: Option<Gaps>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct MonitorConfig {
    pub name: String,
    #[serde(default)]
    pub gaps: Option<Gaps>,
    #[serde(default)]
    pub bar_enabled: Option<bool>,
    #[serde(default)]
    pub bar_height: Option<i32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct BarConfig {
    pub enabled: bool,
    pub height: i32,
    pub font: String,
    pub font_size: f32,
    pub background: String,
    pub foreground: String,
    pub opacity: f32,
    pub backdrop: String,
    pub widgets: Vec<WidgetConfig>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct WidgetConfig {
    pub id: String,
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_color: Option<String>,
    pub format: String,
    #[serde(default = "left")]
    pub align: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub click: Option<String>,
    #[serde(default)]
    pub exec: Option<String>,
    #[serde(default = "interval")]
    pub interval_seconds: u32,
    #[serde(default = "max_width")]
    pub max_width: i32,
}
fn enabled() -> bool {
    true
}
fn dark() -> String {
    "dark".into()
}
fn left() -> String {
    "left".into()
}
fn interval() -> u32 {
    5
}
fn max_width() -> i32 {
    300
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct BorderConfig {
    pub enabled: bool,
    pub width: i32,
    pub radius: i32,
    pub focused: String,
    pub focused_end: String,
    pub unfocused: String,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Hooks {
    #[serde(default)]
    pub startup: Vec<String>,
    #[serde(default)]
    pub shutdown: Vec<String>,
    #[serde(default)]
    pub reload: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct WeatherConfig {
    /// Friendly city name, or the label for a one-time Windows location lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default = "weather_interval")]
    pub interval_seconds: u32,
}
fn weather_interval() -> u32 {
    900
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Rule {
    /// Overrides automatic detection for windows matched by this rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_mode: Option<GameRule>,
    #[serde(default)]
    pub commands: Vec<String>,
    pub matches: Vec<WindowMatch>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GameRule {
    Always,
    Never,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GamingConfig {
    /// Recognise foreground fullscreen games using Windows Game Mode.
    pub automatic: bool,
    /// Release the dedicated emergency restore hotkey while playing.
    pub pass_every_key: bool,
}
impl Default for GamingConfig {
    fn default() -> Self {
        Self { automatic: true, pass_every_key: false }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct WindowMatch {
    pub process: Option<Matcher>,
    pub class: Option<Matcher>,
    pub title: Option<Matcher>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Matcher {
    pub equals: Option<String>,
    pub regex: Option<String>,
    pub not_regex: Option<String>,
}
impl Matcher {
    fn validate(&self) -> Result<(), String> {
        if usize::from(self.equals.is_some())
            + usize::from(self.regex.is_some())
            + usize::from(self.not_regex.is_some())
            != 1
        {
            return Err("a matcher must contain exactly one of equals, regex or not-regex".into());
        }
        for value in [&self.regex, &self.not_regex].into_iter().flatten() {
            regex::Regex::new(value).map_err(|e| format!("invalid rule regex: {e}"))?;
        }
        Ok(())
    }
    pub fn matches_process(&self, name: &str) -> bool {
        self.equals.as_ref().map_or_else(
            || self.matches(name),
            |value| executable_stem(value).eq_ignore_ascii_case(executable_stem(name)),
        )
    }
    pub fn matches(&self, text: &str) -> bool {
        if let Some(value) = &self.equals {
            return text.eq_ignore_ascii_case(value);
        }
        if let Some(value) = &self.regex {
            return regex::Regex::new(value).is_ok_and(|r| r.is_match(text));
        }
        if let Some(value) = &self.not_regex {
            return regex::Regex::new(value).is_ok_and(|r| !r.is_match(text));
        }
        false
    }
}
/// Windows preserves the executable extension's letter case.
pub fn executable_stem(name: &str) -> &str {
    name.rsplit_once('.')
        .filter(|(_, extension)| extension.eq_ignore_ascii_case("exe"))
        .map_or(name, |(stem, _)| stem)
}
impl WindowMatch {
    pub fn matches(&self, process: &str, class: &str, title: &str) -> bool {
        self.process.as_ref().is_none_or(|m| m.matches_process(process))
            && self.class.as_ref().is_none_or(|m| m.matches(class))
            && self.title.as_ref().is_none_or(|m| m.matches(title))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}
impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line) = self.line {
            write!(f, "line {line}, column {}: ", self.column.unwrap_or(1))?;
        }
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ConfigError {}
impl From<String> for ConfigError {
    fn from(message: String) -> Self {
        Self { message, line: None, column: None }
    }
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG).expect("compiled defaults are valid")
    }
}
impl Config {
    pub fn load(source: &str) -> Result<Self, ConfigError> {
        if source.len() > MAX_CONFIG_BYTES {
            return Err("config exceeds 1 MiB".to_string().into());
        }
        let overrides: toml::Value = toml::from_str(source).map_err(|e: toml::de::Error| {
            error_at(source, e.to_string(), e.span().map(|s| s.start))
        })?;
        let mut base: toml::Value =
            toml::from_str(DEFAULT_CONFIG).expect("compiled defaults parse");
        if let Some(bindings) = overrides.get("bindings").and_then(toml::Value::as_table) {
            let overridden = binding_chords(bindings.keys()).map_err(ConfigError::from)?;
            base.get_mut("bindings")
                .and_then(toml::Value::as_table_mut)
                .expect("compiled bindings are a table")
                .retain(|chord, _| {
                    let key = parse_chord(chord).expect("compiled chord is valid");
                    !overridden.contains(&key)
                });
        }
        merge(&mut base, overrides, "");
        let config: Self = base.try_into().map_err(|e: toml::de::Error| {
            let message = e.to_string();
            let key = message.split('`').nth(1);
            let offset = key.and_then(|key| source.find(key));
            error_at(source, message, offset)
        })?;
        config.validate().map_err(ConfigError::from)?;
        Ok(config)
    }
    pub fn game_rule(&self, process: &str, class: &str, title: &str) -> Option<GameRule> {
        self.rules.iter().rev().find_map(|rule| {
            rule.game_mode.filter(|_| rule.matches.iter().any(|m| m.matches(process, class, title)))
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if !["dark", "light", "system"].contains(&self.theme.as_str()) {
            return Err("theme must be dark, light or system".into());
        }
        for color in self.light_palette.values() {
            self.color(color)?;
        }
        if self.config_version != CONFIG_VERSION {
            return Err(format!(
                "unsupported config-version {}; use moon config migrate",
                self.config_version
            ));
        }
        if self.workspaces.is_empty() || self.workspaces.len() > 128 {
            return Err("configure between 1 and 128 workspaces".into());
        }
        let mut names = BTreeSet::new();
        for workspace in &self.workspaces {
            if workspace.name.trim().is_empty() || !names.insert(&workspace.name) {
                return Err("workspace names must be non-empty and unique".into());
            }
            if let Some(gaps) = &workspace.gaps {
                validate_gaps(gaps, &self.borders)?;
            }
            if let Some(color) = &workspace.color {
                self.color(color)?;
            }
        }
        let mut monitor_names = BTreeSet::new();
        for monitor in &self.monitors {
            if monitor.name.is_empty() || !monitor_names.insert(&monitor.name) {
                return Err("monitor override names must be non-empty and unique".into());
            }
            if let Some(gaps) = &monitor.gaps {
                validate_gaps(gaps, &self.borders)?;
            }
            if monitor.bar_height.is_some_and(|height| !(16..=160).contains(&height)) {
                return Err("monitor bar-height must be 16–160".into());
            }
        }
        validate_gaps(&self.gaps, &self.borders)?;
        if !(0..=32).contains(&self.borders.width) || !(0..=128).contains(&self.borders.radius) {
            return Err("border width must be 0–32 and radius 0–128".into());
        }
        if !(16..=160).contains(&self.bar.height)
            || !(6.0..=72.0).contains(&self.bar.font_size)
            || !(0.0..=1.0).contains(&self.bar.opacity)
        {
            return Err("bar height must be 16–160, font-size 6–72 and opacity 0–1".into());
        }
        if !["none", "mica", "acrylic", "system"].contains(&self.bar.backdrop.as_str()) {
            return Err("bar backdrop must be none, mica, acrylic or system".into());
        }
        for color in [
            &self.bar.background,
            &self.bar.foreground,
            &self.borders.focused,
            &self.borders.focused_end,
            &self.borders.unfocused,
        ]
        .into_iter()
        .chain(self.palette.values())
        {
            self.color(color)?;
        }
        binding_chords(self.bindings.keys())?;
        for commands in self.bindings.values() {
            self.validate_commands(commands, false)?;
        }
        for bindings in self.modes.values() {
            binding_chords(bindings.keys())?;
            for commands in bindings.values() {
                self.validate_commands(commands, false)?;
            }
        }
        for name in self.aliases.keys() {
            self.commands(name)?;
        }
        for commands in [&self.hooks.startup, &self.hooks.shutdown, &self.hooks.reload] {
            self.validate_commands(commands, false)?;
        }
        for command in &self.hooks.reload {
            if self.commands(command)?.contains(&Command::Reload) {
                return Err(
                    "reload hooks cannot reload the configuration, including through aliases"
                        .into(),
                );
            }
        }
        for rule in &self.rules {
            if rule.matches.is_empty() {
                return Err("rule needs at least one match".into());
            }
            self.validate_commands(&rule.commands, true)?;
            for matcher in &rule.matches {
                if matcher.process.is_none() && matcher.class.is_none() && matcher.title.is_none() {
                    return Err("empty rule match would match every window".into());
                }
                for field in
                    [&matcher.process, &matcher.class, &matcher.title].into_iter().flatten()
                {
                    field.validate()?;
                }
            }
        }
        let mut ids = BTreeSet::new();
        for widget in &self.bar.widgets {
            if widget.id.is_empty() || !ids.insert(&widget.id) {
                return Err("widget ids must be non-empty and unique".into());
            }
            if !["left", "center", "right"].contains(&widget.align.as_str()) {
                return Err("widget align must be left, center or right".into());
            }
            if widget.interval_seconds < 1 || widget.max_width < 1 || widget.max_width > 4096 {
                return Err("widget interval must be at least 1 second and max-width 1–4096".into());
            }
            for color in
                [&widget.color, &widget.background, &widget.icon_color].into_iter().flatten()
            {
                self.color(color)?;
            }
            if let Some(command) = &widget.click {
                self.commands(command)?;
            }
        }
        if let Some(weather) = &self.weather
            && (!(-90.0..=90.0).contains(&weather.latitude)
                || !(-180.0..=180.0).contains(&weather.longitude)
                || weather.interval_seconds < 60)
        {
            return Err("weather needs valid latitude/longitude and interval-seconds >= 60".into());
        }
        if self.weather.as_ref().and_then(|weather| weather.location.as_ref()).is_some_and(|name| {
            name.trim().is_empty()
                || name.chars().count() > 200
                || name.chars().any(char::is_control)
        }) {
            return Err(
                "weather location must be a name of 1 to 200 characters without control characters"
                    .into(),
            );
        }
        Ok(())
    }
    fn validate_commands(&self, commands: &[String], window_rule: bool) -> Result<(), String> {
        for command in commands {
            for parsed in self.commands(command)? {
                match parsed {
                    Command::Mode(name) if name != "default" && !self.modes.contains_key(&name) => {
                        return Err(format!("command names unknown mode {name:?}"));
                    }
                    Command::Workspace(name) | Command::MoveToWorkspace { workspace: name, .. }
                        if window_rule && !self.workspaces.iter().any(|w| w.name == name) =>
                    {
                        return Err(format!("window rule names unknown workspace {name:?}"));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    pub fn commands(&self, input: &str) -> Result<Vec<Command>, String> {
        fn expand(
            config: &Config,
            input: &str,
            active: &mut BTreeSet<String>,
            out: &mut Vec<Command>,
            remaining: &mut usize,
        ) -> Result<(), String> {
            if out.len() >= 128 || active.len() >= 16 || *remaining == 0 {
                return Err("command alias expansion exceeds limit".into());
            }
            *remaining -= 1;
            if let Some(commands) = config.aliases.get(input) {
                if !active.insert(input.into()) {
                    return Err(format!("recursive alias {input:?}"));
                }
                for command in commands {
                    expand(config, command, active, out, remaining)?;
                }
                active.remove(input);
            } else {
                out.push(input.parse()?);
            }
            Ok(())
        }
        let mut out = Vec::new();
        let mut remaining = MAX_ALIAS_VISITS;
        expand(self, input, &mut BTreeSet::new(), &mut out, &mut remaining)?;
        Ok(out)
    }
    pub fn gaps_for(&self, workspace: usize, monitor: &str) -> &Gaps {
        self.workspaces
            .get(workspace)
            .and_then(|w| w.gaps.as_ref())
            .or_else(|| {
                self.monitors.iter().find(|m| m.name == monitor).and_then(|m| m.gaps.as_ref())
            })
            .unwrap_or(&self.gaps)
    }
    pub fn bar_enabled_on(&self, monitor: &str) -> bool {
        self.bar.enabled
            && self
                .monitors
                .iter()
                .find(|m| m.name == monitor)
                .and_then(|m| m.bar_enabled)
                .unwrap_or(true)
    }
    pub fn bar_height_on(&self, monitor: &str) -> i32 {
        self.monitors
            .iter()
            .find(|m| m.name == monitor)
            .and_then(|m| m.bar_height)
            .unwrap_or(self.bar.height)
    }
    pub fn color(&self, value: &str) -> Result<u32, String> {
        self.color_for(value, None)
    }
    /// A single palette resolver is shared by surface fills and native window edges.
    pub fn color_for(&self, value: &str, system_theme: Option<&str>) -> Result<u32, String> {
        let light =
            self.theme == "light" || (self.theme == "system" && system_theme == Some("light"));
        let resolved = if light {
            self.light_palette
                .get(value)
                .or_else(|| self.palette.get(value))
                .map_or(value, String::as_str)
        } else {
            self.palette.get(value).map_or(value, String::as_str)
        };
        let hex = resolved
            .strip_prefix('#')
            .ok_or_else(|| format!("unknown colour {value:?}; use #RRGGBB or a palette name"))?;
        if hex.len() != 6 {
            return Err(format!("colour {value:?} must use #RRGGBB"));
        }
        u32::from_str_radix(hex, 16).map_err(|_| format!("invalid colour {value:?}"))
    }
    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| e.to_string())
    }
}
fn validate_gaps(gaps: &Gaps, borders: &BorderConfig) -> Result<(), String> {
    if !(0..=512).contains(&gaps.inner) || !(0..=512).contains(&gaps.outer) {
        return Err("gaps must be between 0 and 512 pixels".into());
    }
    if borders.enabled && gaps.inner < borders.width {
        return Err("inner gap must be at least the border width".into());
    }
    Ok(())
}
fn error_at(source: &str, message: String, offset: Option<usize>) -> ConfigError {
    let prefix = offset.and_then(|at| source.get(..at));
    ConfigError {
        message,
        line: prefix.map(|s| s.bytes().filter(|&b| b == b'\n').count() + 1),
        column: prefix.map(|s| s.rsplit('\n').next().unwrap_or("").chars().count() + 1),
    }
}
fn merge(base: &mut toml::Value, overlay: toml::Value, key: &str) {
    match (base, overlay) {
        (toml::Value::Table(base), toml::Value::Table(overlay)) => {
            for (key, value) in overlay {
                if let Some(existing) = base.get_mut(&key) {
                    merge(existing, value, &key);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (toml::Value::Array(base), toml::Value::Array(overlay)) if key == "rules" => {
            for rule in overlay {
                if !base.contains(&rule) {
                    base.push(rule);
                }
            }
        }
        (base, value) => *base = value,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct KeyChord {
    /// Win32 virtual-key code.
    pub key: u32,
    /// Alt, Shift, Ctrl, and Win occupy bits 0 through 3, respectively.
    pub modifiers: u8,
}

impl KeyChord {
    pub fn is_task_switch(self) -> bool {
        self.key == 0x09 && matches!(self.modifiers, 1 | 3)
    }
}

pub fn parse_chord(chord: &str) -> Result<KeyChord, String> {
    let normalized = chord.to_ascii_lowercase();
    let parts: Vec<_> = normalized.split('+').map(str::trim).collect();
    let Some((key, modifiers)) = parts.split_last() else {
        return Err("empty key binding".into());
    };
    let mut mask = 0;
    for &modifier in modifiers {
        let bit = match modifier {
            "alt" => 1,
            "shift" => 2,
            "ctrl" | "control" => 4,
            "win" | "super" => 8,
            _ => 0,
        };
        if bit == 0 || mask & bit != 0 {
            return Err(format!("invalid modifier in key binding {chord:?}"));
        }
        mask |= bit;
    }
    // https://learn.microsoft.com/windows/win32/inputdev/virtual-key-codes
    let key = match *key {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" | "return" => 0x0d,
        "esc" | "escape" => 0x1b,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "delete" => 0x2e,
        ";" => 0xba,
        "=" | "equal" | "plus" | "oem_plus" => 0xbb,
        "-" | "minus" | "oem_minus" => 0xbd,
        "/" | "slash" | "oem_question" => 0xbf,
        "[" => 0xdb,
        "]" => 0xdd,
        name if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() => {
            u32::from(name.as_bytes()[0].to_ascii_uppercase())
        }
        name => name
            .strip_prefix('f')
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| (1..=24).contains(n))
            .map(|n| 0x70 + n - 1)
            .ok_or_else(|| format!("unknown key in binding {chord:?}"))?,
    };
    Ok(KeyChord { key, modifiers: mask })
}

pub fn validate_chord(chord: &str) -> Result<(), String> {
    parse_chord(chord).map(|_| ())
}

fn binding_chords<'a>(
    keys: impl Iterator<Item = &'a String>,
) -> Result<BTreeSet<KeyChord>, String> {
    let mut chords = BTreeSet::new();
    for key in keys {
        if !chords.insert(parse_chord(key)?) {
            return Err(format!("duplicate key binding {key:?}"));
        }
    }
    Ok(chords)
}
pub fn schema() -> serde_json::Value {
    let mut schema =
        serde_json::to_value(schemars::schema_for!(Config)).expect("schema serializes");
    // The editor validates a user overlay, while serde handles the fully merged config.
    // Lists replace whole entries, so their item schemas deliberately remain strict.
    schema.as_object_mut().unwrap().remove("required");
    for (field, definition) in [("gaps", "Gaps"), ("bar", "BarConfig"), ("borders", "BorderConfig")]
    {
        let mut overlay = schema["$defs"][definition].clone();
        overlay.as_object_mut().unwrap().remove("required");
        let name = format!("{definition}Override");
        schema["$defs"][&name] = overlay;
        schema["properties"][field] = serde_json::json!({"$ref":format!("#/$defs/{name}")});
    }
    schema
}
/// Reference tables are generated from the same schema used by editors.
pub fn reference_markdown() -> String {
    fn type_name(value: &serde_json::Value) -> String {
        if let Some(reference) = value.get("$ref").and_then(serde_json::Value::as_str) {
            return reference.rsplit('/').next().unwrap_or(reference).into();
        }
        if let Some(variants) = value.get("anyOf").and_then(serde_json::Value::as_array) {
            return variants.iter().map(type_name).collect::<Vec<_>>().join(" or ");
        }
        if value.get("type").and_then(serde_json::Value::as_str) == Some("array") {
            return format!("array of {}", type_name(&value["items"]));
        }
        value
            .get("type")
            .map(|v| v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()))
            .unwrap_or_else(|| "value".into())
    }
    fn table(out: &mut String, name: &str, definition: &serde_json::Value) {
        let Some(properties) = definition.get("properties").and_then(serde_json::Value::as_object)
        else {
            return;
        };
        out.push_str(&format!("\n## {name}\n\n| Field | Type |\n|---|---|\n"));
        for (name, property) in properties {
            out.push_str(&format!("| `{name}` | {} |\n", type_name(property).replace('|', "\\|")));
        }
    }
    let schema = schema();
    let mut out = String::from(
        "# moon configuration reference\n\nGenerated by `moon config reference` from the configuration schema. A personal file is layered over the compiled defaults: bindings merge by chord, rules append, and other lists replace. `moon config default` prints every built-in value. Unknown fields are errors.\n",
    );
    table(&mut out, "Config", &schema);
    if let Some(definitions) = schema.get("$defs").and_then(serde_json::Value::as_object) {
        for (name, definition) in definitions {
            table(&mut out, name, definition);
        }
    }
    out
}
/// v0 was the pre-release draft; upgrading only adds its explicit version.
pub fn migrate(source: &str) -> Result<String, String> {
    if source.len() > MAX_CONFIG_BYTES {
        return Err("config exceeds 1 MiB".into());
    }
    let mut document = source.parse::<toml_edit::DocumentMut>().map_err(|e| e.to_string())?;
    let version = document.get("config-version").and_then(toml_edit::Item::as_integer).unwrap_or(0);
    if !(0..=i64::from(CONFIG_VERSION)).contains(&version) {
        return Err(format!("cannot migrate config-version {version}"));
    }
    if version != i64::from(CONFIG_VERSION) {
        if let Some(value) =
            document.get_mut("config-version").and_then(toml_edit::Item::as_value_mut)
        {
            let decor = value.decor().clone();
            *value = toml_edit::Value::from(i64::from(CONFIG_VERSION));
            *value.decor_mut() = decor;
        } else {
            document["config-version"] = toml_edit::value(i64::from(CONFIG_VERSION));
        }
    }
    let result = document.to_string();
    Config::load(&result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_valid_and_round_trip() {
        let config = Config::load(DEFAULT_CONFIG).unwrap();
        config.validate().unwrap();
        assert_eq!(Config::load(&config.to_toml().unwrap()).unwrap(), config);
        assert_eq!(Config::default(), config);
    }
    #[test]
    fn keys_merge_rules_append_lists_replace() {
        let config = Config::load("[bindings]\n'alt+h'=['focus right']\n[[rules]]\ncommands=['ignore']\nmatches=[{process={equals='test'}}]\n").unwrap();
        assert_eq!(config.bindings["alt+h"], ["focus right"]);
        assert!(config.bindings.contains_key("alt+l"));
        assert_eq!(config.rules.len(), Config::default().rules.len() + 1);
        let config = Config::load("workspaces=[{name='one'}]").unwrap();
        assert_eq!(config.workspaces.len(), 1);
    }
    #[test]
    fn unknown_keys_and_future_versions_fail() {
        assert!(Config::load("[gaps]\ninnre=12").unwrap_err().message.contains("inner"));
        assert!(Config::load("config-version=2").is_err());
    }
    #[test]
    fn settings_command_works_in_bindings_widgets_and_aliases() {
        let config = Config::load("[aliases]\nprefs=['settings']\n[bindings]\n'alt+s'=['prefs']\n[bar]\nwidgets=[{id='preferences',format='Settings',click='settings'}]\n").unwrap();
        assert_eq!(config.commands("prefs").unwrap(), vec![Command::Settings]);
        assert_eq!(config.commands(&config.bindings["alt+s"][0]).unwrap(), vec![Command::Settings]);
        assert_eq!(
            config.commands(config.bar.widgets[0].click.as_deref().unwrap()).unwrap(),
            vec![Command::Settings]
        );
        let world = crate::world::World::new(
            config,
            vec![crate::world::Monitor {
                id: crate::ids::MonitorId(1),
                name: "test".into(),
                bounds: crate::geometry::Rect::new(0, 0, 1920, 1080),
                work_area: crate::geometry::Rect::new(0, 0, 1920, 1080),
                scale: 1.0,
            }],
        )
        .unwrap();
        assert_eq!(
            crate::world::reduce(&world, crate::world::Event::Command(Command::Settings))
                .unwrap()
                .plan
                .control,
            Some(crate::world::Control::Settings)
        );
    }
    #[test]
    fn workspace_animation_defaults_on_and_requires_a_boolean() {
        assert!(Config::load("").unwrap().workspace_animation);
        assert!(!Config::load("workspace-animation=false").unwrap().workspace_animation);
        assert!(Config::load("workspace-animation='false'").is_err());
        assert!(Config::load("layout='carousel'").is_err());
    }
    #[test]
    fn reload_hooks_reject_reload_after_nested_alias_expansion() {
        for source in [
            "[hooks]\nreload=['reload']",
            "[aliases]\nrefresh=['reload']\n[hooks]\nreload=['refresh']",
            "[aliases]\nrefresh=['apply']\napply=['reload']\n[hooks]\nreload=['refresh']",
            "[aliases]\nrefresh=['prepare','reload']\nprepare=['focus left']\n[hooks]\nreload=['refresh']",
        ] {
            let error = Config::load(source).unwrap_err();
            assert!(error.message.contains("reload hooks cannot reload"), "{error}");
        }
        let config = Config::load("[aliases]\nrefresh=['reload']\nrefocus=['focus left']\n[hooks]\nstartup=['refresh']\nreload=['refocus']").unwrap();
        assert_eq!(config.commands(&config.hooks.startup[0]).unwrap(), vec![Command::Reload]);
        assert_eq!(
            config.commands(&config.hooks.reload[0]).unwrap(),
            vec![Command::Focus(crate::geometry::Direction::Left)]
        );
    }
    #[test]
    fn comments_survive_migration() {
        let migrated =
            migrate("# personal\nconfig-version = 0 # retain\n[gaps]\ninner=20\n").unwrap();
        assert!(migrated.contains("# personal"));
        assert!(migrated.contains("1 # retain"));
    }
    #[test]
    fn validates_alias_cycles_and_regexes() {
        assert!(Config::load("[aliases]\na=['b']\nb=['a']").is_err());
        assert!(
            Config::load("[[rules]]\ncommands=['ignore']\nmatches=[{title={regex='['}}]").is_err()
        );
    }

    #[test]
    fn alias_expansion_bounds_fan_out_even_when_no_commands_are_produced() {
        let mut source = String::from("[aliases]\na00=[]\n");
        for index in 1..=15 {
            let child = format!("\"a{:02}\"", index - 1);
            let children = [(); 16].map(|()| child.as_str()).join(",");
            source.push_str(&format!("a{index:02}=[{children}]\n"));
        }
        let error = Config::load(&source).unwrap_err();
        assert!(error.message.contains("alias expansion exceeds limit"));
        assert!(Config::load("[aliases]\nnoop=[]").unwrap().commands("noop").unwrap().is_empty());
    }

    #[test]
    fn physical_chords_override_defaults_and_reject_duplicates_in_the_same_table() {
        assert_eq!(parse_chord("alt+slash"), parse_chord("Alt + OEM_QUESTION"));
        assert_eq!(parse_chord("shift+Alt+equal"), parse_chord("alt+shift+oem_plus"));
        let config = Config::load("[bindings]\n'shift+alt+h'=['close']").unwrap();
        assert_eq!(config.bindings["shift+alt+h"], ["close"]);
        assert!(!config.bindings.contains_key("alt+shift+h"));
        for table in ["bindings", "modes.example"] {
            let error = Config::load(&format!("[{table}]\n'alt+slash'=[]\n'alt+oem_question'=[]"))
                .unwrap_err();
            assert!(error.message.contains("duplicate key binding"));
        }
    }

    #[test]
    fn app_rules_validate_workspace_references_through_aliases() {
        let source = "workspaces=[{name='Web Browsing'}]\n[aliases]\nroute=['move-to-workspace Web Browsing']\n[[rules]]\ncommands=['route']\nmatches=[{process={equals='Browser'}}]";
        let config = Config::load(source).unwrap();
        assert_eq!(config.bindings["alt+2"], ["workspace 2"]);
        let invalid = source.replace("move-to-workspace Web Browsing", "move-to-workspace Missing");
        assert!(Config::load(&invalid).unwrap_err().message.contains("unknown workspace"));
    }
    #[test]
    fn editor_schema_accepts_partial_root_sections_without_weakening_replaced_items() {
        let schema = schema();
        assert!(schema.get("required").is_none());
        assert!(schema["$defs"]["GapsOverride"].get("required").is_none());
        assert!(
            schema["$defs"]["WorkspaceConfig"]["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("name"))
        );
        assert!(Config::load("[gaps]\ninner=24").is_ok());
        assert!(Config::load("workspaces=[{}]").is_err());
    }
    #[test]
    fn aliases_modes_and_hooks_share_the_validated_command_language() {
        let config = Config::load(
            r#"
[aliases]
arrange=["focus left","resize width +2%"]
[modes.resize]
h=["resize width -2%"]
escape=["mode default"]
[bindings]
"alt+r"=["mode resize"]
[hooks]
startup=["exec example.exe --ready"]
shutdown=["exec example.exe --stopped"]
reload=["arrange"]
"#,
        )
        .unwrap();
        assert_eq!(config.commands("arrange").unwrap().len(), 2);
        let monitors = vec![crate::world::Monitor {
            id: crate::ids::MonitorId(1),
            name: "test".into(),
            bounds: crate::geometry::Rect::new(0, 0, 1920, 1080),
            work_area: crate::geometry::Rect::new(0, 0, 1920, 1080),
            scale: 1.0,
        }];
        let world = crate::world::World::new(config, monitors).unwrap();
        let changed = crate::world::reduce(
            &world,
            crate::world::Event::Command(Command::Mode("resize".into())),
        )
        .unwrap();
        assert_eq!(changed.world.mode, "resize");
        let launch = crate::world::reduce(
            &world,
            crate::world::Event::Command(
                world.config.commands(&world.config.hooks.startup[0]).unwrap().remove(0),
            ),
        )
        .unwrap();
        assert_eq!(launch.plan.launch, ["example.exe --ready"]);
        assert!(
            crate::world::reduce(
                &world,
                crate::world::Event::Command(Command::Mode("absent".into()))
            )
            .is_err()
        );
    }
    #[test]
    fn stock_rules_cover_office_and_dialogs() {
        let config = Config::default();
        for process in ["moon", "moon-settings"] {
            assert!(config.rules.iter().any(|r| r.commands == ["ignore"]
                && r.matches.iter().any(|m| m.matches(process, "MoonSettings", "Moon Settings"))));
        }
        assert!(config.rules.iter().any(|r| r.commands == ["ignore"]
            && r.matches.iter().any(|m| m.matches("EXCEL", "dialog", ""))));
        assert!(!config.rules.iter().any(|r| r.commands == ["ignore"]
            && r.matches.iter().any(|m| m.matches("EXCEL", "XLMAIN", "Book"))));
    }

    #[test]
    fn weather_city_label_is_optional_bounded_and_preserved() {
        let old = Config::load("[weather]\nlatitude=-33.87\nlongitude=151.21\n").unwrap();
        assert_eq!(old.weather.as_ref().unwrap().location, None);
        assert!(serde_json::to_value(&old).unwrap()["weather"].get("location").is_none());
        let mut config = old;
        config.weather.as_mut().unwrap().location =
            Some("Sydney, New South Wales, Australia".into());
        config.validate().unwrap();
        let serialized = serde_json::to_value(&config).unwrap();
        let restored: Config = serde_json::from_value(serialized).unwrap();
        assert_eq!(restored.weather, config.weather);
        for invalid in ["".into(), "  ".into(), "x".repeat(201), "Sydney\nAustralia".into()] {
            config.weather.as_mut().unwrap().location = Some(invalid);
            assert!(config.validate().is_err());
        }
    }
}
