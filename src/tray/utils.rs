use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::RecvTimeoutError,
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use ksni::blocking::TrayMethods;
use log::{info, warn};

use crate::{
    device::{BatteryEvent, BatteryWorker, DeviceSource, WorkerConfig},
    tray::menu::{BatteryContext, BatteryTray},
};

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

    let (ctx, refresh_flag, handle) = init_tray()?;
    let running = Arc::new(AtomicBool::new(true));

    start_battery_event_consumer(
        ctx.clone(),
        refresh_flag.clone(),
        running.clone(),
        handle.clone(),
    );

    Ok(TrayServiceHandle { running })
}

pub fn start_tray_service() -> anyhow::Result<()> {
    info!("Starting lightcrazy battery tray service");

    let (ctx, refresh_flag, handle) = init_tray()?;
    let running = Arc::new(AtomicBool::new(true));

    {
        let running = running.clone();
        ctrlc::set_handler(move || {
            info!("Received shutdown signal");
            running.store(false, Ordering::Release);
        })
        .context("Failed to set signal handler")?;
    }

    start_battery_event_consumer(
        ctx.clone(),
        refresh_flag.clone(),
        running.clone(),
        handle.clone(),
    );

    while running.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(100));
    }

    info!("Tray service shutting down");
    Ok(())
}

/// Block until `org.kde.StatusNotifierWatcher` is present on the session bus,
/// or until `timeout` elapses. Called before every `spawn()` so that
/// `RegisterStatusNotifierItem` is never sent before anything is listening.
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
                "StatusNotifierWatcher not available after {}s — proceeding anyway; \
                 watcher_offline/respawn will handle it if the tray does not register",
                timeout.as_secs()
            );
            return;
        }
        thread::sleep(Duration::from_millis(500));
    }
}

/// Initialise the tray icon. Returns the shared battery context, the
/// refresh-request flag (consumed by the worker), and the ksni handle.
///
/// The refresh_flag is created here so it can be shared between the tray
/// (whose Refresh Now menu sets it) and the worker (which clears it).
fn init_tray() -> anyhow::Result<(
    Arc<Mutex<BatteryContext>>,
    Arc<AtomicBool>,
    ksni::blocking::Handle<BatteryTray>,
)> {
    // Wait for the SNI watcher before spawning so RegisterStatusNotifierItem
    // is never issued before anything is listening.
    wait_for_watcher(Duration::from_secs(30));

    let ctx = Arc::new(Mutex::new(BatteryContext::default()));
    let refresh_flag = Arc::new(AtomicBool::new(false));

    // No blocking read here, the device/dongle is often not ready at
    // graphical-session.target time.
    let tray = BatteryTray {
        ctx: ctx.clone(),
        refresh_flag: refresh_flag.clone(),
    };
    let handle = tray
        .assume_sni_available(true)
        .spawn()
        .context("Failed to spawn tray icon")?;

    info!("Tray icon spawned successfully");
    Ok((ctx, refresh_flag, handle))
}

/// Spawn the unified battery worker (owned-device mode) and a thread that
/// translates `BatteryEvent`s into tray-context updates and notifications.
fn start_battery_event_consumer(
    ctx: Arc<Mutex<BatteryContext>>,
    refresh_flag: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    handle: ksni::blocking::Handle<BatteryTray>,
) {
    let interval_secs = crate::settings::Settings::load().battery_interval_secs;
    info!("Battery monitoring: checking every {}s", interval_secs);

    let config = WorkerConfig {
        interval: Duration::from_secs(interval_secs),
        disconnect_backoff: Duration::from_secs(5),
        device_source: DeviceSource::Owned,
        refresh_flag,
    };

    thread::spawn(move || {
        let (worker, events) = BatteryWorker::spawn(config);
        // Hold the worker alive for the lifetime of this thread. Its Drop
        // joins the worker thread, so it must stay in scope until we exit.
        let _worker_guard = worker;

        while running.load(Ordering::Acquire) {
            // 500ms timeout so we re-check `running` reasonably often even
            // when the worker is idle (e.g. mouse asleep for hours).
            match events.recv_timeout(Duration::from_millis(500)) {
                Ok(BatteryEvent::Update(status)) => {
                    handle_battery_update(&ctx, &handle, status);
                }
                Ok(BatteryEvent::Asleep) => {
                    log::debug!("Mouse asleep");
                }
                Ok(BatteryEvent::Disconnected) => {
                    info!("Device unreachable; waiting for it to come back");
                }
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    warn!("Battery worker channel closed; consumer exiting");
                    break;
                }
            }
        }
        info!("Battery event consumer thread exiting");
    });
}

/// Apply a successful battery reading to the tray context and fire a
/// notification if the threshold was crossed.
fn handle_battery_update(
    ctx: &Arc<Mutex<BatteryContext>>,
    handle: &ksni::blocking::Handle<BatteryTray>,
    status: crate::device::MouseStatus,
) {
    let threshold = crate::settings::Settings::load().notification_threshold;

    info!(
        "Battery: {}%{}",
        status.battery_level,
        if status.is_charging { " ⚡" } else { "" }
    );

    let (should_notify, level) = {
        let mut ctx_g = ctx.lock().unwrap();
        let old_level = ctx_g.battery.map(|(l, _)| l).unwrap_or(100);
        ctx_g.battery = Some((status.battery_level, status.is_charging));
        let should = ctx_g.notifications.should_notify_low_battery(
            status.battery_level,
            old_level,
            threshold,
            status.is_charging,
        );
        (should, status.battery_level)
    };

    if should_notify {
        let mut ctx_g = ctx.lock().unwrap();
        if let Err(e) = ctx_g.notifications.send_low_battery(level) {
            warn!("Failed to send low-battery notification: {}", e);
        }
    }

    handle.update(|_| {});
}
