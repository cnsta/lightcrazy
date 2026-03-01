use log::{info, warn};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use crate::device::{
    protocol::{self, MouseStatus},
    transport::Device,
};

// Handle to the background battery worker.
// Dropping this signals the worker thread to stop.
pub struct BatteryWorker {
    running: Arc<AtomicBool>,
}

impl Drop for BatteryWorker {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
    }
}

// A battery update message sent from worker to the UI.
#[derive(Debug, Clone)]
pub struct BatteryUpdate {
    pub status: MouseStatus,
}

impl BatteryWorker {
    // Spawn the battery polling worker.
    //
    // "device" is the same "Arc<Mutex<Device>>" that "App" holds. The worker
    // uses "try_lock()" on each poll, if the UI thread is currently sending a
    // command, the poll is skipped silently. This ensures the user-initiated
    // setting command always wins over a background battery read.
    //
    // If "device" is "None" (no mouse connected), the thread exits immediately
    // and the receiver will never receive an update.
    pub fn spawn(
        interval_secs: u64,
        device: Option<Arc<Mutex<Device>>>,
    ) -> (Self, mpsc::Receiver<BatteryUpdate>) {
        let (tx, rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        thread::spawn(move || {
            let device = match device {
                Some(d) => d,
                None => {
                    info!("Battery worker: no device connected, exiting");
                    return;
                }
            };

            info!("Battery worker started (interval: {}s)", interval_secs);

            // Immediate first fetch, resolves the TUI loading spinner right away.
            poll_battery(&device, &tx);

            let poll_interval = Duration::from_secs(interval_secs);
            let tick = Duration::from_millis(200);
            let mut accumulated = Duration::ZERO;
            // Backoff used when the device has re-enumerated. We sleep for a
            // few seconds to give the kernel time to stabilise the new hidraw
            // node before attempting another HID read. During this window the
            // tray icon will simply show the last known battery level.
            let mut backoff_remaining = Duration::ZERO;
            const REENUMERATION_BACKOFF: Duration = Duration::from_secs(5);

            while running_clone.load(Ordering::Acquire) {
                thread::sleep(tick);

                if backoff_remaining > Duration::ZERO {
                    backoff_remaining = backoff_remaining.saturating_sub(tick);
                    continue;
                }

                accumulated += tick;

                if accumulated >= poll_interval {
                    accumulated = Duration::ZERO;
                    if poll_battery(&device, &tx) == PollOutcome::DeviceGone {
                        info!(
                            "Worker: backing off {}s after device re-enumeration",
                            REENUMERATION_BACKOFF.as_secs()
                        );
                        backoff_remaining = REENUMERATION_BACKOFF;
                    }
                }
            }

            info!("Battery worker shutting down");
        });

        (Self { running }, rx)
    }
}

/// Return value from poll_battery indicating whether the device appears to
/// have disconnected (re-enumerated) and we should back off.
#[derive(PartialEq)]
enum PollOutcome {
    Ok,
    Busy,
    /// HID error that likely indicates the device has re-enumerated.
    /// The caller should wait before retrying to let the kernel stabilise.
    DeviceGone,
}

// Attempt a battery read using the shared device handle.
//
// Uses "try_lock()": if the UI thread currently holds the mutex, the poll is
// skipped. Returns DeviceGone when the read fails in a way that suggests the
// dongle has re-enumerated (which happens whenever the mouse enters or exits
// charging mode via any USB connection, even an external power supply).
fn poll_battery(device: &Arc<Mutex<Device>>, tx: &mpsc::Sender<BatteryUpdate>) -> PollOutcome {
    match device.try_lock() {
        Ok(dev) => match protocol::get_mouse_battery(&dev) {
            Ok(status) => {
                info!(
                    "Worker: battery {}%{}",
                    status.battery_level,
                    if status.is_charging { " ⚡" } else { "" }
                );
                // Receiver gone means the TUI closed — silently ignore.
                let _ = tx.send(BatteryUpdate { status });
                PollOutcome::Ok
            }
            Err(e) => {
                let msg = e.to_string();
                // "No such device", "I/O error", or "HID read failed" after
                // the dongle re-enumerates will appear here.
                if msg.contains("No such device")
                    || msg.contains("I/O error")
                    || msg.contains("HID read failed")
                    || msg.contains("Read timeout")
                {
                    warn!("Worker: device appears to have re-enumerated: {}", e);
                    PollOutcome::DeviceGone
                } else {
                    warn!("Worker: battery read failed: {}", e);
                    PollOutcome::Ok
                }
            }
        },
        Err(_) => {
            info!("Worker: device busy (UI command in progress) — skipping poll");
            PollOutcome::Busy
        }
    }
}
