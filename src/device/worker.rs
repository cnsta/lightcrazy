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

            while running_clone.load(Ordering::Acquire) {
                thread::sleep(tick);
                accumulated += tick;

                if accumulated >= poll_interval {
                    accumulated = Duration::ZERO;
                    poll_battery(&device, &tx);
                }
            }

            info!("Battery worker shutting down");
        });

        (Self { running }, rx)
    }
}

// Attempt a battery read using the shared device handle.
//
// Uses "try_lock()": if the UI thread currently holds the mutex (e.g. it is
// mid-way through sending a DPI command), the poll is skipped. The next
// scheduled poll will try again. This prevents the ~2s battery init sequence
// from blocking user interactions.
fn poll_battery(device: &Arc<Mutex<Device>>, tx: &mpsc::Sender<BatteryUpdate>) {
    match device.try_lock() {
        Ok(dev) => match protocol::get_mouse_battery(&dev) {
            Ok(status) => {
                info!(
                    "Worker: battery {}%{}",
                    status.battery_level,
                    if status.is_charging { " ⚡" } else { "" }
                );
                // Receiver gone means the TUI closed, the thread will exit on the next
                // Ordering::Acquire check anyway, so just silently ignore the error.
                let _ = tx.send(BatteryUpdate { status });
            }
            Err(e) => warn!("Worker: battery read failed: {}", e),
        },
        Err(_) => {
            info!("Worker: device busy (UI command in progress) — skipping poll");
        }
    }
}
