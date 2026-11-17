//! Describe observed configuration state without claiming native window effects completed.
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Status {
    pub state: &'static str,
    pub message: String,
}

pub fn describe(expected: &Value, actual: Result<Value, String>, stats: &Value) -> Status {
    let actual = match actual {
        Ok(actual) => actual,
        Err(error) => {
            return Status {
                state: "unavailable",
                message: format!(
                    "Saved on this PC. Moon could not confirm the loaded settings: {error}"
                ),
            };
        }
    };
    let loaded = actual == *expected;
    let mut result = if loaded {
        Status { state: "loaded", message: "Saved and loaded by Moon.".into() }
    } else {
        Status {
            state: "pending",
            message: "Saved on this PC. Moon is using different settings.".into(),
        }
    };
    if stats["paused"] == true {
        result.state = "paused";
        result.message.push_str(" Moon is paused; window arrangement is paused.");
    }
    if stats["session_suspended"] == true {
        result.state = "suspended";
        result
            .message
            .push_str(" Desktop updates are suspended while this session is unavailable.");
    }
    if let Some(error) = stats["last_error"].as_str().filter(|error| !error.is_empty()) {
        result.state = "failed";
        result.message.push_str(" Latest Moon error: ");
        result.message.push_str(error);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matching_configuration_reports_loaded_not_completed_effects() {
        let expected = json!({"theme":"light", "gaps":{"inner":16}});
        let status = describe(&expected, Ok(expected.clone()), &json!({}));
        assert_eq!(status.state, "loaded");
        assert_eq!(status.message, "Saved and loaded by Moon.");
        let error = describe(
            &expected,
            Ok(expected.clone()),
            &json!({"last_error":"Window placement failed"}),
        );
        assert_eq!(error.state, "failed");
        assert!(error.message.starts_with("Saved and loaded by Moon."));
        assert!(error.message.ends_with("Latest Moon error: Window placement failed"));
    }

    #[test]
    fn matching_configuration_preserves_pause_and_session_notices() {
        let expected = json!({"theme":"light"});
        for (stats, state, notice) in [
            (json!({"paused":true}), "paused", "Moon is paused"),
            (json!({"session_suspended":true}), "suspended", "Desktop updates are suspended"),
        ] {
            let status = describe(&expected, Ok(expected.clone()), &stats);
            assert_eq!(status.state, state);
            assert!(status.message.starts_with("Saved and loaded by Moon."));
            assert!(status.message.contains(notice));
        }
        let status = describe(
            &expected,
            Ok(expected.clone()),
            &json!({"paused":true,"session_suspended":true,"last_error":"placement failed"}),
        );
        assert_eq!(status.state, "failed");
        for notice in ["Moon is paused", "Desktop updates are suspended", "placement failed"] {
            assert!(status.message.contains(notice));
        }
    }

    #[test]
    fn different_or_unavailable_configuration_never_reports_loaded() {
        let expected = json!({"gaps":{"inner":16}});
        let status = describe(&expected, Ok(json!({"gaps":{"inner":17}})), &json!({}));
        assert_eq!(status.state, "pending");
        assert!(status.message.contains("different settings"));
        let status = describe(&expected, Err("Connection closed".into()), &json!({}));
        assert_eq!(status.state, "unavailable");
        assert!(status.message.contains("Connection closed"));
    }
}
