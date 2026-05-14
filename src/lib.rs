//! # LightCrazy: Pulsar X2 Mouse Control
//!
//! Linux control software for the Pulsar X2 CrazyLight mouse.
//!
//! ## Example
//!
//! ```no_run
//! use lightcrazy::device::{transport::Device, protocol};
//! use lightcrazy::acquire_device_lock;
//!
//! fn main() -> anyhow::Result<()> {
//!     // Coordinate with any running tray/TUI before touching the device.
//!     let _lock = acquire_device_lock()?;
//!     let device = Device::open()?;
//!     let battery = protocol::get_mouse_battery(&device)?;
//!     println!("Battery: {}%", battery.battery_level);
//!     protocol::set_dpi(&device, 1600)?;
//!     Ok(())
//! }
//! ```

pub mod device;
pub mod lock;
pub mod settings;
pub mod tray;
pub mod ui;

pub use device::protocol::{BatteryReadError, MouseStatus, DPI_MAX, DPI_MIN};
pub use device::transport::Device;
pub use device::{BatteryEvent, BatteryWorker, DeviceSource, WorkerConfig};
pub use lock::{
    acquire_device_lock, acquire_tray_lock, acquire_ui_lock, tray_is_running,
    try_acquire_device_lock, ui_is_active, LockGuard,
};
pub use settings::Settings;
