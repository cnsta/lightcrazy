use log::{debug, info, warn};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::device::{
    protocol::{self, BatteryReadError, MouseStatus},
    transport::Device,
};

/// Events emitted by the worker to whichever component spawned it.
#[derive(Debug, Clone)]
pub enum BatteryEvent {
    Update(MouseStatus),
    Asleep,
    Disconnected,
}

/// How the worker obtains a `Device` for each poll.
pub enum DeviceSource {
    Shared(Arc<Mutex<Device>>),
    Owned,
}

/// Config for `BatteryWorker::spawn`.
pub struct WorkerConfig {
    /// Base poll interval. Adaptive backoff multiplies this by up to 8× when
    /// the mouse is asleep, so the effective ceiling is `8 × interval`.
    pub interval: Duration,
    pub disconnect_backoff: Duration,
    pub device_source: DeviceSource,
    pub refresh_flag: Arc<AtomicBool>,
}

/// Handle to the background worker. Dropping joins the thread.
pub struct BatteryWorker {
    running: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for BatteryWorker {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl BatteryWorker {
    /// Spawn the worker thread. Returns the handle (drop to stop) and the
    /// receiving end of the event channel.
    pub fn spawn(config: WorkerConfig) -> (Self, mpsc::Receiver<BatteryEvent>) {
        let (tx, rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let join = thread::spawn(move || worker_loop(config, running_clone, tx));

        (
            Self {
                running,
                join: Some(join),
            },
            rx,
        )
    }
}

/// Outcome of one attempted poll.
#[derive(PartialEq, Debug)]
enum PollOutcome {
    Ok(MouseStatus),
    Asleep,
    Disconnected,
    Busy,
}

impl DeviceSource {
    /// Attempt one battery read. Maps low-level errors to `PollOutcome`.
    fn poll(&self) -> PollOutcome {
        match self {
            DeviceSource::Shared(arc) => match arc.try_lock() {
                Ok(dev) => classify(protocol::get_mouse_battery(&dev)),
                Err(_) => PollOutcome::Busy,
            },
            DeviceSource::Owned => owned_poll(),
        }
    }
}

fn owned_poll() -> PollOutcome {
    if crate::lock::ui_is_active() {
        debug!("Worker (owned): TUI active, skipping poll");
        return PollOutcome::Busy;
    }

    let _device_guard = match crate::lock::try_acquire_device_lock() {
        Ok(g) => g,
        Err(_) => {
            debug!("Worker (owned): device lock held, skipping poll");
            return PollOutcome::Busy;
        }
    };

    if crate::lock::ui_is_active() {
        debug!("Worker (owned): TUI activated during lock acquire, skipping poll");
        return PollOutcome::Busy;
    }

    let dev = match Device::open() {
        Ok(d) => d,
        Err(e) => {
            debug!("Worker (owned): Device::open failed: {}", e);
            // Open failure is essentially always "dongle not present" — the
            // udev rules give us read access whenever the node exists. Treat
            // it as Disconnected so the backoff kicks in.
            return PollOutcome::Disconnected;
        }
    };

    classify(protocol::get_mouse_battery(&dev))
}

fn classify(result: std::result::Result<MouseStatus, BatteryReadError>) -> PollOutcome {
    match result {
        Ok(status) => PollOutcome::Ok(status),
        Err(BatteryReadError::Asleep) => PollOutcome::Asleep,
        Err(BatteryReadError::Io(e)) => {
            info!("Worker: transport error: {}", e);
            PollOutcome::Disconnected
        }
    }
}

fn next_sleep_interval(base: Duration, consecutive_asleep: u32) -> Duration {
    let multiplier: u32 = match consecutive_asleep {
        0 | 1 => 1,
        2 => 2,
        3 => 4,
        _ => 8,
    };
    base.saturating_mul(multiplier)
}

fn worker_loop(config: WorkerConfig, running: Arc<AtomicBool>, tx: mpsc::Sender<BatteryEvent>) {
    let WorkerConfig {
        interval,
        disconnect_backoff,
        device_source,
        refresh_flag,
    } = config;

    info!(
        "Battery worker started (interval: {}s, disconnect_backoff: {}s)",
        interval.as_secs(),
        disconnect_backoff.as_secs(),
    );

    const TICK: Duration = Duration::from_millis(200);

    let mut accumulated = Duration::ZERO;
    let mut backoff_remaining = Duration::ZERO;
    let mut consecutive_asleep: u32 = 0;
    let mut current_interval = interval;
    let mut first_poll_pending = true;

    while running.load(Ordering::Acquire) {
        thread::sleep(TICK);

        let manual_refresh = refresh_flag.swap(false, Ordering::AcqRel);

        if manual_refresh {
            accumulated = Duration::ZERO;
            backoff_remaining = Duration::ZERO;
        } else if backoff_remaining > Duration::ZERO {
            backoff_remaining = backoff_remaining.saturating_sub(TICK);
            continue;
        } else {
            accumulated += TICK;
            if !first_poll_pending && accumulated < current_interval {
                continue;
            }
        }

        accumulated = Duration::ZERO;
        first_poll_pending = false;

        match device_source.poll() {
            PollOutcome::Ok(status) => {
                if consecutive_asleep > 0 {
                    debug!(
                        "Worker: mouse awake after {} asleep poll(s)",
                        consecutive_asleep
                    );
                }
                consecutive_asleep = 0;
                current_interval = interval;
                let _ = tx.send(BatteryEvent::Update(status));
            }
            PollOutcome::Asleep => {
                consecutive_asleep = consecutive_asleep.saturating_add(1);
                current_interval = next_sleep_interval(interval, consecutive_asleep);
                debug!(
                    "Worker: mouse asleep (consecutive: {}, next poll in {}s)",
                    consecutive_asleep,
                    current_interval.as_secs()
                );
                let _ = tx.send(BatteryEvent::Asleep);
            }
            PollOutcome::Disconnected => {
                consecutive_asleep = 0;
                current_interval = interval;
                backoff_remaining = disconnect_backoff;
                warn!(
                    "Worker: device unreachable, backing off {}s",
                    disconnect_backoff.as_secs()
                );
                let _ = tx.send(BatteryEvent::Disconnected);
            }
            PollOutcome::Busy => {
                debug!("Worker: device busy, will retry next tick");
                accumulated = current_interval.saturating_sub(TICK);
            }
        }
    }

    info!("Battery worker shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_interval_first_poll_uses_base() {
        let base = Duration::from_secs(60);
        assert_eq!(next_sleep_interval(base, 0), base);
        assert_eq!(next_sleep_interval(base, 1), base);
    }

    #[test]
    fn sleep_interval_doubles() {
        let base = Duration::from_secs(60);
        assert_eq!(next_sleep_interval(base, 2), Duration::from_secs(120));
        assert_eq!(next_sleep_interval(base, 3), Duration::from_secs(240));
    }

    #[test]
    fn sleep_interval_caps_at_eight_times() {
        let base = Duration::from_secs(60);
        assert_eq!(next_sleep_interval(base, 4), Duration::from_secs(480));
        assert_eq!(next_sleep_interval(base, 5), Duration::from_secs(480));
        assert_eq!(next_sleep_interval(base, 100), Duration::from_secs(480));
    }

    #[test]
    fn sleep_interval_scales_with_base() {
        let base = Duration::from_secs(30);
        assert_eq!(next_sleep_interval(base, 2), Duration::from_secs(60));
        assert_eq!(next_sleep_interval(base, 4), Duration::from_secs(240));
    }
}
