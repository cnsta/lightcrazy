use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::device::protocol::{LiftOffDistance, PollingRate};

/// Battery check interval presets in seconds.
/// Exposed as selectable steps in the TUI.
pub const INTERVAL_OPTIONS: [u64; 5] = [30, 60, 120, 300, 600];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub polling_rate: u8, // stored as protocol byte for serde compatibility
    pub lod: u8,          // 0=Low, 1=Medium, 2=High
    pub debounce_ms: u8,
    pub angle_snap: bool,
    pub ripple_control: bool,
    pub motion_sync: bool,
    pub turbo_mode: bool,
    #[serde(default = "default_notification_threshold")]
    pub notification_threshold: u8,
    #[serde(default = "default_battery_interval_secs")]
    pub battery_interval_secs: u64,
}

fn default_notification_threshold() -> u8 {
    20
}
fn default_battery_interval_secs() -> u64 {
    60
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            polling_rate: 0x10, // 2000 Hz
            lod: 1,             // Medium (1mm)
            debounce_ms: 2,
            angle_snap: false,
            ripple_control: false,
            motion_sync: false,
            turbo_mode: false,
            notification_threshold: default_notification_threshold(),
            battery_interval_secs: default_battery_interval_secs(),
        }
    }
}

impl Settings {
    /// Load from disk, falling back to defaults on any error.
    /// Uses `log::warn!` rather than `eprintln!` so it is safe to call
    /// after the terminal has entered raw mode.
    pub fn load() -> Self {
        match Self::try_load() {
            Ok(mut s) => {
                s.clamp();
                s
            }
            Err(e) => {
                log::warn!("Failed to load settings ({}), using defaults", e);
                let default = Self::default();
                let _ = default.save();
                default
            }
        }
    }

    fn try_load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let settings: Self =
            serde_json::from_str(&contents).context("Failed to parse settings JSON")?;
        Ok(settings)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self).context("Failed to serialize settings")?;
        fs::write(&path, json).with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(())
    }

    /// Clamp fields that have valid ranges, so corrupt or hand-edited JSON
    /// can't put the app into an unrecoverable state.
    fn clamp(&mut self) {
        self.notification_threshold = self.notification_threshold.clamp(5, 50);
        if !INTERVAL_OPTIONS.contains(&self.battery_interval_secs) {
            self.battery_interval_secs = default_battery_interval_secs();
        }
    }

    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not determine config directory")?;
        Ok(config_dir.join("lightcrazy").join("settings.json"))
    }

    pub fn polling_rate(&self) -> PollingRate {
        match self.polling_rate {
            0x08 => PollingRate::Hz125,
            0x04 => PollingRate::Hz250,
            0x02 => PollingRate::Hz500,
            0x01 => PollingRate::Hz1000,
            0x10 => PollingRate::Hz2000,
            0x20 => PollingRate::Hz4000,
            0x40 => PollingRate::Hz8000,
            _ => PollingRate::Hz2000,
        }
    }

    pub fn lod(&self) -> LiftOffDistance {
        match self.lod {
            0 => LiftOffDistance::Low,
            1 => LiftOffDistance::Medium,
            2 => LiftOffDistance::High,
            _ => LiftOffDistance::Medium,
        }
    }

    pub fn set_polling_rate(&mut self, rate: PollingRate) {
        self.polling_rate = rate as u8;
    }

    pub fn set_lod(&mut self, lod: LiftOffDistance) {
        self.lod = match lod {
            LiftOffDistance::Low => 0,
            LiftOffDistance::Medium => 1,
            LiftOffDistance::High => 2,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let s = Settings::default();
        assert_eq!(s.polling_rate, 0x10);
        assert_eq!(s.lod, 1);
        assert_eq!(s.debounce_ms, 2);
        assert!(!s.angle_snap);
        assert_eq!(s.notification_threshold, 20);
        assert_eq!(s.battery_interval_secs, 60);
    }

    #[test]
    fn test_polling_rate_conversion() {
        let mut s = Settings::default();
        s.set_polling_rate(PollingRate::Hz8000);
        assert_eq!(s.polling_rate, 0x40);
        assert_eq!(s.polling_rate(), PollingRate::Hz8000);
    }

    #[test]
    fn test_clamp() {
        let mut s = Settings::default();
        s.notification_threshold = 99;
        s.battery_interval_secs = 7777;
        s.clamp();
        assert_eq!(s.notification_threshold, 50);
        assert_eq!(s.battery_interval_secs, 60); // reset to default
    }

    #[test]
    fn test_interval_options_contain_default() {
        assert!(INTERVAL_OPTIONS.contains(&default_battery_interval_secs()));
    }
}
