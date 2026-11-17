//! Public v1 NDJSON protocol. Transport security lives in moon-win.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_MESSAGE_BYTES: usize = 65_536;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    pub version: u32,
    pub id: serde_json::Value,
    #[serde(flatten)]
    pub action: RequestAction,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", rename_all = "kebab-case")]
pub enum RequestAction {
    Query {
        #[serde(default = "world")]
        query: String,
    },
    Command {
        commands: Vec<String>,
        /// Reject the command if focus changed since an automation inspected it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_focus: Option<crate::ids::WindowId>,
    },
    Subscribe {},
    Lifecycle {
        action: LifecycleAction,
    },
    Window {
        generation: String,
        revision: String,
        target: WindowTarget,
        action: crate::command::WindowAction,
    },
    Undo {
        generation: String,
        revision: String,
    },
    Control {
        generation: String,
        revision: String,
        action: DesktopControl,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WindowTarget {
    pub id: String,
    pub process_id: String,
    pub process_birth: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopControl {
    Pause,
    Reload,
    Restore,
}

pub fn decimal(value: &str) -> Result<u64, String> {
    let number =
        value.parse::<u64>().map_err(|_| "Expected an unsigned decimal identity string")?;
    if number.to_string() != value {
        return Err("Identity strings must use canonical decimal notation".into());
    }
    Ok(number)
}
pub fn check_generation(
    generation: &str,
    revision: &str,
    current_generation: &str,
    current_revision: u64,
) -> Result<(), String> {
    if generation != current_generation || decimal(revision)? != current_revision {
        return Err("Desktop changed; refresh the overview and try again".into());
    }
    Ok(())
}
impl WindowTarget {
    pub fn numbers(&self) -> Result<(crate::ids::WindowId, u32, u64), String> {
        let id = decimal(&self.id)?;
        let process =
            u32::try_from(decimal(&self.process_id)?).map_err(|_| "Process ID exceeds u32")?;
        let birth = decimal(&self.process_birth)?;
        if id == 0 || process == 0 || birth == 0 {
            return Err("Window and process identities must be nonzero".into());
        }
        Ok((crate::ids::WindowId(id), process, birth))
    }
    pub fn check(
        &self,
        world: &crate::world::World,
        recorded_birth: u64,
        native_process: u32,
        native_birth: u64,
    ) -> Result<crate::ids::WindowId, String> {
        let (id, process, birth) = self.numbers()?;
        if !world.windows.get(&id).is_some_and(|window| window.info.process_id == process)
            || birth != recorded_birth
            || process != native_process
            || birth != native_birth
        {
            return Err("Window identity changed; refresh the overview and try again".into());
        }
        Ok(id)
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LifecycleAction {
    Restart,
    Exit,
}
fn world() -> String {
    "world".into()
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    pub version: u32,
    pub id: serde_json::Value,
    pub ok: bool,
    pub result: serde_json::Value,
    pub error: Option<String>,
    pub revision: u64,
}
impl Request {
    pub fn parse(line: &str) -> Result<Self, String> {
        if line.len() > MAX_MESSAGE_BYTES {
            return Err("IPC request exceeds 64 KiB".into());
        }
        // Flatten and deny_unknown_fields are incompatible in serde. Validate the envelope explicitly.
        let object: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let object = object.as_object().ok_or("request must be an object")?;
        let allowed: &[&str] =
            match object.get("method").and_then(|m| m.as_str()) {
                Some("query") => &["version", "id", "method", "query"],
                Some("command") => &["version", "id", "method", "commands", "expected_focus"],
                Some("subscribe") => &["version", "id", "method"],
                Some("lifecycle") => &["version", "id", "method", "action"],
                Some("window") => {
                    &["version", "id", "method", "generation", "revision", "target", "action"]
                }
                Some("undo") => &["version", "id", "method", "generation", "revision"],
                Some("control") => &["version", "id", "method", "generation", "revision", "action"],
                _ => return Err(
                    "method must be query, command, subscribe, lifecycle, window, undo or control"
                        .into(),
                ),
            };
        if let Some(key) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!("unknown request field {key:?}"));
        }
        // Serde's internally tagged unit variants accept extra fields even with
        // deny_unknown_fields. Keep the action capability boundary explicit.
        if object.get("method").and_then(|m| m.as_str()) == Some("window") {
            let action = object
                .get("action")
                .and_then(|a| a.as_object())
                .ok_or("Window action must be an object")?;
            let fields: &[&str] = match action.get("type").and_then(|t| t.as_str()) {
                Some("focus" | "float" | "tile") => &["type"],
                Some("move-to-workspace") => &["type", "workspace", "follow"],
                _ => {
                    return Err(
                        "Window action must be focus, move-to-workspace, float or tile".into()
                    );
                }
            };
            if let Some(key) = action.keys().find(|key| !fields.contains(&key.as_str())) {
                return Err(format!("unknown window action field {key:?}"));
            }
        }
        let request: Self = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if request.version != PROTOCOL_VERSION {
            return Err(format!("unsupported protocol version {}; supported: 1", request.version));
        }
        if !request.id.is_null() && !request.id.is_string() && !request.id.is_number() {
            return Err("id must be a string, number or null".into());
        }
        if let RequestAction::Command { commands, .. } = &request.action
            && (commands.is_empty() || commands.len() > 128)
        {
            return Err("request must contain 1–128 commands".into());
        }
        match &request.action {
            RequestAction::Window { generation, revision, target, action } => {
                validate_guard(generation, revision)?;
                target.numbers()?;
                if let crate::command::WindowAction::MoveToWorkspace { workspace, .. } = action
                    && (workspace.trim().is_empty() || workspace.len() > 4096)
                {
                    return Err("Workspace name must contain 1–4096 bytes".into());
                }
            }
            RequestAction::Undo { generation, revision }
            | RequestAction::Control { generation, revision, .. } => {
                validate_guard(generation, revision)?
            }
            _ => {}
        }
        Ok(request)
    }

    pub fn check_focus(&self, focused: Option<crate::ids::WindowId>) -> Result<(), String> {
        if let RequestAction::Command { expected_focus: Some(expected), .. } = self.action
            && focused != Some(expected)
        {
            return Err("focus changed; guarded commands were not executed".into());
        }
        Ok(())
    }
}
fn validate_guard(generation: &str, revision: &str) -> Result<(), String> {
    let (process, birth) = generation.split_once(':').ok_or("Invalid daemon generation")?;
    if decimal(process)? == 0 || decimal(birth)? == 0 {
        return Err("Invalid daemon generation".into());
    }
    decimal(revision)?;
    Ok(())
}
impl Response {
    pub fn success(id: serde_json::Value, result: serde_json::Value, revision: u64) -> Self {
        Self { version: PROTOCOL_VERSION, id, ok: true, result, error: None, revision }
    }
    pub fn failure(id: serde_json::Value, error: impl Into<String>, revision: u64) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id,
            ok: false,
            result: serde_json::Value::Null,
            error: Some(error.into()),
            revision,
        }
    }
}
pub fn schema() -> serde_json::Value {
    serde_json::json!({"request":schemars::schema_for!(Request),"response":schemars::schema_for!(Response)})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_envelope() {
        assert!(Request::parse(r#"{"version":1,"id":1,"method":"query"}"#).is_ok());
        assert!(Request::parse(r#"{"version":2,"id":1,"method":"query"}"#).is_err());
        assert!(
            Request::parse(r#"{"version":1,"id":1,"method":"command","commands":[]}"#).is_err()
        );
        assert!(Request::parse(r#"{"version":1,"id":1,"method":"query","extra":true}"#).is_err());
        assert!(
            Request::parse(r#"{"version":1,"id":1,"method":"lifecycle","action":"restart"}"#)
                .is_ok()
        );
        assert!(
            Request::parse(r#"{"version":1,"id":1,"method":"lifecycle","action":"exec"}"#).is_err()
        );
    }

    #[test]
    fn guarded_command_rejects_stale_focus_without_affecting_ordinary_clients() {
        let guarded = Request::parse(
            r#"{"version":1,"id":1,"method":"command","commands":["close"],"expected_focus":42}"#,
        )
        .unwrap();
        assert!(guarded.check_focus(Some(crate::ids::WindowId(42))).is_ok());
        assert!(guarded.check_focus(Some(crate::ids::WindowId(43))).is_err());
        assert!(guarded.check_focus(None).is_err());
        let ordinary =
            Request::parse(r#"{"version":1,"id":1,"method":"command","commands":["close"]}"#)
                .unwrap();
        assert!(ordinary.check_focus(None).is_ok());
    }
}
