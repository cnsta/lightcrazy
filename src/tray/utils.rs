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

pub fn signal_respawn_needed() {
    NEEDS_RESPAWN.store(true, Ordering::Release);
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

/// Block until `org.kde.StatusNotifierWatcher` is present on the session bus,
/// or until `timeout` elapses. Called before every `spawn()` so that
/// `RegisterStatusNotifierItem` is never sent before anything is listening.
///
/// Without this, apps that start before the bar's SNI watcher is ready send
/// their registration into the void: the call returns no error but the watcher
/// never records the item, `watcher_offline` is never called (we were never
/// online to begin with), and the icon silently doesn't appear.
///
/// Note: ksni's `watcher_online` callback only fires when the watcher comes
/// back *after* being offline, it does not fire on a clean first registration
/// where the watcher was already running, so it cannot be used to detect this
/// race after the fact.
fn wait_for_watcher(timeout: Duration) {
    use zbus::blocking::Connection;

    let Ok(conn) = Connection::session() else {
        warn!("Could not connect to session D-Bus; proceeding without watcher check");
        return;
    };
    let Ok(dbus) = zbus::blocking::fdo::DBusProxy::new(&conn) else {
        warn!("Could not create DBus proxy; proceeding without watcher check");
        return;
    };

    let start = Instant::now();
    loop {
        match dbus.list_names() {
            Ok(names)
                if names
                    .iter()
                    .any(|n| n.as_str() == "org.kde.StatusNotifierWatcher") =>
            {
                info!("StatusNotifierWatcher is available");
                return;
            }
            Ok(_) => {}
            Err(e) => warn!("D-Bus list_names failed: {}", e),
        }
        if start.elapsed() >= timeout {
            warn!(
                "StatusNotifierWatcher not available after {}s — proceeding anyway;                  watcher_offline/respawn will handle it if the tray does not register",
                timeout.as_secs()
            );
            return;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn init_tray() -> anyhow::Result<(
    Arc<Mutex<BatteryContext>>,
    ksni::blocking::Handle<BatteryTray>,
)> {
    // Wait for the SNI watcher before spawning so RegisterStatusNotifierItem
    // is never issued before anything is listening. The wait returns immediately
    // if the watcher is already up (the common case), so there is no cost on a
    // healthy boot. 30s covers even very slow graphical-session startups.
    wait_for_watcher(Duration::from_secs(30));

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
    thread::spawn(move || {
        let mut consecutive_failures = 0u32;
        let mut handle = Some(initial_handle);

        while running.load(Ordering::Acquire) {
            if NEEDS_RESPAWN.load(Ordering::Acquire) || handle.is_none() {
                info!("Tray disconnected, attempting respawn...");
                NEEDS_RESPAWN.store(false, Ordering::Release);

                // Wait for the watcher before re-registering. If the bar
                // restarted and took its watcher down with it, spawning
                // before it re-advertises drops the registration silently.
                wait_for_watcher(Duration::from_secs(30));

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
