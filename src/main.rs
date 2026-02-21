use lightcrazy::lock::{acquire_device_lock, acquire_tray_lock, acquire_ui_lock, tray_is_running};
use log::{error, info};

fn main() -> anyhow::Result<()> {
    let open_ui = std::env::args().any(|a| a == "--options" || a == "-o");

    if open_ui {
        // In TUI mode, redirect logs to a file so background thread messages
        // don't bleed through into the raw-mode terminal.
        init_file_logger();

        let _tray_lock = if !tray_is_running() {
            info!("No tray running — starting tray in background");
            let lock = acquire_tray_lock()?;
            lightcrazy::tray::start_tray_background()?;
            Some(lock)
        } else {
            info!("Tray already running — attaching TUI");
            None
        };

        // Signal the tray monitor to skip battery polls while TUI is open.
        let _ui_lock = acquire_ui_lock()
            .map_err(|_| anyhow::anyhow!("TUI is already open in another window"))?;

        // Wait for any in-progress tray battery poll to finish before opening
        // the device ourselves. The tray holds the device lock for the duration
        // of each poll, acquiring it here (blocking) guarantees the protocol
        // is in a clean state before App::new() touches the device. We release
        // it immediately, the UI lock above prevents the tray from starting
        // any new polls, so the device is ours for the TUI session.
        {
            let _settle = acquire_device_lock()
                .map_err(|e| anyhow::anyhow!("Could not acquire device lock: {}", e))?;
            info!("Device lock acquired and released — device is in a clean state");
        }

        if let Err(e) = lightcrazy::ui::run() {
            error!("TUI error: {}", e);
        }
    } else {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
            .init();

        info!("Starting LightCrazy tray");

        let _lock = acquire_tray_lock().map_err(|_| {
            anyhow::anyhow!(
                "Tray is already running.\nUse --options / -o to open the settings panel."
            )
        })?;

        if let Err(e) = lightcrazy::tray::start_tray_service() {
            error!("Tray service error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn init_file_logger() {
    use std::fs::OpenOptions;

    let log_path = std::env::var_os("XDG_RUNTIME_DIR")
        .map(|d| std::path::PathBuf::from(d).join("lightcrazy.log"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/lightcrazy.log"));

    if let Ok(file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
            .format_timestamp(Some(env_logger::TimestampPrecision::Seconds))
            .target(env_logger::Target::Pipe(Box::new(file)))
            .init();

        info!("TUI mode — logging to {}", log_path.display());
    } else {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Error)
            .init();
    }
}
