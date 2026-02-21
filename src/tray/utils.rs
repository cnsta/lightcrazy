use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use anyhow::Context;
use ksni::blocking::TrayMethods;
use log::{error, info, warn};

use crate::tray::menu::{BatteryContext, BatteryTray};

static NEEDS_RESPAWN: AtomicBool = AtomicBool::new(false);

pub fn signal_respawn_needed() {
    NEEDS_RESPAWN.store(true, Ordering::Release);
}

pub struct TrayServiceHandle {
    running: Arc<AtomicBool>,
}

impl Drop for TrayServiceHandle {
    fn drop(&mut self) {
        info!("TrayServiceHandle dropped — stopping background tray threads");
        self.running.store(false, Ordering::Release);
    }
}

pub fn start_tray_background() -> anyhow::Result<TrayServiceHandle> {
    info!("Starting LightCrazy tray in background");

    let (ctx, handle) = init_tray()?;
    let running = Arc::new(AtomicBool::new(true));

    start_battery_monitor(ctx.clone(), running.clone());
    start_tray_watchdog(ctx, running.clone(), handle);

    Ok(TrayServiceHandle { running })
}

pub fn start_tray_service() -> anyhow::Result<()> {
    info!("Starting LightCrazy battery tray service");

    let (ctx, handle) = init_tray()?;
    let running = Arc::new(AtomicBool::new(true));

    {
        let running = running.clone();
        ctrlc::set_handler(move || {
            info!("Received shutdown signal");
            running.store(false, Ordering::Release);
        })
        .context("Failed to set signal handler")?;
    }

    start_battery_monitor(ctx.clone(), running.clone());
    start_tray_watchdog(ctx, running.clone(), handle);

    while running.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(100));
    }

    info!("Tray service shutting down");
    Ok(())
}

fn init_tray() -> anyhow::Result<(
    Arc<Mutex<BatteryContext>>,
    ksni::blocking::Handle<BatteryTray>,
)> {
    let ctx = Arc::new(Mutex::new(BatteryContext::default()));

    {
        let tray = BatteryTray { ctx: ctx.clone() };
        // Hold the device lock during the initial read so that if this process
        // was started alongside a TUI (--options with no prior tray), the TUI's
        // App::new() waits for this read to complete before querying the device.
        let _dev_lock = crate::lock::acquire_device_lock()
            .context("Failed to acquire device lock for initial battery read")?;
        if let Err(e) = tray.update_battery() {
            warn!("Initial battery read failed: {}", e);
        }
    }

    let tray = BatteryTray { ctx: ctx.clone() };
    let handle = tray.spawn().context("Failed to spawn tray icon")?;

    info!("Tray icon spawned successfully");
    Ok((ctx, handle))
}

// Background thread that periodically reads battery and updates the tray icon.
//
// Skips the cycle if the TUI is open (UI lock held) to avoid inter-process
// HID contention. When it does poll, holds the cross-process device lock for
// the duration so the TUI startup cannot open the device mid-protocol.
fn start_battery_monitor(ctx: Arc<Mutex<BatteryContext>>, running: Arc<AtomicBool>) {
    let interval_secs = std::env::var("PULSAR_CHECK_INTERVAL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60u64);

    info!("Battery monitoring: checking every {}s", interval_secs);

    thread::spawn(move || {
        while running.load(Ordering::Acquire) {
            thread::sleep(Duration::from_secs(interval_secs));

            if !running.load(Ordering::Acquire) {
                break;
            }

            // TUI is open, skip to avoid fighting over the device.
            if crate::lock::ui_is_active() {
                info!("TUI active — skipping battery poll");
                continue;
            }

            // Hold the device lock for the entire poll so the TUI can't
            // start its App::new() initialisation mid-protocol.
            let _dev_lock = match crate::lock::acquire_device_lock() {
                Ok(lock) => lock,
                Err(e) => {
                    warn!("Could not acquire device lock for battery poll: {}", e);
                    continue;
                }
            };

            let tray = BatteryTray { ctx: ctx.clone() };
            if let Err(e) = tray.update_battery() {
                warn!("Battery update failed: {}", e);
            }
        }
        info!("Battery monitoring thread exiting");
    });
}

fn start_tray_watchdog(
    ctx: Arc<Mutex<BatteryContext>>,
    running: Arc<AtomicBool>,
    initial_handle: ksni::blocking::Handle<BatteryTray>,
) {
    thread::spawn(move || {
        let mut consecutive_failures = 0u32;
        let mut handle = Some(initial_handle);

        while running.load(Ordering::Acquire) {
            if NEEDS_RESPAWN.load(Ordering::Acquire) || handle.is_none() {
                info!("Tray disconnected, attempting respawn...");
                NEEDS_RESPAWN.store(false, Ordering::Release);

                let tray = BatteryTray { ctx: ctx.clone() };
                match tray.spawn() {
                    Ok(new_handle) => {
                        info!("Successfully respawned tray");
                        handle = Some(new_handle);
                        consecutive_failures = 0;
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        error!(
                            "Failed to respawn tray (attempt {}): {}",
                            consecutive_failures, e
                        );
                        let delay = (2_u64.pow(consecutive_failures.min(4))).min(30);
                        thread::sleep(Duration::from_secs(delay));
                    }
                }
            }
            thread::sleep(Duration::from_secs(2));
        }
        info!("Tray watchdog thread exiting");
    });
}
