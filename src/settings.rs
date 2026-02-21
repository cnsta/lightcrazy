use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::device::protocol::{LiftOffDistance, PollingRate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub polling_rate: u8, // Store as u8 for serde compatibility
    pub lod: u8,          // 0=Low, 1=Medium, 2=High
    pub debounce_ms: u8,
    pub angle_snap: bool,
    pub ripple_control: bool,
    pub motion_sync: bool,
    pub turbo_mode: bool,
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
        }
    }
}

impl Settings {
    // Load settings from config file, or create default if not exists
    pub fn load() -> Self {
        match Self::try_load() {
            Ok(settings) => settings,
            Err(e) => {
                eprintln!("Warning: Failed to load settings ({}), using defaults", e);
                let default = Self::default();
                // Try to save defaults
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

    // Save settings to config file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(self).context("Failed to serialize settings")?;

        fs::write(&path, json).with_context(|| format!("Failed to write {}", path.display()))?;

        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not determine config directory")?;

        Ok(config_dir.join("lightcrazy").join("settings.json"))
    }

    // Convert to PollingRate enum
    pub fn polling_rate(&self) -> PollingRate {
        match self.polling_rate {
            0x08 => PollingRate::Hz125,
            0x04 => PollingRate::Hz250,
            0x02 => PollingRate::Hz500,
            0x01 => PollingRate::Hz1000,
            0x10 => PollingRate::Hz2000,
            0x20 => PollingRate::Hz4000,
            0x40 => PollingRate::Hz8000,
            _ => PollingRate::Hz2000, // Default fallback
        }
    }

    // Convert to LiftOffDistance enum
    pub fn lod(&self) -> LiftOffDistance {
        match self.lod {
            0 => LiftOffDistance::Low,
            1 => LiftOffDistance::Medium,
            2 => LiftOffDistance::High,
            _ => LiftOffDistance::Medium, // Default fallback
        }
    }

    // Update from PollingRate enum
    pub fn set_polling_rate(&mut self, rate: PollingRate) {
        self.polling_rate = rate as u8;
    }

    // Update from LiftOffDistance enum
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
        let settings = Settings::default();
        assert_eq!(settings.polling_rate, 0x10); // 2000 Hz
        assert_eq!(settings.lod, 1); // Medium
        assert_eq!(settings.debounce_ms, 2);
        assert!(!settings.angle_snap);
    }

    #[test]
    fn test_polling_rate_conversion() {
        let mut settings = Settings::default();
        settings.set_polling_rate(PollingRate::Hz8000);
        assert_eq!(settings.polling_rate, 0x40);
        assert_eq!(settings.polling_rate(), PollingRate::Hz8000);
    }
}
