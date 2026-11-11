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
    },
    Subscribe {},
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
            Some("command") => &["version", "id", "method", "commands"],
            Some("subscribe") => &["version", "id", "method"],
            _ => return Err("method must be query, command or subscribe".into()),
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
        if let RequestAction::Command { commands } = &request.action
            && (commands.is_empty() || commands.len() > 128)
        {
            return Err("request must contain 1–128 commands".into());
        }
        Ok(request)
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
    }
}
