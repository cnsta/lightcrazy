use anyhow::Context;
use nix::fcntl::{Flock, FlockArg};
use std::fs::File;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

// Guard that holds an exclusive file lock.
// Released automatically when dropped.
pub struct LockGuard {
    _lock: Flock<File>,
}

// Acquire the tray/device ownership lock (non-blocking).
//
// Held by whichever process owns the tray icon. Fails immediately if
// another instance already holds it.
pub fn acquire_tray_lock() -> anyhow::Result<LockGuard> {
    try_acquire_nonblock("lightcrazy-tray")
}

// Acquire the UI lock (non-blocking).
//
// Held while the TUI is open. The tray's battery monitor checks this
// before each poll and skips if held, preventing inter-process HID
// contention during TUI operation.
pub fn acquire_ui_lock() -> anyhow::Result<LockGuard> {
    try_acquire_nonblock("lightcrazy-ui")
}

// Acquire the device access lock (blocking).
//
// Held during any HID protocol exchange that must be atomic, specifically,
// the tray's battery poll and the TUI's startup initialization. Unlike the
// other locks, this one blocks until the lock is available rather than
// failing immediately, so callers can use it to wait for an in-progress
// operation to complete.
pub fn acquire_device_lock() -> anyhow::Result<LockGuard> {
    try_acquire_blocking("lightcrazy-device")
}

// Returns true if the tray is already running in another process.
pub fn tray_is_running() -> bool {
    try_acquire_nonblock("lightcrazy-tray").is_err()
}

// Returns true if the TUI is currently open in another process.
pub fn ui_is_active() -> bool {
    try_acquire_nonblock("lightcrazy-ui").is_err()
}

// Non-blocking exclusive lock, fails immediately if already held.
fn try_acquire_nonblock(name: &str) -> anyhow::Result<LockGuard> {
    let file = open_lock_file(name)?;
    let lock = Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|_| {
        anyhow::anyhow!(
            "Lock '{}' is already held.\n\
             If you're sure no other instance is running, delete: {}",
            name,
            lock_path(name).display()
        )
    })?;
    Ok(LockGuard { _lock: lock })
}

// Blocking exclusive lock, waits until the lock is available.
fn try_acquire_blocking(name: &str) -> anyhow::Result<LockGuard> {
    let file = open_lock_file(name)?;
    let lock = Flock::lock(file, FlockArg::LockExclusive)
        .map_err(|(_, e)| anyhow::anyhow!("Failed to acquire lock '{}': {}", name, e))?;
    Ok(LockGuard { _lock: lock })
}

fn open_lock_file(name: &str) -> anyhow::Result<File> {
    let path = lock_path(name);
    std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("Failed to open lock file: {}", path.display()))
}

fn lock_path(name: &str) -> PathBuf {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join(format!("{}.lock", name))
    } else if PathBuf::from("/run").exists() {
        PathBuf::from("/run/user")
            .join(std::env::var("UID").unwrap_or_else(|_| "0".to_string()))
            .join(format!("{}.lock", name))
    } else {
        PathBuf::from("/tmp").join(format!(
            "{}-{}.lock",
            name,
            std::env::var("USER").unwrap_or_else(|_| "unknown".to_string())
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_paths_are_distinct() {
        let tray = lock_path("lightcrazy-tray");
        let ui = lock_path("lightcrazy-ui");
        let device = lock_path("lightcrazy-device");
        assert_ne!(tray, ui);
        assert_ne!(tray, device);
        assert_ne!(ui, device);
    }

    #[test]
    fn test_lock_paths_end_with_lock() {
        for name in &["lightcrazy-tray", "lightcrazy-ui", "lightcrazy-device"] {
            assert!(lock_path(name).to_string_lossy().ends_with(".lock"));
        }
    }
}
