use anyhow::{Context, Result, bail};
use moon_core::config::{self, Config, DEFAULT_CONFIG};
use std::{fs, path::PathBuf};

#[cfg(windows)]
mod daemon;
#[cfg(windows)]
mod gaming;
#[cfg(any(windows, test))]
mod journal;
#[cfg(any(windows, test))]
mod workspace_shortcuts;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub config: Option<PathBuf>,
    pub only: Option<String>,
    pub hook: Option<bool>,
    pub no_surfaces: bool,
    pub surfaces: bool,
    pub takeover: bool,
    pub resume: bool,
    pub background: bool,
    pub internal_stdio: Option<String>,
    pub wait_parent: Option<u32>,
}

impl Options {
    #[cfg(any(windows, test))]
    pub fn hook_enabled(&self) -> bool {
        self.hook.unwrap_or(self.only.is_none())
    }
}

fn main() {
    #[cfg(windows)]
    let mut child_stdio = None;
    let result = (|| {
        let (options, words) = parse_args(std::env::args().skip(1).collect())?;
        #[cfg(windows)]
        if let Some(mode) = &options.internal_stdio {
            let path = journal::data_dir().join("moon.log");
            child_stdio = Some(
                moon_win::runtime::redirect_internal_stdio(
                    (mode == "log").then_some(path.as_path()),
                )
                .context("configure internal Moon standard streams")?,
            );
        }
        run(options, words)
    })();
    if let Err(error) = result {
        eprintln!("moon: {error:#}");
        std::process::exit(1);
    }
}

fn run(options: Options, words: Vec<String>) -> Result<()> {
    let first = words.first().map(String::as_str).unwrap_or("start");
    match first {
        "help" | "--help" | "-h" => print_help(),
        "version" | "--version" | "-V" => println!("moon {}", env!("MOON_VERSION")),
        "config" => config_command(&options, &words[1..])?,
        "protocol" if words.get(1).is_some_and(|word| word == "schema") => {
            println!("{}", serde_json::to_string_pretty(&moon_core::protocol::schema())?)
        }
        #[cfg(windows)]
        "start" => {
            if options.background {
                daemon::background(options)?;
            } else {
                daemon::run(options)?;
            }
        }
        #[cfg(windows)]
        "settings" => open_settings(&options)?,
        #[cfg(windows)]
        "overview" | "window-search" => open_settings_page(&options, Some("overview"))?,
        #[cfg(windows)]
        "doctor" => daemon::doctor(&options, &words[1..])?,
        #[cfg(windows)]
        "restore" => {
            let _com = moon_win::platform::Apartment::new()?;
            let _instance = moon_win::runtime::Instance::acquire()?;
            println!("Restored {} windows", journal::restore(&journal::default_path(), false)?);
        }
        #[cfg(windows)]
        "watchdog" => daemon::watchdog(&words[1..])?,
        #[cfg(windows)]
        "query" | "subscribe" => daemon::client(&words)?,
        #[cfg(windows)]
        _ => daemon::client(&words)?,
        #[cfg(not(windows))]
        _ => {
            bail!("desktop commands run on Windows 11; use `just run {}` from WSL", words.join(" "))
        }
    }
    Ok(())
}

fn parse_args(arguments: Vec<String>) -> Result<(Options, Vec<String>)> {
    let mut options = Options::default();
    let mut words = Vec::new();
    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        // Application arguments belong to the shell command, including option
        // names moon itself accepts. An explicit delimiter also ends parsing.
        if argument == "--" {
            words.extend(args);
            break;
        }
        if words.is_empty() && matches!(argument.as_str(), "exec" | "shell-exec") {
            words.push(argument);
            words.extend(args);
            break;
        }
        match argument.as_str() {
            "--config" => {
                let path = args.next().context("--config requires a path")?;
                options.config = Some(
                    std::path::absolute(&path)
                        .with_context(|| format!("resolve config path {path:?}"))?,
                );
            }
            "--only" => {
                options.only = Some(
                    args.next()
                        .context("--only requires a process name")?
                        .trim_end_matches(".exe")
                        .to_owned(),
                )
            }
            "--hook" => options.hook = Some(true),
            "--no-hook" => options.hook = Some(false),
            "--no-surfaces" => options.no_surfaces = true,
            "--surfaces" => options.surfaces = true,
            "--takeover" => options.takeover = true,
            "--resume" => options.resume = true,
            "--background" => options.background = true,
            "--internal-stdio" => {
                let value = args.next().context("--internal-stdio requires log or null")?;
                if !matches!(value.as_str(), "log" | "null") || options.internal_stdio.is_some() {
                    bail!("Invalid internal standard-stream mode");
                }
                options.internal_stdio = Some(value);
            }
            "--wait-parent" => {
                options.wait_parent =
                    Some(args.next().context("--wait-parent requires a PID")?.parse()?)
            }
            _ => words.push(argument),
        }
    }
    if options.internal_stdio.is_some()
        && (options.background
            || !matches!(words.first().map(String::as_str), Some("start" | "watchdog")))
    {
        bail!("Internal standard streams are only valid for direct Moon children");
    }
    Ok((options, words))
}

pub fn config_path(options: &Options) -> PathBuf {
    options.config.clone().unwrap_or_else(|| {
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config/moon/config.toml")
    })
}

pub fn load_config(options: &Options) -> Result<Config> {
    let path = config_path(options);
    if !path.exists() && options.config.is_none() {
        return Ok(Config::default());
    }
    let source =
        fs::read_to_string(&path).with_context(|| format!("read config {}", path.display()))?;
    Config::load(&source).with_context(|| format!("config {}", path.display()))
}

/// Read-only installer preflight; the current daemon keeps its committed state.
#[cfg(any(windows, test))]
pub fn restart_config(options: &Options) -> Result<Config> {
    if options.only.is_some() || options.hook == Some(false) || options.no_surfaces {
        bail!(
            "This Moon session uses --only, --no-hook or --no-surfaces and cannot be transferred by setup. Exit this diagnostic session before installing."
        );
    }
    load_config(options)
}

#[cfg(windows)]
pub fn open_settings(options: &Options) -> Result<()> {
    open_settings_page(options, None)
}

#[cfg(any(windows, test))]
pub fn settings_companion_present(path: &std::path::Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => bail!("Moon Settings path is not a file: {}", path.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("check Moon Settings companion"),
    }
}

#[cfg(windows)]
pub fn settings_companion() -> Result<PathBuf> {
    Ok(std::env::current_exe()?.with_file_name("moon-settings.exe"))
}

#[cfg(windows)]
pub fn open_settings_page(options: &Options, page: Option<&str>) -> Result<()> {
    let _apartment = moon_win::platform::Apartment::new()?;
    let companion = settings_companion()?;
    if !settings_companion_present(&companion)? {
        bail!("Moon Settings is missing. Run Moon Setup to repair the installation.");
    }
    let mut arguments = format!("--config \"{}\"", config_path(options).display());
    if let Some(page) = page {
        if !matches!(page, "shortcuts" | "overview") {
            bail!("Unknown Moon Settings page {page:?}");
        }
        arguments.push_str(&format!(" --page {page}"));
    }
    moon_win::runtime::shell_execute(&companion.to_string_lossy(), &arguments)?;
    Ok(())
}

fn config_command(options: &Options, words: &[String]) -> Result<()> {
    match words.first().map(String::as_str).unwrap_or("show") {
        "default" => print!("{DEFAULT_CONFIG}"),
        "path" => println!("{}", config_path(options).display()),
        "show" => print!("{}", load_config(options)?.to_toml().map_err(anyhow::Error::msg)?),
        "validate" => {
            load_config(options)?;
            println!("Configuration is valid");
        }
        "reference" => print!("{}", config::reference_markdown()),
        "schema" => println!("{}", serde_json::to_string_pretty(&config::schema())?),
        "migrate" => {
            let path = config_path(options);
            let source = fs::read_to_string(&path)?;
            let migrated = config::migrate(&source).map_err(anyhow::Error::msg)?;
            Config::load(&migrated)?;
            if migrated != source {
                let backup = path.with_extension("toml.pre-migration");
                let mut backup_file = fs::OpenOptions::new().write(true).create_new(true).open(&backup)
                    .with_context(|| format!("create migration backup {}; move an existing backup before migrating again", backup.display()))?;
                use std::io::Write;
                backup_file.write_all(source.as_bytes())?;
                backup_file.sync_all()?;
                fs::write(&path, migrated)?;
                println!("Migrated {}; original saved to {}", path.display(), backup.display());
            } else {
                println!("Configuration is current");
            }
        }
        other => bail!(
            "unknown config operation {other:?}; use default, show, validate, path, schema or migrate"
        ),
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn window_request(words: &[String], catalog: &serde_json::Value) -> Result<serde_json::Value> {
    use moon_core::{
        command::WindowAction,
        protocol::{Request, RequestAction, WindowTarget},
    };
    let (id, action) = words
        .split_first()
        .context("Use moon window ID focus|float|tile|move-to-workspace NAME [--follow]")?;
    let action = match action {
        [action] if action == "focus" => WindowAction::Focus,
        [action] if action == "float" => WindowAction::Float,
        [action] if action == "tile" => WindowAction::Tile,
        [action, names @ ..] if action == "move-to-workspace" && !names.is_empty() => {
            let follow = names.last().is_some_and(|name| name == "--follow");
            let names = if follow { &names[..names.len() - 1] } else { names };
            if names.is_empty() {
                bail!("Provide a workspace name");
            }
            WindowAction::MoveToWorkspace { workspace: names.join(" "), follow }
        }
        _ => bail!("Window action must be focus, float, tile or move-to-workspace NAME [--follow]"),
    };
    let target = catalog["windows"]
        .as_array()
        .and_then(|windows| windows.iter().find(|window| window["id"].as_str() == Some(id)))
        .context("Window is absent from the current desktop catalog")?;
    let field = |object: &serde_json::Value, key: &str| -> Result<String> {
        Ok(object[key]
            .as_str()
            .with_context(|| format!("Desktop catalog is missing {key}"))?
            .into())
    };
    let request = Request {
        version: 1,
        id: serde_json::json!(1),
        action: RequestAction::Window {
            generation: field(catalog, "generation")?,
            revision: field(catalog, "revision")?,
            target: WindowTarget {
                id: field(target, "id")?,
                process_id: field(target, "process_id")?,
                process_birth: field(target, "process_birth")?,
            },
            action,
        },
    };
    let value = serde_json::to_value(request)?;
    Request::parse(&value.to_string()).map_err(anyhow::Error::msg)?;
    Ok(value)
}

fn print_help() {
    println!(
        "moon {} — Windows 11 window manager\n\n\
moon [start] [--background] [--only PROCESS] [--hook|--no-hook] [--no-surfaces] [--takeover]\n\
moon doctor                  Check capabilities and competing managers\n\
moon settings                Open Moon Settings\n\
moon overview                Search and arrange managed windows\n\
moon undo                    Undo the last deliberate layout change\n\
moon window ID focus|float|tile|move-to-workspace NAME [--follow]\n\
moon query [world|stats|windows|monitors|transactions|config|restart-config|shortcuts|overview|desktop|undo]\n\
moon subscribe               Stream committed state changes\n\
moon config default|show|validate|schema|reference|migrate|path\n\
moon focus|move left|right|up|down\n\
moon resize width|height +2%|-2%\n\
moon workspace NAME          Switch workspace\n\
moon move-to-workspace NAME [--follow]\n\
moon recent-workspace        Return to previous workspace\n\
moon toggle-direction|toggle-floating|toggle-fullscreen|close\n\
moon focus-monitor|move-to-monitor|move-workspace left|right|up|down\n\
moon game-mode start|end\\n\\
moon query games\\n\\
moon pause|reload|restart|exit|restore|cheatsheet\n\
moon exec COMMAND            Launch through the ordinary desktop shell\n\n\
Use --config PATH to select a config file. No file is required.\n\
Place moon options before exec; arguments after exec belong to the application.\n\
Emergency recovery: Ctrl+Alt+Shift+F12.",
        env!("MOON_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn internal_stdio_preserves_child_arguments_and_requires_a_direct_child_role() {
        for mode in ["log", "null"] {
            let (options, words) = parse_args(
                [
                    "start",
                    "--resume",
                    "--wait-parent",
                    "123",
                    "--config",
                    "a folder/moon.toml",
                    "--internal-stdio",
                    mode,
                ]
                .map(str::to_owned)
                .to_vec(),
            )
            .unwrap();
            assert_eq!(options.internal_stdio.as_deref(), Some(mode));
            assert!(options.resume);
            assert_eq!(options.wait_parent, Some(123));
            assert_eq!(options.config, Some(std::path::absolute("a folder/moon.toml").unwrap()));
            assert_eq!(words, ["start"]);
            let (options, words) = parse_args(
                ["watchdog", "123", "456", "--internal-stdio", mode].map(str::to_owned).to_vec(),
            )
            .unwrap();
            assert_eq!(options.internal_stdio.as_deref(), Some(mode));
            assert_eq!(words, ["watchdog", "123", "456"]);
        }
        for args in [
            vec!["start", "--internal-stdio"],
            vec!["start", "--internal-stdio", "inherit"],
            vec!["start", "--internal-stdio", "log", "--internal-stdio", "null"],
            vec!["start", "--background", "--internal-stdio", "log"],
            vec!["query", "world", "--internal-stdio", "log"],
            vec!["--internal-stdio", "null"],
        ] {
            assert!(parse_args(args.into_iter().map(str::to_owned).collect()).is_err());
        }
        let (options, words) = parse_args(
            ["exec", "app.exe", "--internal-stdio", "application-option"]
                .map(str::to_owned)
                .to_vec(),
        )
        .unwrap();
        assert!(options.internal_stdio.is_none());
        assert_eq!(words, ["exec", "app.exe", "--internal-stdio", "application-option"]);
    }

    #[test]
    fn window_cli_uses_exact_catalog_tokens_and_only_typed_actions() {
        let catalog = serde_json::json!({"generation":"20:134391580679147114","revision":"9007199254740993","windows":[{"id":"42","process_id":"11","process_birth":"134391580679147115"}]});
        let request = window_request(
            &["42", "move-to-workspace", "Web Browsing", "--follow"].map(str::to_owned),
            &catalog,
        )
        .unwrap();
        assert_eq!(request["revision"], "9007199254740993");
        assert_eq!(request["target"]["process_birth"], "134391580679147115");
        assert_eq!(
            request["action"],
            serde_json::json!({"type":"move-to-workspace","workspace":"Web Browsing","follow":true})
        );
        assert!(window_request(&["43", "focus"].map(str::to_owned), &catalog).is_err());
        assert!(window_request(&["42", "exec", "example"].map(str::to_owned), &catalog).is_err());
    }
    #[test]
    fn restart_preflight_reads_disk_without_applying_or_rewriting_settings() {
        let directory = std::env::temp_dir().join(format!(
            "moon-preflight-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("config.toml");
        fs::write(&path, "[gaps]\ninner=17\n").unwrap();
        let options = Options { config: Some(path.clone()), ..Options::default() };
        let committed = load_config(&options).unwrap();
        assert_eq!(restart_config(&options).unwrap(), committed);

        let changed = "[gaps]\ninner=24\n";
        fs::write(&path, changed).unwrap();
        let proposed = restart_config(&options).unwrap();
        assert_eq!(proposed.gaps.inner, 24);
        assert_eq!(committed.gaps.inner, 17);
        assert_eq!(fs::read_to_string(&path).unwrap(), changed);

        let invalid = "[gaps]\ninner='invalid'\n";
        fs::write(&path, invalid).unwrap();
        assert!(restart_config(&options).is_err());
        assert_eq!(committed.gaps.inner, 17);
        assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        fs::remove_file(&path).unwrap();
        assert!(restart_config(&options).is_err());
        assert!(!path.exists());
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn restart_preflight_refuses_diagnostic_options_before_reading_configuration() {
        for words in [
            vec!["--no-hook"],
            vec!["--no-surfaces"],
            vec!["--only", "testwin"],
            vec!["--only", "testwin", "--surfaces"],
        ] {
            let (options, _) = parse_args(words.into_iter().map(str::to_owned).collect()).unwrap();
            let before = format!("{options:?}");
            let error = restart_config(&options).unwrap_err().to_string();
            assert!(error.contains("cannot be transferred by setup"));
            assert_eq!(format!("{options:?}"), before);
        }
    }

    #[test]
    fn live_shortcut_status_matches_the_hook_startup_options() {
        assert!(Options::default().hook_enabled());
        for words in [
            vec!["start", "--no-hook"],
            vec!["start", "--only", "testwin"],
            vec!["start", "--only", "testwin", "--surfaces"],
        ] {
            let (options, _) = parse_args(words.into_iter().map(str::to_owned).collect()).unwrap();
            assert!(!options.hook_enabled());
        }
    }

    #[test]
    fn shortcut_help_falls_back_only_for_a_missing_companion() {
        let directory = std::env::temp_dir().join(format!(
            "moon-help-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("moon-settings.exe");
        assert!(!settings_companion_present(&path).unwrap());
        fs::write(&path, b"unexecuted companion fixture").unwrap();
        assert!(settings_companion_present(&path).unwrap());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(settings_companion_present(&path).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn command_arguments_keep_follow_and_negative_resize() {
        let (options, command) = parse_args(
            ["--config", "settings.toml", "move-to-workspace", "2", "--follow"]
                .map(str::to_owned)
                .to_vec(),
        )
        .unwrap();
        assert_eq!(options.config, Some(std::path::absolute("settings.toml").unwrap()));
        assert_eq!(command, ["move-to-workspace", "2", "--follow"]);
        let (_, resize) =
            parse_args(["resize", "width", "-2%"].map(str::to_owned).to_vec()).unwrap();
        assert_eq!(resize, ["resize", "width", "-2%"]);
        assert!(parse_args(vec!["--only".into()]).is_err());
    }

    #[test]
    fn selected_config_is_absolute_before_forwarding_even_for_a_new_file() {
        let relative = "not-yet-created/selected-config.toml";
        let (options, _) =
            parse_args(vec!["--config".into(), relative.into(), "settings".into()]).unwrap();
        let selected = config_path(&options);
        assert!(selected.is_absolute());
        assert_eq!(selected, std::env::current_dir().unwrap().join(relative));
        let (forwarded, _) = parse_args(vec![
            "--config".into(),
            selected.to_string_lossy().into_owned(),
            "start".into(),
        ])
        .unwrap();
        assert_eq!(config_path(&forwarded), selected);
        assert!(parse_args(vec!["--config".into(), String::new()]).is_err());
    }

    #[test]
    fn exec_does_not_consume_application_options() {
        for verb in ["exec", "shell-exec"] {
            let arguments = [
                "--config",
                "moon.toml",
                verb,
                "app.exe",
                "--config",
                "app.toml",
                "--only",
                "child",
                "--background",
                "--",
                "literal",
            ];
            let (options, command) = parse_args(arguments.map(str::to_owned).to_vec()).unwrap();
            assert_eq!(options.config, Some(std::path::absolute("moon.toml").unwrap()));
            assert!(options.only.is_none());
            assert!(!options.background);
            assert_eq!(command, arguments[2..]);
        }
    }

    #[test]
    fn explicit_delimiter_preserves_following_words() {
        let (options, command) = parse_args(
            ["--config", "moon.toml", "--", "custom-command", "--only", "testwin"]
                .map(str::to_owned)
                .to_vec(),
        )
        .unwrap();
        assert_eq!(options.config, Some(std::path::absolute("moon.toml").unwrap()));
        assert!(options.only.is_none());
        assert_eq!(command, ["custom-command", "--only", "testwin"]);
    }
}
