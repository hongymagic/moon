use anyhow::{Context, Result, bail};
use moon_core::config::{self, Config, DEFAULT_CONFIG};
use std::{fs, path::PathBuf};

#[cfg(windows)]
mod daemon;
#[cfg(any(windows, test))]
mod journal;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub config: Option<PathBuf>,
    pub only: Option<String>,
    pub no_hook: bool,
    pub no_surfaces: bool,
    pub surfaces: bool,
    pub takeover: bool,
    pub resume: bool,
    pub background: bool,
    pub wait_parent: Option<u32>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("moon: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let (options, words) = parse_args(std::env::args().skip(1).collect())?;
    let first = words.first().map(String::as_str).unwrap_or("start");
    match first {
        "help" | "--help" | "-h" => print_help(),
        "version" | "--version" | "-V" => println!("moon {}", env!("CARGO_PKG_VERSION")),
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
            "--no-hook" => options.no_hook = true,
            "--no-surfaces" => options.no_surfaces = true,
            "--surfaces" => options.surfaces = true,
            "--takeover" => options.takeover = true,
            "--resume" => options.resume = true,
            "--background" => options.background = true,
            "--wait-parent" => {
                options.wait_parent =
                    Some(args.next().context("--wait-parent requires a PID")?.parse()?)
            }
            _ => words.push(argument),
        }
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

#[cfg(windows)]
pub fn open_settings(options: &Options) -> Result<()> {
    let _apartment = moon_win::platform::Apartment::new()?;
    let companion = std::env::current_exe()?.with_file_name("moon-settings.exe");
    if !companion.is_file() {
        bail!("Moon Settings is missing. Run Moon Setup to repair the installation.");
    }
    moon_win::runtime::shell_execute(
        &companion.to_string_lossy(),
        &format!("--config \"{}\"", config_path(options).display()),
    )?;
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

fn print_help() {
    println!(
        "moon {} — Windows 11 window manager\n\n\
moon [start] [--background] [--only PROCESS] [--no-hook] [--no-surfaces] [--takeover]\n\
moon doctor                  Check capabilities and competing managers\n\
moon settings                Open Moon Settings\n\
moon query [world|stats|windows|monitors|transactions|config]\n\
moon subscribe               Stream committed state changes\n\
moon config default|show|validate|schema|reference|migrate|path\n\
moon focus|move left|right|up|down\n\
moon resize width|height +2%|-2%\n\
moon workspace NAME          Switch workspace\n\
moon move-to-workspace NAME [--follow]\n\
moon recent-workspace        Return to previous workspace\n\
moon toggle-direction|toggle-floating|toggle-fullscreen|close\n\
moon focus-monitor|move-to-monitor|move-workspace left|right|up|down\n\
moon pause|reload|restart|exit|restore|cheatsheet\n\
moon exec COMMAND            Launch through the ordinary desktop shell\n\n\
Use --config PATH to select a config file. No file is required.\n\
Place moon options before exec; arguments after exec belong to the application.\n\
Emergency recovery: Ctrl+Alt+Shift+F12.",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;
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
