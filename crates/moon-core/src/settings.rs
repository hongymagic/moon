//! Validated, comment-preserving edits to the user's TOML overlay.
//!
//! Paths address the overlay. An inherited replacement list is copied from the
//! defaults before editing a row; `rules` indices always address personal rules.
//! Reset removes an override. It never removes a built-in rule or migrates a file.
use crate::config::{self, Config, ConfigError};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use toml_edit::{DocumentMut, Item, Table, Value};

pub const MAX_SETTINGS_BYTES: usize = config::MAX_CONFIG_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathSegment {
    Key(String),
    Index(usize),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Edit {
    Set { path: Vec<PathSegment>, value: Json },
    Reset { path: Vec<PathSegment> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    Conflict,
    InvalidConfig,
    InvalidPath,
    InvalidValue,
    Validation,
    Io,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsError {
    pub code: ErrorCode,
    pub message: String,
    pub path: Option<Vec<PathSegment>>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub help: String,
}
impl SettingsError {
    pub fn new(code: ErrorCode, message: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
            line: None,
            column: None,
            help: help.into(),
        }
    }
    pub fn conflict() -> Self {
        Self::new(
            ErrorCode::Conflict,
            "The configuration changed after these settings were opened.",
            "Reload the settings before saving; your changes have not overwritten the file.",
        )
    }
    fn config(error: ConfigError, code: ErrorCode) -> Self {
        Self {
            code,
            message: error.message,
            path: None,
            line: error.line,
            column: error.column,
            help:
                "Correct the configuration and try again. The existing file has not been changed."
                    .into(),
        }
    }
    fn at(mut self, path: &[PathSegment]) -> Self {
        self.path = Some(path.to_vec());
        self
    }
}
impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line) = self.line {
            write!(f, "line {line}, column {}: ", self.column.unwrap_or(1))?;
        }
        write!(f, "{} {}", self.message, self.help)
    }
}
impl std::error::Error for SettingsError {}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    /// Exact concurrency token. `None` distinguishes an absent file from an empty one.
    pub source: Option<String>,
    pub config: Config,
    /// Only personal overrides. In particular, this contains no built-in rules.
    pub overrides: Json,
    pub defaults: Config,
    pub schema: Json,
}
impl Snapshot {
    pub fn load(source: Option<&str>) -> Result<Self, SettingsError> {
        let text = source.unwrap_or("");
        let config =
            Config::load(text).map_err(|e| SettingsError::config(e, ErrorCode::InvalidConfig))?;
        let overlay: toml::Value = toml::from_str(text).map_err(|e: toml::de::Error| {
            SettingsError::new(
                ErrorCode::InvalidConfig,
                e.to_string(),
                "Correct the TOML before saving.",
            )
        })?;
        Ok(Self {
            source: source.map(str::to_owned),
            config,
            overrides: serde_json::to_value(overlay)
                .expect("TOML configuration is JSON-compatible"),
            defaults: Config::default(),
            schema: config::schema(),
        })
    }
}
#[derive(Clone, Debug)]
pub struct PreparedUpdate {
    pub source: String,
    pub config: Config,
    pub changed: bool,
}

/// Prepare an all-or-nothing batch. The host must recheck the token under its
/// file lock immediately before replacing the file; this function does no I/O.
pub fn prepare_update(
    current: Option<&str>,
    expected: Option<&str>,
    edits: &[Edit],
) -> Result<PreparedUpdate, SettingsError> {
    if current != expected {
        return Err(SettingsError::conflict());
    }
    let snapshot = Snapshot::load(current)?;
    if edits.len() > 1024 {
        return Err(SettingsError::new(
            ErrorCode::InvalidValue,
            "Too many settings in one update.",
            "Save at most 1024 edits at a time.",
        ));
    }
    let text = current.unwrap_or("");
    let mut document = text.parse::<DocumentMut>().map_err(|e| {
        SettingsError::new(
            ErrorCode::InvalidConfig,
            e.to_string(),
            "Correct the TOML before saving.",
        )
    })?;
    let defaults = serde_json::to_value(
        toml::from_str::<toml::Value>(config::DEFAULT_CONFIG).expect("compiled defaults parse"),
    )
    .expect("defaults serialize");
    for edit in edits {
        let (path, value) = match edit {
            Edit::Set { path, value } => (path, Some(value)),
            Edit::Reset { path } => (path, None),
        };
        let field_schema = path_schema(&snapshot.schema, path)?;
        if value.is_none() && matches!(path.last(), Some(PathSegment::Index(_))) {
            return Err(invalid_path(
                "Reset a field or the entire list; remove a row by replacing the list.",
            )
            .at(path));
        }
        let normalized = value
            .map(|v| normalize(v, &snapshot.schema, field_schema))
            .transpose()
            .map_err(|e| e.at(path))?;
        edit_item(document.as_item_mut(), path, normalized.as_ref(), &defaults, true)
            .map_err(|e| e.at(path))?;
    }
    let rendered = document.to_string();
    let config =
        Config::load(&rendered).map_err(|e| SettingsError::config(e, ErrorCode::Validation))?;
    let overlay: toml::Value = toml::from_str(&rendered).expect("toml_edit renders valid TOML");
    let unchanged = serde_json::to_value(overlay).expect("TOML serializes") == snapshot.overrides;
    // Preserve byte-for-byte identity on no-op saves, including final newline.
    let source = if unchanged { text.to_owned() } else { preserve_line_endings(rendered, text) };
    if source.len() > MAX_SETTINGS_BYTES {
        return Err(SettingsError::new(
            ErrorCode::Validation,
            "The updated configuration exceeds 1 MiB.",
            "Reduce the configuration size before saving. The existing file has not been changed.",
        ));
    }
    Ok(PreparedUpdate { changed: source != text, source, config })
}

fn invalid_path(message: impl Into<String>) -> SettingsError {
    SettingsError::new(
        ErrorCode::InvalidPath,
        message,
        "Reload settings and select an existing setting or row.",
    )
}
fn resolved<'a>(root: &'a Json, mut node: &'a Json) -> &'a Json {
    for _ in 0..16 {
        if let Some(reference) = node.get("$ref").and_then(Json::as_str)
            && let Some(next) = reference.strip_prefix('#').and_then(|p| root.pointer(p))
        {
            node = next;
            continue;
        }
        if let Some(variants) = node.get("anyOf").and_then(Json::as_array)
            && let Some(next) =
                variants.iter().find(|v| v.get("type").and_then(Json::as_str) != Some("null"))
        {
            node = next;
            continue;
        }
        break;
    }
    node
}
fn child_schema<'a>(root: &'a Json, node: &'a Json, part: &PathSegment) -> Option<&'a Json> {
    let node = resolved(root, node);
    match part {
        PathSegment::Key(key) => node
            .get("properties")
            .and_then(|p| p.get(key))
            .or_else(|| node.get("additionalProperties").filter(|v| v.is_object())),
        PathSegment::Index(_) => node.get("items"),
    }
}
fn path_schema<'a>(root: &'a Json, path: &[PathSegment]) -> Result<&'a Json, SettingsError> {
    if path.is_empty()
        || path.len() > 16
        || path.iter().any(|p| matches!(p, PathSegment::Key(k) if k.len() > 512))
    {
        return Err(invalid_path("The settings path is empty or too long.").at(path));
    }
    if matches!(path.first(), Some(PathSegment::Key(k)) if k == "config-version") {
        return Err(invalid_path(
            "The configuration version is read-only; use migration to change it.",
        )
        .at(path));
    }
    let mut node = root;
    for part in path {
        node = child_schema(root, node, part)
            .ok_or_else(|| invalid_path("Unknown settings field or invalid path.").at(path))?;
    }
    Ok(node)
}
fn allows_null(root: &Json, node: &Json) -> bool {
    node.get("type")
        .is_some_and(|t| t == "null" || t.as_array().is_some_and(|a| a.iter().any(|t| t == "null")))
        || node
            .get("anyOf")
            .and_then(Json::as_array)
            .is_some_and(|a| a.iter().any(|v| allows_null(root, v)))
        || node
            .get("$ref")
            .and_then(Json::as_str)
            .and_then(|r| r.strip_prefix('#'))
            .and_then(|p| root.pointer(p))
            .is_some_and(|v| allows_null(root, v))
}
fn normalize(value: &Json, root: &Json, node: &Json) -> Result<Json, SettingsError> {
    let error = || {
        SettingsError::new(
            ErrorCode::InvalidValue,
            "This setting cannot contain null or an unknown field.",
            "Use Reset to remove an override; provide all required fields for list rows.",
        )
    };
    match value {
        Json::Null => Err(error()),
        Json::Object(object) => {
            let mut result = serde_json::Map::new();
            for (key, value) in object {
                let child =
                    child_schema(root, node, &PathSegment::Key(key.clone())).ok_or_else(error)?;
                if value.is_null() && allows_null(root, child) {
                    continue;
                }
                result.insert(key.clone(), normalize(value, root, child)?);
            }
            Ok(Json::Object(result))
        }
        Json::Array(values) => {
            let child = child_schema(root, node, &PathSegment::Index(0)).ok_or_else(error)?;
            values
                .iter()
                .map(|v| normalize(v, root, child))
                .collect::<Result<Vec<_>, _>>()
                .map(Json::Array)
        }
        _ => Ok(value.clone()),
    }
}
fn json_item(value: &Json) -> Result<Item, SettingsError> {
    fn convert(value: &Json) -> Option<Value> {
        Some(match value {
            Json::Bool(v) => Value::from(*v),
            Json::String(v) => Value::from(v.as_str()),
            Json::Number(v) => {
                if let Some(n) = v.as_i64() {
                    Value::from(n)
                } else if v.is_f64() {
                    Value::from(v.as_f64().filter(|n| n.is_finite())?)
                } else {
                    return None;
                }
            }
            Json::Array(values) => {
                Value::Array(values.iter().map(convert).collect::<Option<toml_edit::Array>>()?)
            }
            Json::Object(values) => Value::InlineTable(
                values
                    .iter()
                    .map(|(k, v)| Some((k.clone(), convert(v)?)))
                    .collect::<Option<toml_edit::InlineTable>>()?,
            ),
            Json::Null => return None,
        })
    }
    convert(value).map(Item::Value).ok_or_else(|| {
        SettingsError::new(
            ErrorCode::InvalidValue,
            "The value cannot be represented as TOML.",
            "Use a finite number, text, boolean, list or object.",
        )
    })
}
fn item_json(item: &Item) -> Option<Json> {
    if let Some(table) = item.as_table_like() {
        return Some(Json::Object(
            table
                .iter()
                .map(|(k, v)| Some((k.to_owned(), item_json(v)?)))
                .collect::<Option<serde_json::Map<_, _>>>()?,
        ));
    }
    if let Some(length) =
        item.as_array().map(|a| a.len()).or_else(|| item.as_array_of_tables().map(|a| a.len()))
    {
        return (0..length)
            .map(|i| item_json(item.get(i)?))
            .collect::<Option<Vec<_>>>()
            .map(Json::Array);
    }
    if let Some(v) = item.as_str() {
        return Some(Json::String(v.into()));
    }
    if let Some(v) = item.as_bool() {
        return Some(Json::Bool(v));
    }
    if let Some(v) = item.as_integer() {
        return Some(Json::from(v));
    }
    item.as_float().and_then(serde_json::Number::from_f64).map(Json::Number)
}
fn assign(item: &mut Item, value: &Json) -> Result<(), SettingsError> {
    if item_json(item).as_ref() == Some(value) {
        return Ok(());
    }
    if let (Some(table), Json::Object(values)) = (item.as_table_like_mut(), value) {
        let removed: Vec<_> = table
            .iter()
            .filter(|(k, _)| !values.contains_key(*k))
            .map(|(k, _)| k.to_owned())
            .collect();
        for key in removed {
            table.remove(&key);
        }
        for (key, value) in values {
            assign(table.entry(key).or_insert(Item::None), value)?;
        }
        return Ok(());
    }
    if let Json::Array(values) = value {
        let length =
            item.as_array().map(|a| a.len()).or_else(|| item.as_array_of_tables().map(|a| a.len()));
        if let Some(length) = length {
            if item.is_array_of_tables() && !values.iter().all(Json::is_object) {
                return Err(SettingsError::new(
                    ErrorCode::InvalidValue,
                    "This list requires objects.",
                    "Keep one object per row.",
                ));
            }
            // Workspace/monitor names and widget ids identify rows across UI
            // reordering, so their comments travel with the corresponding row.
            let stable_key = ["id", "name"].into_iter().find(|key| {
                !values.is_empty()
                    && values.iter().all(|v| v.get(*key).is_some_and(Json::is_string))
                    && (0..length).all(|i| {
                        item.get(i).and_then(|v| v.get(*key)).and_then(Item::as_str).is_some()
                    })
            });
            if let Some(key) = stable_key {
                let old: Vec<_> = (0..length).map(|i| item.get(i).unwrap().clone()).collect();
                let mut used = vec![false; length];
                let mut rows = Vec::new();
                for (index, value) in values.iter().enumerate() {
                    let identity = value[key].as_str().unwrap();
                    let existing = old
                        .iter()
                        .enumerate()
                        .position(|(i, row)| {
                            !used[i] && row.get(key).and_then(Item::as_str) == Some(identity)
                        })
                        .or_else(|| {
                            (index < old.len()
                                && !used[index]
                                && !values.iter().any(|v| {
                                    v[key].as_str() == old[index].get(key).and_then(Item::as_str)
                                }))
                            .then_some(index)
                        });
                    let mut row = if let Some(i) = existing {
                        used[i] = true;
                        old[i].clone()
                    } else if item.is_array_of_tables() {
                        Item::Table(Table::new())
                    } else {
                        Item::None
                    };
                    assign(&mut row, value)?;
                    if let Some(table) = row.as_table_mut() {
                        table.set_position(
                            old.get(index).and_then(Item::as_table).and_then(Table::position),
                        );
                    }
                    rows.push(row);
                }
                if let Some(array) = item.as_array_mut() {
                    array.clear();
                    for row in rows {
                        array.push_formatted(row.into_value().expect("array row is a value"));
                    }
                } else if let Some(array) = item.as_array_of_tables_mut() {
                    array.clear();
                    for row in rows {
                        array.push(row.into_table().expect("table row stays a table"));
                    }
                }
                return Ok(());
            }
            for (index, value) in values.iter().take(length).enumerate() {
                assign(item.get_mut(index).expect("existing array index"), value)?;
            }
            if let Some(array) = item.as_array_mut() {
                while array.len() > values.len() {
                    array.remove(array.len() - 1);
                }
                for value in values.iter().skip(length) {
                    array.push(json_item(value)?.into_value().expect("JSON item is a value"));
                }
            } else if let Some(array) = item.as_array_of_tables_mut() {
                while array.len() > values.len() {
                    array.remove(array.len() - 1);
                }
                for value in values.iter().skip(length) {
                    let mut table = Item::Table(Table::new());
                    assign(&mut table, value)?;
                    array.push(table.into_table().expect("table stays a table"));
                }
            }
            return Ok(());
        }
    }
    let mut replacement = json_item(value)?;
    if let (Some(old), Some(new)) = (item.as_value(), replacement.as_value_mut()) {
        *new.decor_mut() = old.decor().clone();
    }
    *item = replacement;
    Ok(())
}
fn edit_item(
    item: &mut Item,
    path: &[PathSegment],
    value: Option<&Json>,
    defaults: &Json,
    root: bool,
) -> Result<(), SettingsError> {
    let (part, remaining) =
        path.split_first().ok_or_else(|| invalid_path("A field is required."))?;
    match part {
        PathSegment::Key(key) => {
            let default = if root && key == "rules" {
                &Json::Null
            } else {
                defaults.get(key).unwrap_or(&Json::Null)
            };
            let inline = item.is_inline_table();
            let table = item
                .as_table_like_mut()
                .ok_or_else(|| invalid_path("A parent setting is not an object."))?;
            if remaining.is_empty() {
                if let Some(value) = value {
                    assign(table.entry(key).or_insert(Item::None), value)?;
                } else {
                    table.remove(key);
                }
                return Ok(());
            }
            if !table.contains_key(key) {
                if value.is_none() {
                    if let Some(PathSegment::Index(index)) = remaining.first()
                        && default.get(*index).is_none()
                    {
                        return Err(invalid_path("The requested row does not exist."));
                    }
                    return Ok(());
                }
                let child = if matches!(remaining.first(), Some(PathSegment::Index(_))) {
                    if !default.is_array() {
                        return Err(invalid_path("The requested personal row does not exist."));
                    }
                    json_item(default)?
                } else if inline {
                    Item::Value(Value::InlineTable(toml_edit::InlineTable::new()))
                } else {
                    let mut table = Table::new();
                    table.set_implicit(true);
                    Item::Table(table)
                };
                table.insert(key, child);
            }
            edit_item(table.get_mut(key).expect("inserted child"), remaining, value, default, false)
        }
        PathSegment::Index(index) => {
            let child = item
                .get_mut(*index)
                .ok_or_else(|| invalid_path("The requested row does not exist."))?;
            if remaining.is_empty() {
                if let Some(value) = value {
                    assign(child, value)
                } else {
                    Err(invalid_path(
                        "Reset a field or the entire list; remove a row by replacing the list.",
                    ))
                }
            } else {
                edit_item(
                    child,
                    remaining,
                    value,
                    defaults.get(*index).unwrap_or(&Json::Null),
                    false,
                )
            }
        }
    }
}
fn preserve_line_endings(rendered: String, original: &str) -> String {
    // Restore CRLF only when it is the document's uniform newline style.
    if original.contains("\r\n") && !original.replace("\r\n", "").contains('\n') {
        rendered.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn edit(path: Json, value: Json) -> Edit {
        serde_json::from_value(json!({"op":"set","path":path,"value":value})).unwrap()
    }
    fn reset(path: Json) -> Edit {
        serde_json::from_value(json!({"op":"reset","path":path})).unwrap()
    }
    #[test]
    fn scalar_edit_preserves_comments_personal_overrides_and_crlf() {
        let source = "# my preferences\r\ntheme = 'dark' # keep this\r\n[bindings]\r\n'alt+h' = ['focus right'] # personal shortcut\r\n[gaps]\r\ninner = 24\r\nouter = 40 # breathing room\r\n";
        let updated = prepare_update(
            Some(source),
            Some(source),
            &[edit(json!(["gaps", "inner"]), json!(30))],
        )
        .unwrap();
        assert_eq!(updated.config.gaps.inner, 30);
        assert_eq!(updated.config.gaps.outer, 40);
        assert_eq!(updated.config.bindings["alt+h"], ["focus right"]);
        assert!(updated.source.contains("theme = 'dark' # keep this\r\n"));
        assert!(updated.source.contains("outer = 40 # breathing room\r\n"));
        assert!(updated.source.contains("# personal shortcut\r\n"));
        assert!(!updated.source.replace("\r\n", "").contains('\n'));
    }
    #[test]
    fn reset_inherits_defaults_and_preserves_other_explicit_fields() {
        let source =
            "[gaps]\ninner = 24\nouter = 40 # retain\n[bindings]\n'alt+h' = ['focus right']\n";
        let updated = prepare_update(
            Some(source),
            Some(source),
            &[reset(json!(["gaps", "inner"])), reset(json!(["bindings", "alt+h"]))],
        )
        .unwrap();
        assert_eq!(updated.config.gaps.inner, Config::default().gaps.inner);
        assert_eq!(updated.config.gaps.outer, 40);
        assert_eq!(updated.config.bindings["alt+h"], ["focus left"]);
        assert!(updated.source.contains("outer = 40 # retain"));
    }
    #[test]
    fn no_op_edits_preserve_exact_bytes_and_missing_file_identity() {
        let source = "theme = 'dark' # no final newline";
        let updated =
            prepare_update(Some(source), Some(source), &[edit(json!(["theme"]), json!("dark"))])
                .unwrap();
        assert!(!updated.changed);
        assert_eq!(updated.source, source);
        assert!(!prepare_update(None, None, &[reset(json!(["theme"]))]).unwrap().changed);
        assert_eq!(prepare_update(None, Some(""), &[]).unwrap_err().code, ErrorCode::Conflict);
    }
    #[test]
    fn editing_an_inherited_row_materializes_only_that_replacement_list() {
        let updated =
            prepare_update(None, None, &[edit(json!(["workspaces", 2, "icon"]), json!("code"))])
                .unwrap();
        assert_eq!(updated.config.workspaces.len(), 10);
        assert_eq!(updated.config.workspaces[2].icon.as_deref(), Some("code"));
        assert_eq!(updated.config.workspaces[1], Config::default().workspaces[1]);
        let snapshot = Snapshot::load(Some(&updated.source)).unwrap();
        assert!(snapshot.overrides.get("bindings").is_none());
        assert_eq!(snapshot.overrides.as_object().unwrap().len(), 1);
    }
    #[test]
    fn nested_inline_row_and_array_of_tables_edits_preserve_row_comments() {
        let source = "[[workspaces]] # work\nname = 'work' # first row\nicon = 'old'\n\n[[workspaces]] # play\nname = 'play'\n";
        let updated = prepare_update(
            Some(source),
            Some(source),
            &[edit(json!(["workspaces", 0, "icon"]), json!("new"))],
        )
        .unwrap();
        assert!(updated.source.contains("[[workspaces]] # work"));
        assert!(updated.source.contains("name = 'work' # first row"));
        assert!(updated.source.contains("[[workspaces]] # play"));
        let updated = prepare_update(
            None,
            None,
            &[
                edit(json!(["workspaces", 0, "gaps", "inner"]), json!(24)),
                edit(json!(["workspaces", 0, "gaps", "outer"]), json!(48)),
            ],
        )
        .unwrap();
        assert_eq!(updated.config.workspaces[0].gaps.as_ref().unwrap().outer, 48);
    }
    #[test]
    fn personal_rules_are_distinct_and_reset_cannot_remove_built_ins() {
        let source = "[[rules]] # personal\ncommands=['set-floating']\nmatches=[{process={equals='Fixture'}}]\n";
        let snapshot = Snapshot::load(Some(source)).unwrap();
        assert_eq!(snapshot.overrides["rules"].as_array().unwrap().len(), 1);
        assert_eq!(snapshot.config.rules.len(), snapshot.defaults.rules.len() + 1);
        let updated = prepare_update(
            Some(source),
            Some(source),
            &[edit(json!(["rules", 0, "commands"]), json!(["ignore"]))],
        )
        .unwrap();
        assert_eq!(updated.config.rules.last().unwrap().commands, ["ignore"]);
        assert!(updated.source.contains("# personal"));
        let reset = prepare_update(Some(source), Some(source), &[reset(json!(["rules"]))]).unwrap();
        assert_eq!(reset.config.rules, Config::default().rules);
        assert_eq!(
            prepare_update(None, None, &[edit(json!(["rules", 0, "commands"]), json!(["ignore"]))])
                .unwrap_err()
                .code,
            ErrorCode::InvalidPath
        );
    }
    #[test]
    fn reordered_named_rows_keep_their_own_comments() {
        let source = "[[workspaces]] # Work comment\nname = 'work'\n\n[[workspaces]] # Play comment\nname = 'play'\n";
        let updated = prepare_update(
            Some(source),
            Some(source),
            &[edit(json!(["workspaces"]), json!([{"name":"play"},{"name":"work"}]))],
        )
        .unwrap();
        assert_eq!(
            updated.config.workspaces.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
            ["play", "work"]
        );
        assert!(updated.source.contains("[[workspaces]] # Play comment\nname = 'play'"));
        assert!(updated.source.contains("[[workspaces]] # Work comment\nname = 'work'"));
        let inline =
            "workspaces = [\n  {name='work'}, # work row\n  {name='play'}, # play row\n]\n";
        let updated = prepare_update(
            Some(inline),
            Some(inline),
            &[edit(json!(["workspaces"]), json!([{"name":"play"},{"name":"work"}]))],
        )
        .unwrap();
        assert_eq!(updated.config.workspaces[0].name, "play");
        assert!(updated.source.contains("# work row"));
        assert!(updated.source.contains("# play row"));
    }
    #[test]
    fn optional_json_nulls_in_complete_rows_become_absent_toml_fields() {
        let mut rows = serde_json::to_value(Config::default().workspaces).unwrap();
        rows[0]["icon"] = json!("home");
        let updated = prepare_update(None, None, &[edit(json!(["workspaces"]), rows)]).unwrap();
        assert_eq!(updated.config.workspaces[0].icon.as_deref(), Some("home"));
        assert!(updated.config.workspaces[1].icon.is_none());
        assert!(
            prepare_update(None, None, &[edit(json!(["bar"]), json!({"height":null}))]).is_err()
        );
    }
    #[test]
    fn batch_validation_allows_interdependent_changes_without_accepting_bad_files() {
        let edits =
            [edit(json!(["gaps", "inner"]), json!(2)), edit(json!(["borders", "width"]), json!(2))];
        assert!(prepare_update(None, None, &edits[..1]).is_err());
        assert_eq!(prepare_update(None, None, &edits).unwrap().config.gaps.inner, 2);
        let invalid = "[bar\n";
        let error = prepare_update(Some(invalid), Some(invalid), &edits).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidConfig);
        assert!(error.line.is_some());
        assert_eq!(
            prepare_update(Some("config-version=0"), Some("config-version=0"), &[])
                .unwrap_err()
                .code,
            ErrorCode::InvalidConfig
        );
    }
    #[test]
    fn unknown_fields_and_impossible_paths_are_rejected_even_for_reset() {
        for path in [
            json!(["gaps", "innre"]),
            json!(["bar", "height", 0]),
            json!(["workspaces", 99, "icon"]),
            json!(["config-version"]),
            json!([]),
        ] {
            assert_eq!(
                prepare_update(None, None, &[reset(path)]).unwrap_err().code,
                ErrorCode::InvalidPath
            );
        }
        assert_eq!(
            prepare_update(
                None,
                None,
                &[edit(json!(["workspaces", 99, "name"]), json!("missing"))]
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidPath
        );
        assert_eq!(
            prepare_update(None, None, &[reset(json!(["workspaces", 0]))]).unwrap_err().code,
            ErrorCode::InvalidPath
        );
    }
    #[test]
    fn new_modes_and_weather_are_validated_as_one_atomic_batch() {
        let updated = prepare_update(
            None,
            None,
            &[
                edit(json!(["modes", "resize", "h"]), json!(["resize width -2%"])),
                edit(json!(["bindings", "alt+r"]), json!(["mode resize"])),
                edit(json!(["weather", "latitude"]), json!(10.0)),
                edit(json!(["weather", "longitude"]), json!(20.0)),
            ],
        )
        .unwrap();
        assert_eq!(updated.config.modes["resize"]["h"], ["resize width -2%"]);
        assert_eq!(updated.config.weather.unwrap().longitude, 20.0);
    }
}
