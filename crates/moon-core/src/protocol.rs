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
        let allowed: &[&str] = match object.get("method").and_then(|m| m.as_str()) {
            Some("query") => &["version", "id", "method", "query"],
            Some("command") => &["version", "id", "method", "commands", "expected_focus"],
            Some("subscribe") => &["version", "id", "method"],
            Some("lifecycle") => &["version", "id", "method", "action"],
            _ => return Err("method must be query, command, subscribe or lifecycle".into()),
        };
        if let Some(key) = object.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!("unknown request field {key:?}"));
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
