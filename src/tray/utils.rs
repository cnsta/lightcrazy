use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use ksni::blocking::TrayMethods;
use log::{error, info, warn};

use crate::tray::menu::{BatteryContext, BatteryTray};

static NEEDS_RESPAWN: AtomicBool = AtomicBool::new(false);

/// Set by `watcher_online` in menu.rs each time ksni confirms the tray is
/// registered with the StatusNotifierWatcher. The watchdog uses this to detect
/// the boot-race case where `spawn()` succeeded but the watcher wasn't ready
/// yet, meaning `watcher_online` is never called and `watcher_offline` never
/// fires, leaving the tray silently unregistered with no other signal to act on.
static WATCHER_ONLINE: AtomicBool = AtomicBool::new(false);

pub fn signal_respawn_needed() {
    NEEDS_RESPAWN.store(true, Ordering::Release);
}

pub fn signal_watcher_online() {
    WATCHER_ONLINE.store(true, Ordering::Release);
}

pub struct TrayServiceHandle {
    running: Arc<AtomicBool>,
}

impl Drop for TrayServiceHandle {
    fn drop(&mut self) {
        info!("TrayServiceHandle dropped, stopping background tray threads");
        self.running.store(false, Ordering::Release);
    }
}

pub fn start_tray_background() -> anyhow::Result<TrayServiceHandle> {
    info!("Starting lightcrazy tray in background");

    let (ctx, handle) = init_tray()?;
    let running = Arc::new(AtomicBool::new(true));

    start_battery_monitor(ctx.clone(), running.clone(), handle.clone());
    start_tray_watchdog(ctx, running.clone(), handle);

    Ok(TrayServiceHandle { running })
}

pub fn start_tray_service() -> anyhow::Result<()> {
    info!("Starting lightcrazy battery tray service");

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

    start_battery_monitor(ctx.clone(), running.clone(), handle.clone());
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

    // No blocking read here, the device/dongle is often not ready at
    // graphical-session.target time. The monitor thread handles the initial
    // read with short-interval retries so startup is never delayed.
    let tray = BatteryTray { ctx: ctx.clone() };
    let handle = tray.spawn().context("Failed to spawn tray icon")?;

    info!("Tray icon spawned successfully");
    Ok((ctx, handle))
}

/// Background thread that periodically reads battery and updates the tray icon.
fn start_battery_monitor(
    ctx: Arc<Mutex<BatteryContext>>,
    running: Arc<AtomicBool>,
    handle: ksni::blocking::Handle<BatteryTray>,
) {
    const STARTUP_RETRY_SECS: u64 = 5;

    // Read interval from settings so any change made in the TUI takes
    // effect on the next tray restart (interval is captured once per session).
    let interval_secs = crate::settings::Settings::load().battery_interval_secs;

    info!(
        "Battery monitoring: checking every {}s (startup retry every {}s)",
        interval_secs, STARTUP_RETRY_SECS
    );

    thread::spawn(move || {
        let mut initial_read_done = false;

        while running.load(Ordering::Acquire) {
            // Startup phase: short retries until we have one successful read.
            // Normal phase: long interval between polls.
            let sleep_secs = if initial_read_done {
                interval_secs
            } else {
                STARTUP_RETRY_SECS
            };
            thread::sleep(Duration::from_secs(sleep_secs));

            if !running.load(Ordering::Acquire) {
                break;
            }

            if crate::lock::ui_is_active() {
                info!("TUI active, skipping battery poll");
                continue;
            }

            let _dev_lock = match crate::lock::acquire_device_lock() {
                Ok(lock) => lock,
                Err(e) => {
                    warn!("Could not acquire device lock for battery poll: {}", e);
                    continue;
                }
            };

            let tray = BatteryTray { ctx: ctx.clone() };
            match tray.update_battery() {
                Ok(()) => {
                    // Notify the ksni service that properties have changed so it
                    // pushes updated icon, tooltip, and menu to the host immediately.
                    // Without this call, the host sees no change signal and keeps
                    // displaying whatever it cached at spawn time.
                    handle.update(|_| {});

                    if !initial_read_done {
                        info!(
                            "Initial battery read succeeded, switching to {}s interval",
                            interval_secs
                        );
                        initial_read_done = true;
                    }
                }
                Err(e) => {
                    if initial_read_done {
                        warn!("Battery update failed: {}", e);
                    } else {
                        info!(
                            "Startup battery read not ready yet ({}), retrying in {}s",
                            e, STARTUP_RETRY_SECS
                        );
                    }
                }
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
    // How long to wait before concluding the watcher never registered us.
    // Long enough to cover slow graphical-session startups without being
    // so short that a legitimate slow boot triggers a false respawn.
    const BOOT_RACE_GRACE_SECS: u64 = 20;

    thread::spawn(move || {
        let mut consecutive_failures = 0u32;
        let mut handle = Some(initial_handle);
        let started_at = Instant::now();

        while running.load(Ordering::Acquire) {
            // Boot-race detection: if the grace period has elapsed and
            // watcher_online has still never fired, the initial RegisterStatus-
            // NotifierItem call was lost (watcher wasn't up yet, ksni's internal
            // retry didn't fire, and watcher_offline was never called because we
            // were never "on"). Trigger a fresh spawn to re-register cleanly.
            if handle.is_some()
                && !WATCHER_ONLINE.load(Ordering::Acquire)
                && started_at.elapsed().as_secs() > BOOT_RACE_GRACE_SECS
            {
                info!(
                    "watcher_online not seen within {}s of startup — \
                     likely boot-race, triggering respawn",
                    BOOT_RACE_GRACE_SECS
                );
                NEEDS_RESPAWN.store(true, Ordering::Release);
            }

            if NEEDS_RESPAWN.load(Ordering::Acquire) || handle.is_none() {
                info!("Tray disconnected, attempting respawn...");
                NEEDS_RESPAWN.store(false, Ordering::Release);
                // Reset so the grace-period check applies fresh to the new handle.
                WATCHER_ONLINE.store(false, Ordering::Release);

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
