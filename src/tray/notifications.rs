use notify_rust::{Notification, Urgency};
use std::time::Instant;

// Minimum time between repeated low-battery notifications.
const COOLDOWN_SECS: u64 = 300; // 5 minutes

#[derive(Debug, Clone)]
pub struct NotificationState {
    // When the last low-battery notification was sent.
    // "None" if no notification has been sent this session.
    last_low_battery: Option<Instant>,
}

impl NotificationState {
    pub fn new() -> Self {
        Self {
            last_low_battery: None,
        }
    }

    // Returns true if a low-battery notification should be sent now.
    //
    // Conditions (all must hold):
    // * Not currently charging.
    // * Level is at or below the configured threshold.
    // * Level has dropped since the last reading (avoids re-alerting
    //   at the same level across consecutive polls).
    // * No notification has been sent within the last 5 minutes
    //   (prevents spam if the mouse sits at threshold for a long time).
    pub fn should_notify_low_battery(
        &self,
        current_level: u8,
        previous_level: u8,
        threshold: u8,
        is_charging: bool,
    ) -> bool {
        if is_charging || current_level > threshold || current_level >= previous_level {
            return false;
        }

        self.last_low_battery
            .map(|t| t.elapsed().as_secs() >= COOLDOWN_SECS)
            .unwrap_or(true)
    }

    // Send a low-battery notification and record the time for cooldown.
    pub fn send_low_battery(&mut self, level: u8) -> anyhow::Result<()> {
        Notification::new()
            .summary("Low Battery")
            .body(&format!("Pulsar X2 battery at {}%", level))
            .icon("battery-caution-symbolic")
            .urgency(Urgency::Critical)
            .timeout(0) // persistent until dismissed — battery needs attention
            .show()
            .map_err(|e| anyhow::anyhow!("Notification failed: {}", e))?;

        self.last_low_battery = Some(Instant::now());
        Ok(())
    }

    // Send a one-shot informational notification (fire and forget).
    pub fn send_notification(summary: &str, body: &str, icon: &str) {
        let _ = Notification::new()
            .summary(summary)
            .body(body)
            .icon(icon)
            .urgency(Urgency::Normal)
            .timeout(5000)
            .show();
    }
}

impl Default for NotificationState {
    fn default() -> Self {
        Self::new()
    }
}
