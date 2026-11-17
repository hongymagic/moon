//! The Settings bridge exposes a small typed action surface, never arbitrary CLI text.
use moon_core::protocol::Request;
use serde_json::{Value, json};

pub fn request(method: &str, params: &Value) -> Result<Value, String> {
    let (ipc_method, allowed): (&str, &[&str]) = match method {
        "window-action" => ("window", &["generation", "revision", "target", "action"]),
        "layout-undo" => ("undo", &["generation", "revision"]),
        "desktop-action" => ("control", &["generation", "revision", "action"]),
        _ => return Err("This desktop action is not supported.".into()),
    };
    let params = params.as_object().ok_or("Desktop action parameters must be an object.")?;
    if let Some(key) = params.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("Unknown desktop action field: {key}."));
    }
    let mut value = json!({"version":1,"id":"settings","method":ipc_method});
    value.as_object_mut().unwrap().extend(params.clone());
    Request::parse(&value.to_string())?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_cannot_override_the_protocol_or_inject_commands() {
        for params in [
            json!({"method":"lifecycle","action":"exit"}),
            json!({"generation":"a","revision":"1","commands":["exec anything"]}),
            json!({"version":2,"id":"replacement"}),
            Value::Null,
        ] {
            assert!(request("window-action", &params).is_err());
        }
        assert!(request("exec", &json!({})).is_err());
    }
}
