#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use moon_core::config::Config;
    use moon_win::{
        providers::{Providers, Values},
        runtime,
    };
    use std::time::{Duration, Instant};
    fn wait_for_value(providers: &mut Providers, key: &str, error_key: &str) -> Values {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let values = providers.sample();
            if values.contains_key(key)
                || values.contains_key(error_key)
                || Instant::now() >= deadline
            {
                return values;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    let mut providers = Providers::new();
    // This diagnostic explicitly exercises the system palette provider; dark
    // configurations without a theme placeholder do not need to sample it.
    let mut config = Config { theme: "system".into(), ..Config::default() };
    providers.set_config(&config);
    let first = providers.sample();
    assert!(
        !first.keys().any(|key| key.starts_with("weather")),
        "weather must be disabled by default"
    );
    std::thread::sleep(Duration::from_secs(1));
    let values = providers.sample();
    for key in ["clock", "cpu.percent", "memory.percent", "network", "keyboard", "system.theme"] {
        assert!(values.contains_key(key), "provider missing: {key}; keys: {:?}", values.keys());
    }
    assert!(matches!(values.get("system.theme").map(String::as_str), Some("dark" | "light")));
    config.bar.widgets.push(moon_core::config::WidgetConfig {
        id: "exec-smoke".into(),
        format: "{exec-smoke}".into(),
        align: "right".into(),
        color: None,
        background: None,
        icon: None,
        icon_color: None,
        click: None,
        exec: Some("echo widget-ok".into()),
        interval_seconds: 5,
        max_width: 100,
    });
    config.validate()?;
    providers.set_config(&config);
    let exec = wait_for_value(&mut providers, "exec-smoke", "exec-smoke.error");
    assert_eq!(
        exec.get("exec-smoke").map(String::as_str),
        Some("widget-ok"),
        "{:?}",
        exec.get("exec-smoke.error")
    );
    if std::env::args().any(|arg| arg == "--weather") {
        config.weather = Some(moon_core::config::WeatherConfig {
            latitude: 0.0,
            longitude: 0.0,
            interval_seconds: 900,
        });
        providers.set_config(&config);
        let weather = wait_for_value(&mut providers, "weather.temperature", "weather.error");
        assert!(
            weather.contains_key("weather.temperature"),
            "configured weather missing: {:?}",
            weather.get("weather.error")
        );
        println!("configured weather passed at public test coordinate 0,0");
    }
    let echo =
        runtime::capture_unprivileged("echo moon-provider-ok", Duration::from_secs(5), 4096)?;
    assert_eq!(echo.trim(), "moon-provider-ok");
    let start = Instant::now();
    assert!(
        runtime::capture_unprivileged("ping -n 30 127.0.0.1", Duration::from_secs(1), 4096)
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(15));
    assert!(
        runtime::capture_unprivileged(
            "for /L %i in (1,1,4000) do @echo moon-output-limit",
            Duration::from_secs(5),
            128
        )
        .is_err()
    );
    println!(
        "{}",
        serde_json::json!({"local_provider_keys": values.keys().collect::<Vec<_>>(), "weather_disabled": true, "exec_capture": true, "exec_timeout": true, "exec_output_limit": true})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("providers_probe requires Windows");
    std::process::exit(1);
}
