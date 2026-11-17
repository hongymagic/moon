//! Named, command-free presets. Applying returns draft edits; only the sidecar is written here.
use crate::store::{read_source, resolve_path, with_locked_path, write_atomically};
use moon_core::{
    config::{BarConfig, BorderConfig, Config, Gaps, MonitorConfig, WorkspaceConfig},
    settings::{Edit, ErrorCode, PathSegment, SettingsError},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

const VERSION: u32 = 1;
const MAX_PRESET_BYTES: usize = 65_536;
const MAX_ENTRIES: usize = 64;
const BUILTINS: [&str; 3] = ["Linen", "Slate", "Forest"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Profile,
    Theme,
}

#[derive(Debug, Serialize)]
pub struct Catalog {
    pub source: Option<String>,
    pub entries: Vec<Entry>,
}
#[derive(Debug, Serialize)]
pub struct Entry {
    pub kind: Kind,
    pub name: String,
    pub builtin: bool,
    pub values: Value,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    kind: Kind,
    name: String,
    values: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    entries: Vec<Saved>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    version: u32,
    kind: Kind,
    name: String,
    values: Value,
}

/// Explicit style allowlist: widget formats, commands and private paths cannot be shared.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct BarStyle {
    height: i32,
    font: String,
    font_size: f32,
    background: String,
    foreground: String,
    opacity: f32,
    backdrop: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct BorderStyle {
    width: i32,
    radius: i32,
    focused: String,
    focused_end: String,
    unfocused: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Theme {
    theme: String,
    palette: BTreeMap<String, String>,
    light_palette: BTreeMap<String, String>,
    bar: BarStyle,
    borders: BorderStyle,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Profile {
    appearance: Theme,
    bar_enabled: bool,
    borders_enabled: bool,
    gaps: Gaps,
    workspaces: Vec<WorkspaceConfig>,
    monitors: Vec<MonitorConfig>,
    workspace_animation: bool,
    cursor_jump: bool,
    focus_follows_cursor: bool,
}

pub struct Library {
    config_path: PathBuf,
}
impl Library {
    pub fn new(config_path: PathBuf) -> Self {
        Self { config_path }
    }

    fn path(&self) -> Result<PathBuf, SettingsError> {
        let mut path = resolve_path(&self.config_path)?.into_os_string();
        path.push(".moon-library.json");
        Ok(path.into())
    }

    pub fn load(&self) -> Result<Catalog, SettingsError> {
        let source = read_source(&self.path()?)?;
        let document = parse_document(source.as_deref())?;
        catalog(source, document)
    }

    pub fn save(
        &self,
        expected: Option<&str>,
        kind: Kind,
        name: &str,
        config: &Config,
        replace: bool,
    ) -> Result<Catalog, SettingsError> {
        config.validate().map_err(invalid)?;
        let values = match kind {
            Kind::Theme => to_value(&Theme::capture(config))?,
            Kind::Profile => to_value(&Profile {
                appearance: Theme::capture(config),
                bar_enabled: config.bar.enabled,
                borders_enabled: config.borders.enabled,
                gaps: config.gaps.clone(),
                workspaces: config.workspaces.clone(),
                monitors: config.monitors.clone(),
                workspace_animation: config.workspace_animation,
                cursor_jump: config.cursor_jump,
                focus_follows_cursor: config.focus_follows_cursor,
            })?,
        };
        self.put(expected, Saved { kind, name: name.into(), values }, replace)
    }

    pub fn rename(
        &self,
        expected: Option<&str>,
        kind: Kind,
        name: &str,
        new_name: &str,
    ) -> Result<Catalog, SettingsError> {
        mutable_name(kind, name)?;
        mutable_name(kind, new_name)?;
        self.change(expected, |entries| {
            let index = find(entries, kind, name).ok_or_else(not_found)?;
            if find(entries, kind, new_name).is_some_and(|other| other != index) {
                return Err(duplicate());
            }
            entries[index].name = new_name.into();
            Ok(())
        })
    }

    pub fn delete(
        &self,
        expected: Option<&str>,
        kind: Kind,
        name: &str,
    ) -> Result<Catalog, SettingsError> {
        mutable_name(kind, name)?;
        self.change(expected, |entries| {
            let index = find(entries, kind, name).ok_or_else(not_found)?;
            entries.remove(index);
            Ok(())
        })
    }

    pub fn import(
        &self,
        expected: Option<&str>,
        json: &str,
        replace: bool,
    ) -> Result<Catalog, SettingsError> {
        if json.len() > MAX_PRESET_BYTES {
            return Err(invalid("A shared preset must be at most 64 KiB."));
        }
        let exchange: Exchange = serde_json::from_str(json).map_err(json_error)?;
        version(exchange.version)?;
        self.put(
            expected,
            Saved { kind: exchange.kind, name: exchange.name, values: exchange.values },
            replace,
        )
    }

    pub fn export(&self, kind: Kind, name: &str) -> Result<String, SettingsError> {
        let entry = self.entry(kind, name)?;
        let output = serde_json::to_string_pretty(&Exchange {
            version: VERSION,
            kind,
            name: entry.name,
            values: entry.values,
        })
        .map_err(json_error)?;
        if output.len() > MAX_PRESET_BYTES {
            return Err(invalid("A shared preset must be at most 64 KiB."));
        }
        Ok(output)
    }

    pub fn apply(
        &self,
        kind: Kind,
        name: &str,
        config: &Config,
    ) -> Result<Vec<Edit>, SettingsError> {
        config.validate().map_err(invalid)?;
        let entry = self.entry(kind, name)?;
        edits(kind, entry.values, config)
    }

    fn entry(&self, kind: Kind, name: &str) -> Result<Entry, SettingsError> {
        valid_name(name)?;
        self.load()?
            .entries
            .into_iter()
            .find(|entry| entry.kind == kind && same_name(&entry.name, name))
            .ok_or_else(not_found)
    }

    fn put(
        &self,
        expected: Option<&str>,
        saved: Saved,
        replace: bool,
    ) -> Result<Catalog, SettingsError> {
        validate_saved(&saved)?;
        self.change(expected, |entries| {
            if let Some(index) = find(entries, saved.kind, &saved.name) {
                if !replace {
                    return Err(duplicate());
                }
                entries[index] = saved;
            } else {
                if entries.len() == MAX_ENTRIES {
                    return Err(invalid("The library can contain up to 64 saved items."));
                }
                entries.push(saved);
            }
            Ok(())
        })
    }

    fn change(
        &self,
        expected: Option<&str>,
        change: impl FnOnce(&mut Vec<Saved>) -> Result<(), SettingsError>,
    ) -> Result<Catalog, SettingsError> {
        with_locked_path(&self.path()?, |path| {
            let current = read_source(path)?;
            if current.as_deref() != expected {
                return Err(conflict());
            }
            let mut document = parse_document(current.as_deref())?;
            change(&mut document.entries)?;
            let source = serde_json::to_string_pretty(&document).map_err(json_error)?;
            if current.as_deref() != Some(&source) {
                write_atomically(path, current.as_deref(), &source)?;
            }
            catalog(Some(source), document)
        })
    }
}

impl Theme {
    fn capture(config: &Config) -> Self {
        let bar = &config.bar;
        Self {
            theme: config.theme.clone(),
            palette: config.palette.clone(),
            light_palette: config.light_palette.clone(),
            bar: BarStyle {
                height: bar.height,
                font: bar.font.clone(),
                font_size: bar.font_size,
                background: bar.background.clone(),
                foreground: bar.foreground.clone(),
                opacity: bar.opacity,
                backdrop: bar.backdrop.clone(),
            },
            borders: BorderStyle {
                width: config.borders.width,
                radius: config.borders.radius,
                focused: config.borders.focused.clone(),
                focused_end: config.borders.focused_end.clone(),
                unfocused: config.borders.unfocused.clone(),
            },
        }
    }

    fn update(&self, config: &mut Config) {
        config.theme.clone_from(&self.theme);
        // Keep custom widget colours which are outside this preset's palette.
        config.palette.extend(self.palette.clone());
        config.light_palette.extend(self.light_palette.clone());
        let widgets = std::mem::take(&mut config.bar.widgets);
        config.bar = BarConfig {
            enabled: config.bar.enabled,
            height: self.bar.height,
            font: self.bar.font.clone(),
            font_size: self.bar.font_size,
            background: self.bar.background.clone(),
            foreground: self.bar.foreground.clone(),
            opacity: self.bar.opacity,
            backdrop: self.bar.backdrop.clone(),
            widgets,
        };
        config.borders = BorderConfig {
            enabled: config.borders.enabled,
            width: self.borders.width,
            radius: self.borders.radius,
            focused: self.borders.focused.clone(),
            focused_end: self.borders.focused_end.clone(),
            unfocused: self.borders.unfocused.clone(),
        };
    }

    fn validate(&self) -> Result<(), SettingsError> {
        for palette in [&self.palette, &self.light_palette] {
            if palette.len() > 128 {
                return Err(invalid("A palette can contain up to 128 colours."));
            }
            for (key, value) in palette {
                if key.is_empty()
                    || key.len() > 64
                    || !key.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
                {
                    return Err(invalid(
                        "Colour names must use up to 64 letters, digits, hyphens or underscores.",
                    ));
                }
                if value.len() > 64 || value.chars().any(char::is_control) {
                    return Err(invalid("A colour value is invalid."));
                }
            }
        }
        if self.bar.font.trim().is_empty()
            || self.bar.font.chars().count() > 128
            || self.bar.font.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
        {
            return Err(invalid("Use a font family name, not a font file path."));
        }
        Ok(())
    }

    fn append_edits(&self, out: &mut Vec<Edit>) -> Result<(), SettingsError> {
        set(out, &["theme"], &self.theme)?;
        for (section, palette) in
            [("palette", &self.palette), ("light-palette", &self.light_palette)]
        {
            for (key, value) in palette {
                set(out, &[section, key], value)?;
            }
        }
        for (key, value) in
            to_value(&self.bar)?.as_object().expect("BarStyle serializes as an object")
        {
            set(out, &["bar", key], value)?;
        }
        for (key, value) in
            to_value(&self.borders)?.as_object().expect("BorderStyle serializes as an object")
        {
            set(out, &["borders", key], value)?;
        }
        Ok(())
    }
}

fn edits(kind: Kind, values: Value, base: &Config) -> Result<Vec<Edit>, SettingsError> {
    let mut merged = base.clone();
    let mut out = Vec::new();
    match kind {
        Kind::Theme => {
            let theme: Theme = serde_json::from_value(values).map_err(json_error)?;
            theme.validate()?;
            theme.update(&mut merged);
            theme.append_edits(&mut out)?;
        }
        Kind::Profile => {
            let profile: Profile = serde_json::from_value(values).map_err(json_error)?;
            profile.appearance.validate()?;
            profile.appearance.update(&mut merged);
            merged.bar.enabled = profile.bar_enabled;
            merged.borders.enabled = profile.borders_enabled;
            merged.gaps = profile.gaps.clone();
            merged.workspaces.clone_from(&profile.workspaces);
            merged.monitors.clone_from(&profile.monitors);
            merged.workspace_animation = profile.workspace_animation;
            merged.cursor_jump = profile.cursor_jump;
            merged.focus_follows_cursor = profile.focus_follows_cursor;
            profile.appearance.append_edits(&mut out)?;
            set(&mut out, &["bar", "enabled"], &profile.bar_enabled)?;
            set(&mut out, &["borders", "enabled"], &profile.borders_enabled)?;
            set(&mut out, &["gaps", "inner"], &profile.gaps.inner)?;
            set(&mut out, &["gaps", "outer"], &profile.gaps.outer)?;
            set(&mut out, &["workspaces"], &profile.workspaces)?;
            set(&mut out, &["monitors"], &profile.monitors)?;
            set(&mut out, &["workspace-animation"], &profile.workspace_animation)?;
            set(&mut out, &["cursor-jump"], &profile.cursor_jump)?;
            set(&mut out, &["focus-follows-cursor"], &profile.focus_follows_cursor)?;
        }
    }
    merged.validate().map_err(invalid)?;
    Ok(out)
}

fn set(out: &mut Vec<Edit>, path: &[&str], value: &impl Serialize) -> Result<(), SettingsError> {
    out.push(Edit::Set {
        path: path.iter().map(|key| PathSegment::Key((*key).into())).collect(),
        value: to_value(value)?,
    });
    Ok(())
}
fn to_value(value: &impl Serialize) -> Result<Value, SettingsError> {
    serde_json::to_value(value).map_err(json_error)
}
fn json_error(error: serde_json::Error) -> SettingsError {
    invalid(format!("The preset is not valid: {error}"))
}
fn invalid(message: impl Into<String>) -> SettingsError {
    SettingsError::new(
        ErrorCode::Validation,
        message,
        "Check the preset and try again. Your configuration has not changed.",
    )
}
fn conflict() -> SettingsError {
    SettingsError::new(
        ErrorCode::Conflict,
        "The library changed in another Settings window.",
        "Refresh the library before trying again; no items were overwritten.",
    )
}
fn duplicate() -> SettingsError {
    SettingsError::new(
        ErrorCode::Conflict,
        "An item with this name already exists.",
        "Choose another name or explicitly replace the existing item.",
    )
}
fn not_found() -> SettingsError {
    invalid("This library item no longer exists. Refresh the library.")
}
fn version(value: u32) -> Result<(), SettingsError> {
    if value == VERSION { Ok(()) } else { Err(invalid("This library version is not supported.")) }
}
fn same_name(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}
fn valid_name(name: &str) -> Result<(), SettingsError> {
    if name.is_empty()
        || name.trim() != name
        || name.chars().count() > 64
        || name.len() > 256
        || name.chars().any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(invalid(
            "Use a name of 1–64 characters without surrounding spaces, control characters or path separators.",
        ));
    }
    Ok(())
}
fn mutable_name(kind: Kind, name: &str) -> Result<(), SettingsError> {
    valid_name(name)?;
    if kind == Kind::Theme && BUILTINS.iter().any(|builtin| same_name(builtin, name)) {
        return Err(invalid("Built-in themes are read-only. Save a copy with another name."));
    }
    Ok(())
}
fn find(entries: &[Saved], kind: Kind, name: &str) -> Option<usize> {
    entries.iter().position(|entry| entry.kind == kind && same_name(&entry.name, name))
}

fn validate_saved(saved: &Saved) -> Result<(), SettingsError> {
    mutable_name(saved.kind, &saved.name)?;
    let exchange = Exchange {
        version: VERSION,
        kind: saved.kind,
        name: saved.name.clone(),
        values: saved.values.clone(),
    };
    if serde_json::to_string_pretty(&exchange).map_err(json_error)?.len() > MAX_PRESET_BYTES {
        return Err(invalid("A shared preset must be at most 64 KiB."));
    }
    // A theme has no gap settings. Validate its own ranges here; compatibility
    // with the recipient's gaps is checked against that draft when applying.
    let mut validation_base = Config::default();
    if saved.kind == Kind::Theme {
        validation_base.gaps = Gaps { inner: 512, outer: 512 };
    }
    edits(saved.kind, saved.values.clone(), &validation_base)?;
    Ok(())
}
fn parse_document(source: Option<&str>) -> Result<Document, SettingsError> {
    let Some(source) = source else {
        return Ok(Document { version: VERSION, entries: Vec::new() });
    };
    let document: Document = serde_json::from_str(source).map_err(json_error)?;
    version(document.version)?;
    if document.entries.len() > MAX_ENTRIES {
        return Err(invalid("The library can contain up to 64 saved items."));
    }
    let mut names = BTreeSet::new();
    for saved in &document.entries {
        validate_saved(saved)?;
        if !names.insert((saved.kind as u8, saved.name.to_lowercase())) {
            return Err(duplicate());
        }
    }
    Ok(document)
}
fn catalog(source: Option<String>, document: Document) -> Result<Catalog, SettingsError> {
    let mut entries = Vec::new();
    for name in BUILTINS {
        entries.push(Entry {
            kind: Kind::Theme,
            name: name.into(),
            builtin: true,
            values: to_value(&builtin(name))?,
        });
    }
    entries.extend(document.entries.into_iter().map(|entry| Entry {
        kind: entry.kind,
        name: entry.name,
        builtin: false,
        values: entry.values,
    }));
    Ok(Catalog { source, entries })
}
fn builtin(name: &str) -> Theme {
    let mut theme = Theme::capture(&Config::default());
    theme.theme = "system".into();
    let (dark, light) = match name {
        "Slate" => (
            ["#111820", "#1a2530", "#263746", "#e6edf3", "#8db3d1", "#5885aa"],
            ["#edf2f7", "#e2eaf1", "#cddbe6", "#203346", "#386889", "#284f70"],
        ),
        "Forest" => (
            ["#141b18", "#202d25", "#304637", "#e8eee4", "#9bb789", "#648561"],
            ["#eff1e9", "#e1e7d8", "#cbd7c1", "#273c2d", "#4d7043", "#36552f"],
        ),
        _ => (
            ["#191715", "#28231f", "#3c342c", "#f2e9dc", "#d69c78", "#ad714f"],
            ["#f4eee4", "#e8dfd1", "#d8c9b6", "#352b24", "#995c38", "#784227"],
        ),
    };
    for (palette, colors) in [(&mut theme.palette, dark), (&mut theme.light_palette, light)] {
        for (key, color) in
            ["base", "surface0", "surface1", "text", "terracotta", "rust"].into_iter().zip(colors)
        {
            palette.insert(key.into(), color.into());
        }
    }
    theme
}

#[cfg(test)]
mod tests {
    use super::*;
    use moon_core::settings;
    use serde_json::json;
    use std::{
        fs,
        sync::{
            Arc, Barrier,
            atomic::{AtomicU64, Ordering},
        },
        thread,
    };

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "moon-library-test-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            )))
        }
        fn config_path(&self) -> PathBuf {
            self.0.join("config.toml")
        }
        fn library(&self) -> Library {
            Library::new(self.config_path())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn exchange(library: &Library) -> Value {
        let mut value: Value =
            serde_json::from_str(&library.export(Kind::Theme, "Linen").unwrap()).unwrap();
        value["name"] = json!("My theme");
        value
    }

    #[test]
    fn builtins_offer_valid_light_dark_pairs_without_creating_any_files() {
        let fixture = Fixture::new();
        let library = fixture.library();
        let catalog = library.load().unwrap();
        assert!(catalog.source.is_none());
        assert_eq!(catalog.entries.len(), 3);
        for entry in catalog.entries {
            assert!(entry.builtin);
            assert_eq!(entry.values["theme"], "system");
            assert_ne!(entry.values["palette"]["base"], entry.values["light-palette"]["base"]);
            let edits = library.apply(entry.kind, &entry.name, &Config::default()).unwrap();
            let update = settings::prepare_update(None, None, &edits).unwrap();
            update.config.validate().unwrap();
            for mode in ["dark", "light"] {
                update.config.color_for("base", Some(mode)).unwrap();
                update.config.color_for("text", Some(mode)).unwrap();
            }
        }
        assert!(!fixture.0.exists());
    }

    #[test]
    fn profile_roundtrip_keeps_commands_and_private_settings_out_of_shared_json() {
        let fixture = Fixture::new();
        let library = fixture.library();
        let source = "# personal header\ntheme='dark'\n[hooks]\nstartup=['exec private-startup-sentinel']\n[aliases]\nprivate_alias=['exec private-alias-sentinel']\n[[bar.widgets]]\nid='personal'\nformat='private-format-sentinel'\nexec='private-widget-sentinel'\nclick='exec private-click-sentinel'\n[weather]\nlatitude=12.3456\nlongitude=78.9012\n";
        fs::create_dir_all(&fixture.0).unwrap();
        fs::write(fixture.config_path(), source).unwrap();
        let mut config = Config::load(source).unwrap();
        config.gaps.inner = 18;
        config.workspaces[0].name = "Writing".into();
        config.workspaces[0].monitor = Some("DISPLAY1".into());
        config.workspace_animation = false;
        let saved = library.save(None, Kind::Profile, "Writing desk", &config, false).unwrap();
        assert!(saved.source.is_some());
        let json = library.export(Kind::Profile, "writing DESK").unwrap();
        for secret in
            ["private-", "12.3456", "78.9012", "bindings", "hooks", "rules", "aliases", "widgets"]
        {
            assert!(!json.contains(secret), "shared {secret}");
        }
        let edits =
            library.apply(Kind::Profile, "Writing desk", &Config::load(source).unwrap()).unwrap();
        let update = settings::prepare_update(Some(source), Some(source), &edits).unwrap();
        assert!(update.source.contains("# personal header"));
        assert!(update.source.contains("startup=['exec private-startup-sentinel']"));
        assert_eq!(update.config.bar.widgets, config.bar.widgets);
        assert_eq!(update.config.hooks, config.hooks);
        assert_eq!(update.config.aliases, config.aliases);
        assert_eq!(update.config.bindings, config.bindings);
        assert_eq!(update.config.rules, config.rules);
        assert_eq!(update.config.weather, config.weather);
        assert_eq!(update.config.gaps.inner, 18);
        assert_eq!(update.config.workspaces[0].name, "Writing");
        assert!(!update.config.workspace_animation);
        assert_eq!(fs::read_to_string(fixture.config_path()).unwrap(), source);
        let other = Fixture::new();
        let imported = other.library().import(None, &json, false).unwrap();
        assert!(imported.entries.iter().any(|entry| entry.name == "Writing desk"));
    }

    #[test]
    fn theme_preserves_widget_actions_and_custom_palette_tokens() {
        let fixture = Fixture::new();
        let source = "# leave this comment\n[palette]\ncompany='#123456' # keep this too\n[[bar.widgets]]\nid='personal'\nformat='{clock}'\ncolor='company'\nclick='exec notepad'\n";
        let config = Config::load(source).unwrap();
        let edits = fixture.library().apply(Kind::Theme, "Forest", &config).unwrap();
        let update = settings::prepare_update(Some(source), Some(source), &edits).unwrap();
        assert_eq!(update.config.palette["company"], "#123456");
        assert_eq!(update.config.bar.widgets, config.bar.widgets);
        assert_eq!(update.config.workspaces, config.workspaces);
        assert!(update.source.contains("company='#123456' # keep this too"));
        assert!(update.source.contains("# leave this comment"));
    }

    #[test]
    fn themes_save_valid_wide_borders_but_check_recipient_gaps_before_apply() {
        let fixture = Fixture::new();
        let library = fixture.library();
        let mut config = Config::default();
        config.borders.width = 32;
        config.gaps.inner = 48;
        library.save(None, Kind::Theme, "Wide borders", &config, false).unwrap();
        let document = library.export(Kind::Theme, "Wide borders").unwrap();
        let other = Fixture::new();
        other.library().import(None, &document, false).unwrap();
        assert!(other.library().apply(Kind::Theme, "Wide borders", &Config::default()).is_err());
        assert!(other.library().apply(Kind::Theme, "Wide borders", &config).is_ok());
    }

    #[test]
    fn themes_preserve_visibility_while_profiles_restore_it() {
        let fixture = Fixture::new();
        let library = fixture.library();
        let source = "[bar]\nenabled=false\n[borders]\nenabled=false\n";
        let config = Config::load(source).unwrap();
        let themed = settings::prepare_update(
            Some(source),
            Some(source),
            &library.apply(Kind::Theme, "Linen", &config).unwrap(),
        )
        .unwrap();
        assert!(!themed.config.bar.enabled);
        assert!(!themed.config.borders.enabled);
        let shared: Value =
            serde_json::from_str(&library.export(Kind::Theme, "Linen").unwrap()).unwrap();
        assert!(shared["values"]["bar"].get("enabled").is_none());
        assert!(shared["values"]["borders"].get("enabled").is_none());
        library.save(None, Kind::Profile, "Visible", &Config::default(), false).unwrap();
        let profile = settings::prepare_update(
            Some(source),
            Some(source),
            &library.apply(Kind::Profile, "Visible", &config).unwrap(),
        )
        .unwrap();
        assert!(profile.config.bar.enabled);
        assert!(profile.config.borders.enabled);
    }

    #[test]
    fn unsafe_unknown_invalid_and_oversized_imports_never_change_library() {
        let fixture = Fixture::new();
        let library = fixture.library();
        let saved = library.save(None, Kind::Theme, "Original", &Config::default(), false).unwrap();
        let original = saved.source.unwrap();
        let baseline = exchange(&library);
        let mut unsafe_cases = Vec::new();
        for (path, value) in [
            (vec!["command"], json!("exec injected")),
            (vec!["version"], json!(2)),
            (vec!["kind"], json!("automation")),
            (vec!["name"], json!("../outside")),
            (vec!["name"], json!("x".repeat(65))),
            (vec!["values", "hooks"], json!({"startup":["exec injected"]})),
            (vec!["values", "bar", "widgets"], json!([{ "exec":"injected" }])),
            (vec!["values", "bar", "font"], json!("C:\\private\\font.ttf")),
            (vec!["values", "bar", "height"], json!(1000)),
            (vec!["values", "palette", "bad/key"], json!("#123456")),
            (vec!["values", "borders", "focused"], json!("missing-colour")),
        ] {
            let mut candidate = baseline.clone();
            let mut destination = &mut candidate;
            for key in path {
                destination = &mut destination[key];
            }
            *destination = value;
            unsafe_cases.push(candidate.to_string());
        }
        unsafe_cases.push(" ".repeat(MAX_PRESET_BYTES + 1));
        for candidate in unsafe_cases {
            assert!(
                library.import(Some(&original), &candidate, false).is_err(),
                "accepted {candidate}"
            );
            assert_eq!(
                read_source(&library.path().unwrap()).unwrap().as_deref(),
                Some(original.as_str())
            );
        }
    }

    #[test]
    fn explicit_replace_rename_delete_and_source_tokens_prevent_lost_updates() {
        let fixture = Fixture::new();
        let library = fixture.library();
        let saved = library.save(None, Kind::Theme, "Personal", &Config::default(), false).unwrap();
        assert_eq!(
            library
                .save(saved.source.as_deref(), Kind::Theme, "PERSONAL", &Config::default(), false)
                .unwrap_err()
                .code,
            ErrorCode::Conflict
        );
        let mut changed = Config::default();
        changed.bar.height = 48;
        let replaced =
            library.save(saved.source.as_deref(), Kind::Theme, "Personal", &changed, true).unwrap();
        assert_eq!(
            library
                .rename(saved.source.as_deref(), Kind::Theme, "Personal", "Changed")
                .unwrap_err()
                .code,
            ErrorCode::Conflict
        );
        let renamed =
            library.rename(replaced.source.as_deref(), Kind::Theme, "Personal", "Renamed").unwrap();
        assert_eq!(library.load().unwrap().entries.last().unwrap().values["bar"]["height"], 48);
        assert!(library.export(Kind::Theme, "Personal").is_err());
        assert!(library.delete(renamed.source.as_deref(), Kind::Theme, "LINEN").is_err());
        assert!(
            library.save(renamed.source.as_deref(), Kind::Theme, "Linen", &changed, true).is_err()
        );
        let deleted = library.delete(renamed.source.as_deref(), Kind::Theme, "renamed").unwrap();
        assert_eq!(deleted.entries.len(), 3);
        assert!(!fixture.config_path().exists());
    }

    #[test]
    fn concurrent_writers_share_one_lock_and_only_one_expected_source_can_commit() {
        let fixture = Fixture::new();
        let barrier = Arc::new(Barrier::new(2));
        let workers: Vec<_> = (0..2)
            .map(|index| {
                let library = fixture.library();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    library.save(
                        None,
                        Kind::Profile,
                        &format!("Desk {index}"),
                        &Config::default(),
                        false,
                    )
                })
            })
            .collect();
        let results: Vec<_> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| result
                    .as_ref()
                    .is_err_and(|error| error.code == ErrorCode::Conflict))
                .count(),
            1
        );
        assert_eq!(fixture.library().load().unwrap().entries.len(), 4);
    }

    #[test]
    fn corrupt_or_duplicate_sidecars_are_not_silently_replaced() {
        let fixture = Fixture::new();
        let library = fixture.library();
        fs::create_dir_all(&fixture.0).unwrap();
        let entry = Saved {
            kind: Kind::Theme,
            name: "Same".into(),
            values: to_value(&builtin("Linen")).unwrap(),
        };
        let duplicate = Document { version: VERSION, entries: vec![entry.clone(), entry] };
        for corrupt in ["{broken".into(), serde_json::to_string(&duplicate).unwrap()] {
            fs::write(library.path().unwrap(), &corrupt).unwrap();
            assert!(library.load().is_err());
            assert!(
                library
                    .save(Some(&corrupt), Kind::Profile, "New", &Config::default(), false)
                    .is_err()
            );
            assert_eq!(fs::read_to_string(library.path().unwrap()).unwrap(), corrupt);
        }
    }

    #[test]
    fn applying_profile_checks_existing_rules_instead_of_dropping_them() {
        let fixture = Fixture::new();
        let library = fixture.library();
        let mut minimal = Config::default();
        minimal.workspaces.truncate(1);
        library.save(None, Kind::Profile, "One workspace", &minimal, false).unwrap();
        let source =
            "[[rules]]\ncommands=['move-to-workspace 2']\nmatches=[{process={equals='notepad'}}]\n";
        let config = Config::load(source).unwrap();
        assert!(library.apply(Kind::Profile, "One workspace", &config).is_err());
        assert_eq!(Config::load(source).unwrap(), config);
    }

    #[test]
    fn collection_limit_is_checked_without_discarding_existing_entries() {
        let fixture = Fixture::new();
        let library = fixture.library();
        fs::create_dir_all(&fixture.0).unwrap();
        let values = to_value(&builtin("Slate")).unwrap();
        let document = Document {
            version: VERSION,
            entries: (0..MAX_ENTRIES)
                .map(|index| Saved {
                    kind: Kind::Theme,
                    name: format!("Theme {index}"),
                    values: values.clone(),
                })
                .collect(),
        };
        let source = serde_json::to_string(&document).unwrap();
        fs::write(library.path().unwrap(), &source).unwrap();
        assert!(
            library
                .save(Some(&source), Kind::Theme, "One more", &Config::default(), false)
                .is_err()
        );
        assert_eq!(fs::read_to_string(library.path().unwrap()).unwrap(), source);
    }

    #[cfg(unix)]
    #[test]
    fn configuration_aliases_use_the_same_sidecar() {
        let fixture = Fixture::new();
        fs::create_dir_all(&fixture.0).unwrap();
        fs::write(fixture.config_path(), "").unwrap();
        let alias = fixture.0.join("alias.toml");
        std::os::unix::fs::symlink(fixture.config_path(), &alias).unwrap();
        let alias_library = Library::new(alias);
        let saved =
            alias_library.save(None, Kind::Theme, "Alias", &Config::default(), false).unwrap();
        assert_eq!(fixture.library().load().unwrap().source, saved.source);
        assert_eq!(alias_library.path().unwrap(), fixture.library().path().unwrap());
    }
}
