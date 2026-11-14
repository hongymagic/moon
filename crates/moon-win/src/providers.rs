//! Native, local-only provider sampling. Construct and sample on one worker thread.
//! Missing capabilities produce an explicit error key, never invented measurements.
use moon_core::{
    config::{Config, WeatherConfig, WidgetConfig},
    scene::template_placeholders,
};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties,
    },
    UI::ViewManagement::{UIColorType, UISettings},
    Win32::{
        Foundation::{ERROR_SUCCESS, FILETIME},
        Globalization::{GetLocaleInfoW, LOCALE_SISO639LANGNAME},
        Media::Audio::{
            Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole,
            eRender,
        },
        NetworkManagement::{
            IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2},
            Ndis::IfOperStatusUp,
        },
        System::{
            Com::{CLSCTX_ALL, CoCreateInstance},
            Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
            SystemInformation::{GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX},
            Threading::GetSystemTimes,
            WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        },
        UI::{
            Input::KeyboardAndMouse::GetKeyboardLayout,
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    },
};
use windows_future::{AsyncStatus, IAsyncOperation};

pub type Values = BTreeMap<String, String>;
const MAX_EXEC_JOBS: usize = 4;

fn exec_for<'a>(widgets: &'a [WidgetConfig], id: &str) -> Option<&'a str> {
    widgets.iter().find(|widget| widget.id == id).and_then(|widget| widget.exec.as_deref())
}

type Completed<T> = (Instant, Result<T, String>);

struct BackgroundSample<T> {
    completed: Option<Completed<T>>,
    pending: Option<(bool, Receiver<Completed<T>>)>,
}

impl<T: Send + 'static> BackgroundSample<T> {
    fn new() -> Self {
        Self { completed: None, pending: None }
    }

    fn invalidate(&mut self) {
        self.completed = None;
        if let Some((valid, _)) = &mut self.pending {
            *valid = false;
        }
    }

    fn poll(&mut self) {
        let Some((valid, receive)) = &self.pending else { return };
        let completed = match receive.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                (Instant::now(), Err("Provider worker stopped before returning a result".into()))
            }
        };
        if *valid {
            self.completed = Some(completed);
        }
        self.pending = None;
    }

    fn due(&self, seconds: u32) -> bool {
        self.pending.is_none()
            && self
                .completed
                .as_ref()
                .is_none_or(|(time, _)| time.elapsed() >= Duration::from_secs(u64::from(seconds)))
    }

    fn start(&mut self, name: &str, sample: impl FnOnce() -> Result<T, String> + Send + 'static) {
        let (sender, receive) = mpsc::channel();
        match thread::Builder::new().name(name.into()).spawn(move || {
            let result = sample();
            let _ = sender.send((Instant::now(), result));
        }) {
            Ok(_) => self.pending = Some((true, receive)),
            Err(error) => self.completed = Some((Instant::now(), Err(error.to_string()))),
        }
    }

    fn value(&self) -> Option<&Result<T, String>> {
        self.completed.as_ref().map(|(_, result)| result)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProviderDemand {
    theme: bool,
    clock: bool,
    memory: bool,
    cpu: bool,
    battery: bool,
    keyboard: bool,
    volume: bool,
    network: bool,
    media: bool,
    weather: bool,
}
impl ProviderDemand {
    fn from_config(config: &Config) -> Self {
        let mut demand = Self { theme: config.theme == "system", ..Self::default() };
        for widget in &config.bar.widgets {
            // Formats control optional visibility even for exec widgets. Exec
            // IDs are direct value lookups too: built-in collisions retain the
            // existing fallback/override order, including when the bar is off.
            let keys = template_placeholders(&widget.format)
                .map(|key| (key, config.bar.enabled))
                .chain(widget.exec.as_ref().map(|_| (widget.id.as_str(), true)));
            for (key, enabled) in keys {
                // Theme also supplies floating native borders and carousel fills.
                if matches!(key, "system.theme" | "system.theme.error") {
                    demand.theme = true;
                }
                if !enabled {
                    continue;
                }
                match key.split('.').next().unwrap_or_default() {
                    "clock" => demand.clock = true,
                    "memory" => demand.memory = true,
                    "cpu" => demand.cpu = true,
                    "battery" => demand.battery = true,
                    "keyboard" => demand.keyboard = true,
                    "volume" => demand.volume = true,
                    "network" => demand.network = true,
                    "media" => demand.media = true,
                    "weather" => demand.weather = true,
                    _ => {}
                }
            }
        }
        demand
    }
}

/// CPU deltas are retained only for sampling; this object contains no WM state.
pub struct Providers {
    demand: ProviderDemand,
    previous_cpu: Option<(u64, u64)>,
    com_initialized: bool,
    ui_settings: Option<UISettings>,
    media: Option<GlobalSystemMediaTransportControlsSessionManager>,
    media_request:
        Option<(Instant, IAsyncOperation<GlobalSystemMediaTransportControlsSessionManager>)>,
    media_properties: Option<(
        Instant,
        IAsyncOperation<GlobalSystemMediaTransportControlsSessionMediaProperties>,
    )>,
    media_values: Values,
    media_retry: Instant,
    config: Config,
    weather: BackgroundSample<(f64, i64)>,
    exec: BTreeMap<String, BackgroundSample<String>>,
}

impl Default for Providers {
    fn default() -> Self {
        Self::new()
    }
}

impl Providers {
    pub fn new() -> Self {
        // A provider is thread-affine: COM objects are created and released here.
        let com_initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_ok();
        let config = Config::default();
        Self {
            demand: ProviderDemand::from_config(&config),
            previous_cpu: None,
            com_initialized,
            ui_settings: None,
            media: None,
            media_request: None,
            media_properties: None,
            media_values: Values::new(),
            media_retry: Instant::now(),
            config,
            weather: BackgroundSample::new(),
            exec: BTreeMap::new(),
        }
    }

    /// Changes to location invalidate the weather cache. No location means no requests.
    pub fn set_config(&mut self, config: &Config) {
        let demand = ProviderDemand::from_config(config);
        if demand.cpu != self.demand.cpu {
            // Re-enabled CPU starts with a fresh pair of samples, not the time
            // spent disabled (which would hide its current utilization).
            self.previous_cpu = None;
        }
        if self.config.weather != config.weather || !demand.weather {
            self.weather.invalidate();
        }
        if !demand.media {
            self.clear_media();
        } else if !self.demand.media {
            self.media_retry = Instant::now();
        }
        if !demand.theme {
            self.ui_settings = None;
        }
        self.demand = demand;
        self.exec.retain(|id, sample| {
            let next = exec_for(&config.bar.widgets, id);
            if next != exec_for(&self.config.bar.widgets, id) {
                sample.invalidate();
            }
            next.is_some() || sample.pending.is_some()
        });
        self.config = config.clone();
    }

    fn clear_media(&mut self) {
        if let Some((_, request)) = self.media_request.take() {
            let _ = request.Cancel();
        }
        if let Some((_, request)) = self.media_properties.take() {
            let _ = request.Cancel();
        }
        self.media = None;
        self.media_values.clear();
    }

    /// Call no faster than once a second, off the keyboard and message-loop threads.
    pub fn sample(&mut self) -> Values {
        let mut values = Values::new();
        if self.demand.theme && self.com_initialized && self.ui_settings.is_none() {
            self.ui_settings = UISettings::new().ok();
        }
        if let Some(settings) = &self.ui_settings {
            match settings.GetColorValue(UIColorType::Foreground) {
                Ok(color) => {
                    // Microsoft's Win32 theme guidance derives the mode from foreground luminance.
                    let foreground_is_light =
                        5 * u32::from(color.G) + 2 * u32::from(color.R) + u32::from(color.B)
                            > 8 * 128;
                    values.insert(
                        "system.theme".into(),
                        if foreground_is_light { "dark" } else { "light" }.into(),
                    );
                }
                Err(error) => {
                    values.insert("system.theme.error".into(), error.to_string());
                }
            }
        }
        unsafe {
            if self.demand.clock {
                let now = GetLocalTime();
                let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
                let months = [
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov",
                    "Dec",
                ];
                values.insert(
                    "clock".into(),
                    format!(
                        "{} {} {} {:02}:{:02}",
                        days[usize::from(now.wDayOfWeek).min(6)],
                        now.wDay,
                        months[usize::from(now.wMonth.saturating_sub(1)).min(11)],
                        now.wHour,
                        now.wMinute
                    ),
                );
                values.insert("clock.time".into(), format!("{:02}:{:02}", now.wHour, now.wMinute));
                values.insert(
                    "clock.date".into(),
                    format!("{:04}-{:02}-{:02}", now.wYear, now.wMonth, now.wDay),
                );
            }
            if self.demand.memory {
                let mut memory = MEMORYSTATUSEX {
                    dwLength: size_of::<MEMORYSTATUSEX>() as u32,
                    ..Default::default()
                };
                match GlobalMemoryStatusEx(&mut memory) {
                    Ok(()) => {
                        values.insert("memory".into(), format!("{}%", memory.dwMemoryLoad));
                        values.insert("memory.percent".into(), memory.dwMemoryLoad.to_string());
                        values.insert(
                            "memory.used".into(),
                            memory.ullTotalPhys.saturating_sub(memory.ullAvailPhys).to_string(),
                        );
                        values.insert("memory.total".into(), memory.ullTotalPhys.to_string());
                    }
                    Err(error) => {
                        values.insert("memory.error".into(), error.to_string());
                    }
                }
            }
            if self.demand.cpu {
                let (mut idle, mut kernel, mut user) =
                    (FILETIME::default(), FILETIME::default(), FILETIME::default());
                match GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) {
                    Ok(()) => {
                        let total = filetime(kernel).saturating_add(filetime(user));
                        let idle = filetime(idle);
                        if let Some((old_total, old_idle)) = self.previous_cpu {
                            let elapsed = total.saturating_sub(old_total);
                            if elapsed > 0 {
                                let percent = (100.0
                                    * (elapsed.saturating_sub(idle.saturating_sub(old_idle)))
                                        as f64
                                    / elapsed as f64)
                                    .clamp(0.0, 100.0);
                                values.insert("cpu".into(), format!("{percent:.0}%"));
                                values.insert("cpu.percent".into(), format!("{percent:.0}"));
                            }
                        }
                        self.previous_cpu = Some((total, idle));
                    }
                    Err(error) => {
                        values.insert("cpu.error".into(), error.to_string());
                    }
                }
            }
            if self.demand.battery {
                let mut power = SYSTEM_POWER_STATUS::default();
                if GetSystemPowerStatus(&mut power).is_ok()
                    && power.BatteryFlag != 128
                    && power.BatteryLifePercent <= 100
                {
                    values.insert(
                        "battery".into(),
                        format!(
                            "{}{}%",
                            if power.ACLineStatus == 1 { "+" } else { "" },
                            power.BatteryLifePercent
                        ),
                    );
                    values.insert("battery.percent".into(), power.BatteryLifePercent.to_string());
                    values.insert("battery.charging".into(), (power.ACLineStatus == 1).to_string());
                }
            }
            if self.demand.keyboard {
                let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
                let locale = (GetKeyboardLayout(thread).0 as usize & 0xffff) as u32;
                let mut language = [0u16; 32];
                let length = GetLocaleInfoW(locale, LOCALE_SISO639LANGNAME, Some(&mut language));
                if length > 1 {
                    values.insert(
                        "keyboard".into(),
                        String::from_utf16_lossy(&language[..length as usize - 1]).to_uppercase(),
                    );
                }
            }
            if self.demand.volume {
                match volume() {
                    Ok((level, muted)) => {
                        values.insert(
                            "volume".into(),
                            if muted { "Muted".into() } else { format!("{:.0}%", level * 100.0) },
                        );
                        values.insert("volume.percent".into(), format!("{:.0}", level * 100.0));
                        values.insert("volume.muted".into(), muted.to_string());
                    }
                    Err(error) => {
                        values.insert("volume.error".into(), error.to_string());
                    }
                }
            }
            if self.demand.network {
                network(&mut values);
            }
        }
        if self.demand.media {
            if self.com_initialized
                && self.media.is_none()
                && self.media_request.is_none()
                && Instant::now() >= self.media_retry
            {
                self.media_request =
                    GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                        .ok()
                        .map(|request| (Instant::now(), request));
                self.media_retry = Instant::now() + Duration::from_secs(30);
            }
            if let Some((started, request)) = &self.media_request {
                if request.Status().is_ok_and(|status| status != AsyncStatus::Started) {
                    self.media = request.GetResults().ok();
                    self.media_request = None;
                } else if started.elapsed() > Duration::from_secs(5) {
                    let _ = request.Cancel();
                    self.media_request = None;
                }
            }
            if let Some((started, request)) = &self.media_properties {
                if request.Status().is_ok_and(|status| status != AsyncStatus::Started) {
                    match request.GetResults() {
                        Ok(media) => {
                            let title = media.Title().map(|s| s.to_string()).unwrap_or_default();
                            let artist = media.Artist().map(|s| s.to_string()).unwrap_or_default();
                            self.media_values.clear();
                            self.media_values.insert("media".into(), title.clone());
                            self.media_values.insert("media.title".into(), title);
                            self.media_values.insert("media.artist".into(), artist);
                        }
                        Err(error) => {
                            self.media_values.clear();
                            self.media_values.insert("media.error".into(), error.to_string());
                        }
                    }
                    self.media_properties = None;
                } else if started.elapsed() > Duration::from_secs(5) {
                    let _ = request.Cancel();
                    self.media_properties = None;
                    self.media_values.clear();
                    self.media_values
                        .insert("media.error".into(), "media request timed out".into());
                }
            }
            if self.media_properties.is_none()
                && let Some(manager) = &self.media
            {
                if let Ok(session) = manager.GetCurrentSession() {
                    self.media_properties =
                        session.TryGetMediaPropertiesAsync().ok().map(|op| (Instant::now(), op));
                } else {
                    self.media_values.clear();
                }
            }
            values.extend(self.media_values.clone());
        }
        self.sample_external(&mut values);
        values
    }

    fn sample_external(&mut self, values: &mut Values) {
        let widgets: Vec<_> = self.config.bar.widgets.iter().filter(|w| w.exec.is_some()).collect();
        for sample in self.exec.values_mut() {
            sample.poll();
        }
        self.exec.retain(|_, sample| sample.pending.is_some() || sample.completed.is_some());
        let active = self.exec.values().filter(|sample| sample.pending.is_some()).count();
        let mut due: Vec<_> = widgets
            .iter()
            .filter(|widget| {
                self.exec.get(&widget.id).is_none_or(|sample| sample.due(widget.interval_seconds))
            })
            .map(|widget| {
                let last = self
                    .exec
                    .get(&widget.id)
                    .and_then(|sample| sample.completed.as_ref().map(|(time, _)| *time));
                (last, *widget)
            })
            .collect();
        // Prefer work that has never run, then the oldest result, so a short
        // interval cannot starve widgets later in the configuration.
        due.sort_by_key(|(last, _)| *last);
        for (_, widget) in due.into_iter().take(MAX_EXEC_JOBS.saturating_sub(active)) {
            let command = widget.exec.clone().expect("exec widget has a command");
            self.exec.entry(widget.id.clone()).or_insert_with(BackgroundSample::new).start(
                "moon-provider-exec",
                move || {
                    let _apartment =
                        crate::platform::Apartment::new().map_err(|e| e.to_string())?;
                    crate::runtime::capture_unprivileged(&command, Duration::from_secs(5), 4096)
                        .map(|value| value.trim().replace(['\r', '\n'], " "))
                        .map_err(|error| error.to_string())
                },
            );
        }
        for widget in widgets {
            if let Some(result) = self.exec.get(&widget.id).and_then(BackgroundSample::value) {
                match result {
                    Ok(value) => {
                        values.insert(widget.id.clone(), value.clone());
                    }
                    Err(error) => {
                        values.insert(format!("{}.error", widget.id), error.clone());
                    }
                }
            }
        }
        self.weather.poll();
        if self.demand.weather
            && let Some(config) = &self.config.weather
        {
            if self.weather.due(config.interval_seconds) {
                let config = config.clone();
                self.weather.start("moon-provider-weather", move || fetch_weather(&config));
            }
            match self.weather.value() {
                Some(Ok((temperature, code))) => {
                    values.insert("weather".into(), format!("{temperature:.0}°C"));
                    values.insert("weather.temperature".into(), format!("{temperature:.1}"));
                    values.insert("weather.code".into(), code.to_string());
                    values.insert("weather.source".into(), "Open-Meteo.com (CC BY 4.0)".into());
                }
                Some(Err(error)) => {
                    values.insert("weather.error".into(), error.clone());
                }
                None => {}
            }
        }
    }
}

impl Drop for Providers {
    fn drop(&mut self) {
        self.clear_media();
        self.ui_settings = None;
        if self.com_initialized {
            unsafe { RoUninitialize() };
        }
    }
}

fn filetime(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

unsafe fn volume() -> windows::core::Result<(f32, bool)> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let endpoint = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let volume: IAudioEndpointVolume = endpoint.Activate(CLSCTX_ALL, None)?;
        Ok((volume.GetMasterVolumeLevelScalar()?, volume.GetMute()?.as_bool()))
    }
}

unsafe fn network(values: &mut Values) {
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        let result = GetIfTable2(&mut table);
        if result != ERROR_SUCCESS {
            values.insert("network.error".into(), format!("GetIfTable2: {}", result.0));
            return;
        }
        if table.is_null() {
            return;
        }
        let rows =
            std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        let active = rows
            .iter()
            .filter(|row| {
                row.OperStatus == IfOperStatusUp && row.Type != 24 && row.TransmitLinkSpeed > 0
            })
            .max_by_key(|row| {
                (row.InterfaceAndOperStatusFlags._bitfield & 1, row.TransmitLinkSpeed)
            });
        if let Some(row) = active {
            let len = row.Alias.iter().position(|&c| c == 0).unwrap_or(row.Alias.len());
            values.insert("network".into(), String::from_utf16_lossy(&row.Alias[..len]));
            values.insert("network.received".into(), row.InOctets.to_string());
            values.insert("network.sent".into(), row.OutOctets.to_string());
        } else {
            values.insert("network".into(), "Offline".into());
        }
        FreeMibTable(table.cast());
    }
}

/// The endpoint is fixed; coordinates are validated by Config. Redirects retain
/// WinHTTP's TLS verification and no credentials/cookies are supplied.
fn fetch_weather(config: &WeatherConfig) -> std::result::Result<(f64, i64), String> {
    use windows::{
        Win32::Networking::WinHttp::*,
        core::{PCWSTR, w},
    };
    struct Internet(*mut std::ffi::c_void);
    impl Internet {
        fn new(handle: *mut std::ffi::c_void) -> std::result::Result<Self, String> {
            if handle.is_null() {
                Err(windows::core::Error::from_thread().to_string())
            } else {
                Ok(Self(handle))
            }
        }
    }
    impl Drop for Internet {
        fn drop(&mut self) {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
    unsafe {
        let session = Internet::new(WinHttpOpen(
            w!("moon/0.1 weather"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, 3000, 3000, 3000, 3000).map_err(|e| e.to_string())?;
        let connect = Internet::new(WinHttpConnect(session.0, w!("api.open-meteo.com"), 443, 0))?;
        let path: Vec<u16> = format!("/v1/forecast?latitude={}&longitude={}&current=temperature_2m,weather_code&forecast_days=1", config.latitude, config.longitude).encode_utf16().chain(Some(0)).collect();
        let request = Internet::new(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(|e| e.to_string())?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(|e| e.to_string())?;
        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(|e| e.to_string())?;
        if status != 200 {
            return Err(format!("weather HTTP {status}"));
        }
        let mut body = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut buffer = [0u8; 4096];
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read)
                .map_err(|e| e.to_string())?;
            if read == 0 {
                break;
            }
            if body.len() + read as usize > 262_144 || Instant::now() > deadline {
                return Err("weather response exceeds size/time limit".into());
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        let json: serde_json::Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        let current = &json["current"];
        let temperature = current["temperature_2m"]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("weather temperature missing")?;
        let code = current["weather_code"].as_i64().ok_or("weather code missing")?;
        Ok((temperature, code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_format(format: &str) -> Config {
        let mut config = Config::default();
        config.bar.widgets.truncate(1);
        config.bar.widgets[0].id = "test".into();
        config.bar.widgets[0].format = format.into();
        config
    }

    fn cached<T: Send + 'static>(result: Result<T, String>) -> BackgroundSample<T> {
        BackgroundSample { completed: Some((Instant::now(), result)), pending: None }
    }

    #[test]
    fn default_templates_keep_all_displayed_provider_families() {
        let demand = ProviderDemand::from_config(&Config::default());
        assert!(demand.clock && demand.memory && demand.cpu && demand.battery);
        assert!(
            demand.keyboard && demand.volume && demand.network && demand.media && demand.weather
        );
        assert!(!demand.theme, "explicit dark palette does not need system colors");
        assert_eq!(
            ProviderDemand::from_config(&with_format("fixed text")),
            ProviderDemand::default()
        );
    }

    #[test]
    fn subkeys_errors_and_nested_visibility_enable_only_their_families() {
        let demand = ProviderDemand::from_config(&with_format(
            "{cpu.percent} {memory.error} {network.received} {media.artist} {volume.error} {weather.error}",
        ));
        assert!(
            demand.cpu
                && demand.memory
                && demand.network
                && demand.media
                && demand.volume
                && demand.weather
        );
        assert!(!demand.clock && !demand.battery && !demand.keyboard && !demand.theme);
        assert_eq!(
            ProviderDemand::from_config(&with_format("unmatched {cpu")),
            ProviderDemand::default()
        );
        assert!(ProviderDemand::from_config(&with_format("prefix {{cpu}")).cpu);
        assert_eq!(
            ProviderDemand::from_config(&with_format(
                "{title}{mode}{paused}{direction}{workspaces}{unknown}"
            )),
            ProviderDemand::default()
        );
    }

    #[test]
    fn exec_optional_visibility_and_theme_consumers_keep_dependencies() {
        let mut config = with_format("{memory.percent}");
        config.bar.widgets[0].exec = Some("echo fixture".into());
        assert!(ProviderDemand::from_config(&config).memory);
        config.bar.enabled = false;
        assert_eq!(ProviderDemand::from_config(&config), ProviderDemand::default());
        config.theme = "system".into();
        assert!(ProviderDemand::from_config(&config).theme);
        config.theme = "dark".into();
        config.bar.widgets[0].format = "{system.theme.error}".into();
        assert!(ProviderDemand::from_config(&config).theme);
    }

    #[test]
    fn disabling_providers_clears_samples_and_reenabling_media_has_no_retry_delay() {
        let mut providers = Providers::new();
        assert!(providers.media_request.is_none(), "construction must not request media");
        assert!(providers.media_retry <= Instant::now());
        providers.previous_cpu = Some((100, 20));
        providers.media_values.insert("media".into(), "stale".into());
        providers.media_retry = Instant::now() + Duration::from_secs(30);
        providers.weather = cached(Ok((20.0, 1)));
        providers.set_config(&with_format("fixed"));
        assert!(providers.previous_cpu.is_none());
        assert!(providers.media_values.is_empty());
        assert!(providers.media_request.is_none() && providers.media_properties.is_none());
        assert!(providers.weather.value().is_none());
        assert!(providers.sample().is_empty());
        providers.set_config(&Config::default());
        assert!(providers.previous_cpu.is_none());
        assert!(providers.media_retry <= Instant::now());
        assert!(providers.media_request.is_none(), "reactivation waits for sampling");
    }

    #[test]
    fn explicit_exec_output_survives_disabled_bar_and_no_builtin_demand() {
        let mut providers = Providers::new();
        let mut config = with_format("fixed");
        config.bar.enabled = false;
        let command = "echo fixture".to_owned();
        config.bar.widgets[0].exec = Some(command.clone());
        providers.set_config(&config);
        // The valid cached result exercises the real exec sampling branch without
        // starting a shell or requiring access to any desktop application.
        providers.exec.insert("test".into(), cached(Ok("fixture".into())));
        assert_eq!(providers.sample(), Values::from([("test".into(), "fixture".into())]));
    }

    #[test]
    fn failed_exec_with_builtin_id_keeps_native_fallback() {
        let mut providers = Providers::new();
        let mut config = with_format("fixed");
        config.bar.widgets[0].id = "memory".into();
        let command = "fixture command is never launched".to_owned();
        config.bar.widgets[0].exec = Some(command.clone());
        providers.set_config(&config);
        providers.exec.insert("memory".into(), cached(Err("fixture failure".into())));
        let values = providers.sample();
        assert!(values.get("memory").is_some_and(|value| value.ends_with('%')));
        assert_eq!(values.get("memory.error").map(String::as_str), Some("fixture failure"));
    }

    #[test]
    fn weather_still_overrides_exec_with_colliding_id_without_fetching() {
        let mut providers = Providers::new();
        let mut config = with_format("fixed");
        config.bar.widgets[0].id = "weather".into();
        let command = "fixture command is never launched".to_owned();
        config.bar.widgets[0].exec = Some(command.clone());
        config.weather =
            Some(WeatherConfig { latitude: 0.0, longitude: 0.0, interval_seconds: 900 });
        providers.set_config(&config);
        providers.exec.insert("weather".into(), cached(Ok("command output".into())));
        providers.weather = cached(Ok((20.0, 1)));
        assert_eq!(providers.sample().get("weather").map(String::as_str), Some("20°C"));
    }

    #[test]
    fn pending_external_samples_do_not_delay_local_values_and_stale_replies_are_discarded() {
        let mut providers = Providers::new();
        let mut config = with_format("{clock} {weather} {test}");
        config.bar.widgets[0].exec = Some("old command".into());
        config.weather =
            Some(WeatherConfig { latitude: 0.0, longitude: 0.0, interval_seconds: 900 });
        providers.set_config(&config);
        let (exec_sender, exec_receiver) = mpsc::channel();
        let (weather_sender, weather_receiver) = mpsc::channel();
        providers.exec.insert(
            "test".into(),
            BackgroundSample { completed: None, pending: Some((true, exec_receiver)) },
        );
        providers.weather.pending = Some((true, weather_receiver));
        let values = providers.sample();
        assert!(values.contains_key("clock.time"));
        assert!(!values.contains_key("test") && !values.contains_key("weather"));

        config.bar.widgets[0].exec = None;
        config.weather = None;
        providers.set_config(&config);
        exec_sender.send((Instant::now(), Ok("stale".into()))).unwrap();
        weather_sender.send((Instant::now(), Ok((20.0, 1)))).unwrap();
        let values = providers.sample();
        assert!(!values.contains_key("test") && !values.contains_key("weather"));
        assert!(providers.exec.is_empty() && providers.weather.pending.is_none());
    }

    #[test]
    fn direct_exec_subkey_error_and_theme_ids_enable_their_sources() {
        for (id, expected) in [
            ("memory.used", ProviderDemand { memory: true, ..ProviderDemand::default() }),
            ("volume.error", ProviderDemand { volume: true, ..ProviderDemand::default() }),
            ("system.theme", ProviderDemand { theme: true, ..ProviderDemand::default() }),
            ("system.theme.error", ProviderDemand { theme: true, ..ProviderDemand::default() }),
        ] {
            let mut config = with_format("fixed");
            config.bar.enabled = false;
            config.bar.widgets[0].id = id.into();
            config.bar.widgets[0].exec = Some("echo fixture".into());
            assert_eq!(ProviderDemand::from_config(&config), expected, "exec id {id}");
        }
    }
}
