//! Typed local launch options. A page is data, never a navigable URL or script.
use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Startup {
    pub config: Option<PathBuf>,
    pub page: Page,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    General,
    Shortcuts,
}
impl Page {
    pub fn name(&self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Shortcuts => "Shortcuts",
        }
    }
}
impl Startup {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut startup = Self::default();
        let mut page_seen = false;
        let mut args = args.into_iter();
        while let Some(argument) = args.next() {
            match argument.to_str() {
                Some("--config") if startup.config.is_none() => {
                    let path = args.next().ok_or("--config needs a file path.")?;
                    if path.is_empty() {
                        return Err("--config needs a file path.".into());
                    }
                    startup.config = Some(PathBuf::from(path));
                }
                Some("--page") if !page_seen => {
                    startup.page = match args.next().as_deref().and_then(|value| value.to_str()) {
                        Some("general") => Page::General,
                        Some("shortcuts") => Page::Shortcuts,
                        _ => return Err("--page must be general or shortcuts.".into()),
                    };
                    page_seen = true;
                }
                _ => {
                    return Err(
                        "Use each option once: --config <file> and --page general|shortcuts."
                            .into(),
                    );
                }
            }
        }
        Ok(startup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Startup, String> {
        Startup::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn default_and_shortcuts_launch_preserve_paths_in_either_order() {
        assert_eq!(parse(&[]).unwrap(), Startup::default());
        for args in [
            ["--config", "配置 with spaces.toml", "--page", "shortcuts"],
            ["--page", "shortcuts", "--config", "配置 with spaces.toml"],
        ] {
            let startup = parse(&args).unwrap();
            assert_eq!(startup.config, Some(PathBuf::from("配置 with spaces.toml")));
            assert_eq!(startup.page.name(), "Shortcuts");
        }
        assert_eq!(parse(&["--page", "general"]).unwrap().page.name(), "General");
    }

    #[test]
    fn launch_rejects_unknown_duplicate_or_incomplete_options() {
        for args in [
            vec!["--page"],
            vec!["--config"],
            vec!["--config", ""],
            vec!["--page", "https://example.com"],
            vec!["--page", "shortcuts", "--page", "general"],
            vec!["--config", "one", "--config", "two"],
            vec!["--unknown"],
        ] {
            assert!(parse(&args).is_err(), "accepted {args:?}");
        }
    }
}
