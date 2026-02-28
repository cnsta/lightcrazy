use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{
        self, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use throbber_widgets_tui::ThrobberState;
use tui_slider::SliderState;

use crate::device::{
    protocol::{self, LiftOffDistance, PollingRate},
    transport::Device,
    worker::BatteryWorker,
};
use crate::settings::{Settings, INTERVAL_OPTIONS};

/// The six DPI presets. The slider uses the *index* (0–5) so all steps are
/// equal width on screen regardless of the numeric gaps between values.
pub const DPI_VALUES: [u16; 6] = [400, 800, 1600, 3200, 6400, 12800];

pub const POLLING_RATES: [PollingRate; 7] = [
    PollingRate::Hz125,
    PollingRate::Hz250,
    PollingRate::Hz500,
    PollingRate::Hz1000,
    PollingRate::Hz2000,
    PollingRate::Hz4000,
    PollingRate::Hz8000,
];

pub const LOD_OPTIONS: [LiftOffDistance; 3] = [
    LiftOffDistance::Low,
    LiftOffDistance::Medium,
    LiftOffDistance::High,
];

const STATUS_MSG_TTL: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq)]
pub enum SettingRow {
    Dpi,
    PollingRate,
    LiftOffDistance,
    Debounce,
    AngleSnap,
    RippleControl,
    MotionSync,
    TurboMode,
    NotificationThreshold,
    BatteryInterval,
}

pub const SETTINGS_ROWS: [SettingRow; 10] = [
    SettingRow::Dpi,
    SettingRow::PollingRate,
    SettingRow::Debounce,
    SettingRow::LiftOffDistance,
    SettingRow::AngleSnap,
    SettingRow::RippleControl,
    SettingRow::MotionSync,
    SettingRow::TurboMode,
    SettingRow::NotificationThreshold,
    SettingRow::BatteryInterval,
];

pub struct App {
    pub device: Option<Arc<Mutex<Device>>>,
    pub device_path: String,
    pub device_wired: bool,

    pub settings: Settings,
    pub battery: Option<protocol::MouseStatus>,
    pub battery_loading: bool,
    pub firmware: Option<String>,

    /// Last DPI value confirmed on the device (set at startup via firmware query).
    pub dpi: u16,
    /// Index into DPI_VALUES (0–5). Equal spacing on screen. Pending until Enter.
    pub dpi_slider: SliderState,

    /// Index into POLLING_RATES (0–6). Equal spacing on screen. Pending until Enter.
    pub polling_slider: SliderState,

    /// Debounce, 0–20 ms. Already linear so the slider value IS the ms value. Pending until Enter.
    pub debounce_slider: SliderState,

    pub settings_row: usize,
    pub status_msg: Option<(String, bool, Instant)>,
    pub should_quit: bool,
    pub throbber_state: ThrobberState,

    _worker: Option<BatteryWorker>,
    battery_rx: Option<std::sync::mpsc::Receiver<crate::device::worker::BatteryUpdate>>,
}

impl App {
    pub fn new() -> Self {
        let settings = Settings::load();

        let (device, device_path, device_wired, firmware, dpi) = match Device::open() {
            Ok(dev) => {
                let path = dev.path().display().to_string();
                let wired = dev.is_wired();
                let firmware = protocol::get_firmware(&dev).ok();
                let dpi = protocol::query_current_dpi(&dev).unwrap_or(1600);
                (Some(Arc::new(Mutex::new(dev))), path, wired, firmware, dpi)
            }
            Err(_) => (None, String::new(), false, None, 1600u16),
        };

        if let Some(ref dev_arc) = device {
            let dev = dev_arc.lock().unwrap();
            apply_settings_to_device(&dev, &settings);
        }

        let battery_loading = device.is_some();
        let interval = settings.battery_interval_secs;
        let (worker, battery_rx) = BatteryWorker::spawn(interval, device.clone());

        let dpi_idx = DPI_VALUES.iter().position(|&d| d == dpi).unwrap_or(2);
        let polling_idx = POLLING_RATES
            .iter()
            .position(|&r| r == settings.polling_rate())
            .unwrap_or(3);

        let dpi_slider = SliderState::new(dpi_idx as f64, 0.0, (DPI_VALUES.len() - 1) as f64);
        let polling_slider =
            SliderState::new(polling_idx as f64, 0.0, (POLLING_RATES.len() - 1) as f64);
        let debounce_slider = SliderState::new(settings.debounce_ms as f64, 0.0, 20.0);

        Self {
            device,
            device_path,
            device_wired,
            settings,
            battery: None,
            battery_loading,
            firmware,
            dpi,
            dpi_slider,
            polling_slider,
            debounce_slider,
            settings_row: 0,
            status_msg: None,
            should_quit: false,
            throbber_state: ThrobberState::default(),
            _worker: Some(worker),
            battery_rx: Some(battery_rx),
        }
    }

    pub fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status_msg = Some((text.into(), is_error, Instant::now()));
    }

    pub fn tick(&mut self) {
        if let Some(ref rx) = self.battery_rx {
            while let Ok(update) = rx.try_recv() {
                self.battery = Some(update.status);
                self.battery_loading = false;
            }
        }
        if self.battery_loading {
            self.throbber_state.calc_next();
        }
        if let Some((_, _, shown_at)) = &self.status_msg {
            if shown_at.elapsed() >= STATUS_MSG_TTL {
                self.status_msg = None;
            }
        }
    }

    pub fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.settings_row = self.settings_row.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.settings_row = (self.settings_row + 1).min(SETTINGS_ROWS.len() - 1);
            }
            KeyCode::Left | KeyCode::Char('h') => self.adjust_setting(-1),
            KeyCode::Right | KeyCode::Char('l') => self.adjust_setting(1),
            KeyCode::Enter => self.adjust_setting(0),
            _ => {}
        }
    }

    pub fn on_mouse(&mut self, kind: MouseEventKind, _x: u16, _y: u16) {
        match kind {
            MouseEventKind::ScrollUp => {
                self.settings_row = self.settings_row.saturating_sub(1);
            }
            MouseEventKind::ScrollDown => {
                self.settings_row = (self.settings_row + 1).min(SETTINGS_ROWS.len() - 1);
            }
            MouseEventKind::Down(MouseButton::Left) => self.adjust_setting(-1),
            MouseEventKind::Down(MouseButton::Right) => self.adjust_setting(1),
            _ => {}
        }
    }

    fn with_device<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(&Device),
    {
        let Some(dev_arc) = self.device.clone() else {
            self.set_status("No device", true);
            return false;
        };
        match dev_arc.try_lock() {
            Ok(dev) => {
                f(&dev);
                true
            }
            Err(_) => {
                self.set_status("Device busy — try again", true);
                false
            }
        }
    }

    fn adjust_setting(&mut self, delta: i32) {
        match SETTINGS_ROWS[self.settings_row] {
            SettingRow::Dpi => {
                if delta == 0 {
                    let idx = self.dpi_slider.value().round() as usize;
                    let dpi = DPI_VALUES[idx.min(DPI_VALUES.len() - 1)];
                    let mut ok = false;
                    let mut err_msg = String::new();
                    self.with_device(|dev| match protocol::set_dpi(dev, dpi) {
                        Ok(()) => ok = true,
                        Err(e) => err_msg = e.to_string(),
                    });
                    if ok {
                        self.dpi = dpi;
                        self.set_status(format!("DPI → {}", dpi), false);
                    } else if !err_msg.is_empty() {
                        let confirmed_idx =
                            DPI_VALUES.iter().position(|&d| d == self.dpi).unwrap_or(2);
                        self.dpi_slider.set_value(confirmed_idx as f64);
                        self.set_status(format!("Error: {}", err_msg), true);
                    }
                } else {
                    if delta < 0 {
                        self.dpi_slider.decrease(1.0);
                    } else {
                        self.dpi_slider.increase(1.0);
                    }
                    let idx = self.dpi_slider.value().round() as usize;
                    let pending = DPI_VALUES[idx.min(DPI_VALUES.len() - 1)];
                    self.set_status(format!("DPI {} — press Enter to apply", pending), false);
                }
            }

            SettingRow::PollingRate => {
                if delta == 0 {
                    let idx = self.polling_slider.value().round() as usize;
                    let rate = POLLING_RATES[idx.min(POLLING_RATES.len() - 1)];
                    let mut ok = false;
                    let mut err_msg = String::new();
                    self.with_device(|dev| match protocol::set_polling_rate(dev, rate) {
                        Ok(()) => ok = true,
                        Err(e) => err_msg = e.to_string(),
                    });
                    if ok {
                        self.settings.set_polling_rate(rate);
                        self.set_status(format!("Polling → {} Hz", rate.as_hz()), false);
                    } else if !err_msg.is_empty() {
                        let confirmed_idx = POLLING_RATES
                            .iter()
                            .position(|&r| r == self.settings.polling_rate())
                            .unwrap_or(3);
                        self.polling_slider.set_value(confirmed_idx as f64);
                        self.set_status(format!("Error: {}", err_msg), true);
                    }
                } else {
                    if delta < 0 {
                        self.polling_slider.decrease(1.0);
                    } else {
                        self.polling_slider.increase(1.0);
                    }
                    let idx = self.polling_slider.value().round() as usize;
                    let pending = POLLING_RATES[idx.min(POLLING_RATES.len() - 1)];
                    self.set_status(
                        format!("Polling {} Hz — press Enter to apply", pending.as_hz()),
                        false,
                    );
                }
            }

            SettingRow::LiftOffDistance => {
                let idx = LOD_OPTIONS
                    .iter()
                    .position(|&l| l == self.settings.lod())
                    .unwrap_or(1);
                let new = (idx as i32 + delta).clamp(0, LOD_OPTIONS.len() as i32 - 1) as usize;
                self.settings.set_lod(LOD_OPTIONS[new]);
                self.with_device(|dev| {
                    let _ = protocol::set_lod(dev, LOD_OPTIONS[new]);
                });
                self.set_status(format!("LOD → {}", lod_label(LOD_OPTIONS[new])), false);
            }

            SettingRow::Debounce => {
                if delta == 0 {
                    let val = self.debounce_slider.value().round() as u8;
                    let mut ok = false;
                    let mut err_msg = String::new();
                    self.with_device(|dev| match protocol::set_debounce(dev, val) {
                        Ok(()) => ok = true,
                        Err(e) => err_msg = e.to_string(),
                    });
                    if ok {
                        self.settings.debounce_ms = val;
                        self.set_status(format!("Debounce → {} ms", val), false);
                    } else if !err_msg.is_empty() {
                        self.debounce_slider
                            .set_value(self.settings.debounce_ms as f64);
                        self.set_status(format!("Error: {}", err_msg), true);
                    }
                } else {
                    if delta < 0 {
                        self.debounce_slider.decrease(1.0);
                    } else {
                        self.debounce_slider.increase(1.0);
                    }
                    let pending = self.debounce_slider.value().round() as u8;
                    self.set_status(
                        format!("Debounce {} ms — press Enter to apply", pending),
                        false,
                    );
                }
            }

            SettingRow::AngleSnap => {
                self.settings.angle_snap = !self.settings.angle_snap;
                let v = self.settings.angle_snap;
                self.with_device(|dev| {
                    let _ = protocol::set_angle_snap(dev, v);
                });
                self.set_status(format!("Angle Snap → {}", on_off(v)), false);
            }
            SettingRow::RippleControl => {
                self.settings.ripple_control = !self.settings.ripple_control;
                let v = self.settings.ripple_control;
                self.with_device(|dev| {
                    let _ = protocol::set_ripple_control(dev, v);
                });
                self.set_status(format!("Ripple → {}", on_off(v)), false);
            }
            SettingRow::MotionSync => {
                self.settings.motion_sync = !self.settings.motion_sync;
                let v = self.settings.motion_sync;
                self.with_device(|dev| {
                    let _ = protocol::set_motion_sync(dev, v);
                });
                self.set_status(format!("Motion Sync → {}", on_off(v)), false);
            }
            SettingRow::TurboMode => {
                self.settings.turbo_mode = !self.settings.turbo_mode;
                let v = self.settings.turbo_mode;
                self.with_device(|dev| {
                    let _ = protocol::set_turbo_mode(dev, v);
                });
                self.set_status(format!("Turbo → {}", on_off(v)), false);
            }
            SettingRow::NotificationThreshold => {
                if delta == 0 {
                    return;
                }
                let new =
                    (self.settings.notification_threshold as i32 + delta * 5).clamp(5, 50) as u8;
                self.settings.notification_threshold = new;
                self.set_status(format!("Alert threshold → {}%", new), false);
            }
            SettingRow::BatteryInterval => {
                if delta == 0 {
                    return;
                }
                let idx = INTERVAL_OPTIONS
                    .iter()
                    .position(|&s| s == self.settings.battery_interval_secs)
                    .unwrap_or(1);
                let new = (idx as i32 + delta).clamp(0, INTERVAL_OPTIONS.len() as i32 - 1) as usize;
                self.settings.battery_interval_secs = INTERVAL_OPTIONS[new];
                self.set_status(
                    format!("Battery interval → {}s", INTERVAL_OPTIONS[new]),
                    false,
                );
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Err(e) = self.settings.save() {
            log::warn!("Failed to save settings on exit: {}", e);
        }
    }
}

fn apply_settings_to_device(dev: &Device, settings: &Settings) {
    let _ = protocol::set_polling_rate(dev, settings.polling_rate());
    let _ = protocol::set_lod(dev, settings.lod());
    let _ = protocol::set_debounce(dev, settings.debounce_ms);
    let _ = protocol::set_angle_snap(dev, settings.angle_snap);
    let _ = protocol::set_ripple_control(dev, settings.ripple_control);
    let _ = protocol::set_motion_sync(dev, settings.motion_sync);
    let _ = protocol::set_turbo_mode(dev, settings.turbo_mode);
}

pub fn on_off(v: bool) -> &'static str {
    if v {
        "ON"
    } else {
        "OFF"
    }
}

pub fn lod_label(lod: LiftOffDistance) -> String {
    match lod {
        LiftOffDistance::Low => "0.7 mm".into(),
        LiftOffDistance::Medium => "1 mm".into(),
        LiftOffDistance::High => "2 mm".into(),
    }
}

pub fn run() -> Result<()> {
    use std::io::Write;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.flush()?;

    execute!(
        stdout,
        terminal::Clear(terminal::ClearType::All),
        EnterAlternateScreen,
        EnableMouseCapture,
        Hide,
        event::EnableBracketedPaste,
    )?;
    stdout.flush()?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new();
    let tick = Duration::from_millis(250);

    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| super::render::ui(f, &mut app))?;

            if event::poll(tick)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        app.on_key(key.code, key.modifiers);
                    }
                    Event::Mouse(mouse) => {
                        app.on_mouse(mouse.kind, mouse.column, mouse.row);
                    }
                    Event::Paste(_) => {}
                    _ => {}
                }
            }

            app.tick();
            if app.should_quit {
                break;
            }
        }
        Ok(())
    })();

    disable_raw_mode()?;
    let stdout = terminal.backend_mut();
    execute!(
        stdout,
        LeaveAlternateScreen,
        DisableMouseCapture,
        Show,
        event::DisableBracketedPaste,
    )?;
    terminal.show_cursor()?;

    result
}
