//! Typed, local-only profile and theme operations for the Settings host.
use crate::library::{Kind, Library};
use moon_core::config::Config;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

#[derive(Deserialize)]
#[serde(tag = "method", content = "params", deny_unknown_fields)]
enum Call {
    #[serde(rename = "library-list")]
    List {},
    #[serde(rename = "library-save")]
    Save {
        expected_source: Option<String>,
        kind: Kind,
        name: String,
        config: Config,
        replace: bool,
    },
    #[serde(rename = "library-rename")]
    Rename { expected_source: Option<String>, kind: Kind, name: String, new_name: String },
    #[serde(rename = "library-delete")]
    Delete { expected_source: Option<String>, kind: Kind, name: String },
    #[serde(rename = "library-import")]
    Import { expected_source: Option<String>, document: String, replace: bool },
    #[serde(rename = "library-export")]
    Export { kind: Kind, name: String },
    #[serde(rename = "library-apply")]
    Apply { kind: Kind, name: String, config: Config },
}

pub fn handle(method: &str, params: &Value, path: &Path) -> Result<Value, Value> {
    let call: Call = serde_json::from_value(json!({"method":method,"params":params}))
        .map_err(|error| json!({"message":error.to_string()}))?;
    let library = Library::new(path.to_owned());
    let catalog = match call {
        Call::List {} => library.load(),
        Call::Save { expected_source, kind, name, config, replace } => {
            library.save(expected_source.as_deref(), kind, &name, &config, replace)
        }
        Call::Rename { expected_source, kind, name, new_name } => {
            library.rename(expected_source.as_deref(), kind, &name, &new_name)
        }
        Call::Delete { expected_source, kind, name } => {
            library.delete(expected_source.as_deref(), kind, &name)
        }
        Call::Import { expected_source, document, replace } => {
            library.import(expected_source.as_deref(), &document, replace)
        }
        Call::Export { kind, name } => {
            return library
                .export(kind, &name)
                .map(|document| json!({"document":document}))
                .map_err(|error| json!(error));
        }
        Call::Apply { kind, name, config } => {
            return library
                .apply(kind, &name, &config)
                .map(|edits| json!({"edits":edits}))
                .map_err(|error| json!(error));
        }
    }
    .map_err(|error| json!(error))?;
    serde_json::to_value(catalog).map_err(|error| json!({"message":error.to_string()}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_bridge_requests_cannot_choose_paths_or_execute_commands() {
        let path = Path::new("must-not-be-created/config.toml");
        for (method, params) in [
            ("library-list", json!({"path":"elsewhere"})),
            ("library-export", json!({"kind":"theme","name":"Linen","command":"exec"})),
            ("library-delete", json!({"kind":"script","name":"x","expected_source":null})),
            ("library-unknown", json!({})),
        ] {
            assert!(handle(method, &params, path).is_err());
        }
        assert!(!path.parent().unwrap().exists());
    }
}
