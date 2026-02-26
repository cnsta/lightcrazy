use ksni::{menu::StandardItem, Icon, MenuItem, OfflineReason, ToolTip, Tray};
use log::{error, info, warn};
use std::sync::{Arc, Mutex};

use crate::{
    device::{protocol, transport::Device},
    tray::notifications::NotificationState,
};

#[derive(Debug, Clone)]
pub struct BatteryContext {
    /// The current battery reading. `None` until the first successful poll
    /// completes. Using Option avoids showing "0%" or a low-battery icon
    /// in the window between service start and the first device read.
    pub battery: Option<(u8, bool)>, // (level, is_charging)
    pub notifications: NotificationState,
}

impl Default for BatteryContext {
    fn default() -> Self {
        Self {
            battery: None,
            notifications: NotificationState::new(),
        }
    }
}

pub struct BatteryTray {
    pub ctx: Arc<Mutex<BatteryContext>>,
}

impl BatteryTray {
    pub fn update_battery(&self) -> anyhow::Result<()> {
        let dev = Device::open()?;
        let status = protocol::get_mouse_battery(&dev)?;

        // Read threshold from settings each poll so changes made in the TUI
        // take effect without restarting the tray service.
        let threshold = crate::settings::Settings::load().notification_threshold;

        let mut ctx = self.ctx.lock().unwrap();

        // Previous level for hysteresis, treat "no prior reading" as 100%
        // so we don't fire a low-battery alert on the very first read.
        let old_level = ctx.battery.map(|(l, _)| l).unwrap_or(100);

        ctx.battery = Some((status.battery_level, status.is_charging));

        info!(
            "Battery: {}%{}",
            status.battery_level,
            if status.is_charging { " ⚡" } else { "" }
        );

        let should_notify = ctx.notifications.should_notify_low_battery(
            status.battery_level,
            old_level,
            threshold,
            status.is_charging,
        );

        if should_notify {
            ctx.notifications.send_low_battery(status.battery_level)?;
        }

        Ok(())
    }

    /// Spawn this binary with `--options` in a terminal emulator.
    ///
    /// Resolution order:
    ///   1. `$TERMINAL`, explicit user preference, tried as-is.
    ///   2. `$TERM` hint, several emulators set `$TERM` to a value that
    ///      identifies their binary: alacritty->"alacritty", foot->"foot",
    ///      kitty->"xterm-kitty", wezterm->"wezterm", ghostty->"xterm-ghostty".
    ///      We map these to the correct binary and try before the generic list.
    ///   3. Hard-coded fallback list, skipping anything already tried above.
    fn launch_tui() {
        let bin = std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(String::from))
            .unwrap_or_else(|| "lightcrazy".to_string());

        let args = ["--options"];

        // 1. Explicit $TERMINAL
        let explicit = std::env::var("TERMINAL").ok().filter(|s| !s.is_empty());
        if let Some(ref term) = explicit {
            if try_launch_in_terminal(term, &bin, &args) {
                return;
            }
        }

        // 2. Hint from $TERM.
        //
        // $TERM is normally a terminfo capability name ("xterm-256color" etc.),
        // but several emulators set it to a value that unambiguously identifies
        // their binary. Map only known, unambiguous values.
        let term_hint = std::env::var("TERM").ok().and_then(|t| match t.as_str() {
            "alacritty" => Some("alacritty"),
            "foot" | "foot-extra" => Some("foot"),
            "xterm-kitty" => Some("kitty"),
            "wezterm" => Some("wezterm"),
            "xterm-ghostty" | "ghostty" => Some("ghostty"),
            _ => None,
        });
        if let Some(hint) = term_hint {
            let already_tried = explicit.as_deref().is_some_and(|e| e == hint);
            if !already_tried && try_launch_in_terminal(hint, &bin, &args) {
                return;
            }
        }

        // 3. Generic fallback list, skip anything already attempted above.
        let fallbacks = [
            "kitty",
            "alacritty",
            "wezterm",
            "ghostty",
            "foot",
            "konsole",
            "gnome-terminal",
            "xterm",
        ];
        let already_tried: Vec<&str> = [explicit.as_deref(), term_hint]
            .into_iter()
            .flatten()
            .collect();

        for term in &fallbacks {
            if already_tried.contains(term) {
                continue;
            }
            if try_launch_in_terminal(term, &bin, &args) {
                return;
            }
        }

        // 4. Absolute-path probe for NixOS and other non-standard layouts.
        //
        // When the service's PATH doesn't include per-user profile directories
        // (e.g. a systemd unit started before the shell profile is sourced),
        // name-based lookup above will fail even though the binary is present.
        // Probe the known NixOS profile paths directly by constructing
        // absolute paths from the current user's name.
        if let Some(abs) = find_terminal_in_nix_profiles(&already_tried) {
            if try_launch_in_terminal(&abs, &bin, &args) {
                return;
            }
        }

        error!("No terminal emulator found. Set $TERMINAL or install kitty/alacritty/foot.");
        NotificationState::send_notification(
            "No Terminal Found",
            "Set $TERMINAL or install kitty/alacritty/foot to open the control panel.",
            "dialog-error",
        );
    }
}

/// Probe known NixOS profile bin directories for a terminal emulator.
///
/// Returns the absolute path of the first match found, or None.
/// `skip` contains names already attempted via PATH-based lookup.
fn find_terminal_in_nix_profiles(skip: &[&str]) -> Option<String> {
    use std::path::PathBuf;

    // Build candidate search dirs. The per-user profile path requires the
    // username; fall back to $HOME-based path if USER is not set.
    let user = std::env::var("USER").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();

    let search_dirs: Vec<PathBuf> = [
        format!("/etc/profiles/per-user/{}/bin", user),
        format!("{}/.nix-profile/bin", home),
        "/run/current-system/sw/bin".to_string(),
        "/nix/var/nix/profiles/default/bin".to_string(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .map(PathBuf::from)
    .filter(|p| p.is_dir())
    .collect();

    let candidates = [
        "alacritty",
        "kitty",
        "foot",
        "wezterm",
        "ghostty",
        "konsole",
        "gnome-terminal",
        "xterm",
    ];

    for name in &candidates {
        if skip.contains(name) {
            continue;
        }
        for dir in &search_dirs {
            let full = dir.join(name);
            if full.is_file() {
                let path = full.to_string_lossy().into_owned();
                info!("Found terminal via NixOS profile path: {}", path);
                return Some(path);
            }
        }
    }

    None
}

fn try_launch_in_terminal(terminal: &str, bin: &str, extra_args: &[&str]) -> bool {
    use std::process::Command;

    let result = match terminal {
        "kitty" | "alacritty" | "ghostty" => Command::new(terminal)
            .arg("-e")
            .arg(bin)
            .args(extra_args)
            .spawn(),
        "foot" => Command::new("foot").arg(bin).args(extra_args).spawn(),
        "wezterm" => Command::new("wezterm")
            .args(["start", "--"])
            .arg(bin)
            .args(extra_args)
            .spawn(),
        "konsole" => Command::new("konsole")
            .arg("-e")
            .arg(bin)
            .args(extra_args)
            .spawn(),
        "gnome-terminal" => Command::new("gnome-terminal")
            .arg("--")
            .arg(bin)
            .args(extra_args)
            .spawn(),
        "xterm" => Command::new("xterm")
            .arg("-e")
            .arg(bin)
            .args(extra_args)
            .spawn(),
        other => Command::new(other)
            .arg("-e")
            .arg(bin)
            .args(extra_args)
            .spawn(),
    };

    match result {
        Ok(_) => {
            info!("Launched {} {} in {}", bin, extra_args.join(" "), terminal);
            true
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            warn!("Failed to launch {} in {}: {}", bin, terminal, e);
            false
        }
    }
}

impl Tray for BatteryTray {
    fn icon_name(&self) -> String {
        // Empty string defers to icon_pixmap(). The SNI spec says the host
        // should prefer pixmap data over a theme name when both are present,
        // and must use the pixmap when icon_name is empty.
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let ctx = self.ctx.lock().unwrap();
        let (level, charging) = ctx.battery.unwrap_or((0, false));
        crate::tray::icon::get_pixmaps(level, charging)
    }

    fn id(&self) -> String {
        "lightcrazy-battery".into()
    }

    fn title(&self) -> String {
        let ctx = self.ctx.lock().unwrap();
        match ctx.battery {
            Some((level, _)) => format!("Pulsar X2: {}%", level),
            None => "Pulsar X2: reading...".to_string(),
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let ctx = self.ctx.lock().unwrap();
        let (title, description) = match ctx.battery {
            Some((level, charging)) => (
                format!("Pulsar X2: {}%", level),
                if charging { "Charging" } else { "Discharging" }.to_string(),
            ),
            None => ("Pulsar X2".to_string(), "Reading battery...".to_string()),
        };
        ToolTip {
            title,
            description,
            icon_name: String::default(),
            icon_pixmap: Vec::default(),
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let ctx = self.ctx.lock().unwrap();
        let battery_text = match ctx.battery {
            Some((level, charging)) => {
                format!("Battery: {}%{}", level, if charging { " ⚡" } else { "" })
            }
            None => "Battery: reading...".to_string(),
        };

        vec![
            StandardItem {
                label: battery_text,
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Open Control Panel".into(),
                icon_name: "utilities-terminal-symbolic".into(),
                activate: Box::new(|_: &mut Self| {
                    Self::launch_tui();
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Refresh Now".into(),
                icon_name: "view-refresh-symbolic".into(),
                activate: Box::new(|this: &mut Self| {
                    if let Err(e) = this.update_battery() {
                        error!("Failed to refresh battery: {}", e);
                    }
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Exit".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(|_| {
                    info!("Exiting tray");
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }

    fn watcher_online(&self) {
        info!("Tray watcher online");
        crate::tray::utils::signal_watcher_online();
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        warn!("Tray watcher offline: {:?}", reason);
        crate::tray::utils::signal_respawn_needed();
        false
    }
}
