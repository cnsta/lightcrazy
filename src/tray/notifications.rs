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

#[cfg(test)]
mod tests {
    use super::*;

    /// Charging at any level must never notify, even below threshold.
    #[test]
    fn not_notified_when_charging() {
        let s = NotificationState::new();
        assert!(!s.should_notify_low_battery(5, 10, 20, true));
        assert!(!s.should_notify_low_battery(15, 20, 20, true));
    }

    /// Above the threshold must never notify, regardless of trend.
    #[test]
    fn not_notified_above_threshold() {
        let s = NotificationState::new();
        assert!(!s.should_notify_low_battery(50, 60, 20, false));
        assert!(!s.should_notify_low_battery(21, 22, 20, false));
    }

    /// Equal-to-previous reading is "no change" and must not notify.
    /// Otherwise the user would get spammed every poll while the level
    /// sat at the threshold.
    #[test]
    fn not_notified_when_level_unchanged() {
        let s = NotificationState::new();
        assert!(!s.should_notify_low_battery(15, 15, 20, false));
    }

    /// Level *rose* (the mouse was put on a wireless charging pad, then off,
    /// or the dongle re-enumerated and the new read happened to be higher).
    /// Must not notify, the situation is improving, not worsening.
    #[test]
    fn not_notified_when_level_rose() {
        let s = NotificationState::new();
        assert!(!s.should_notify_low_battery(16, 15, 20, false));
    }

    /// First time we see a drop below threshold: notify.
    #[test]
    fn notified_on_fresh_drop_below_threshold() {
        let s = NotificationState::new();
        assert!(s.should_notify_low_battery(15, 16, 20, false));
        assert!(s.should_notify_low_battery(19, 20, 20, false));
    }

    /// After a notification has been sent, the cooldown window suppresses
    /// repeats. We can't fast-forward time in tests, so we instead set
    /// `last_low_battery` to "now" and verify the next call returns false.
    #[test]
    fn cooldown_suppresses_repeat() {
        let mut s = NotificationState::new();
        s.last_low_battery = Some(Instant::now());
        // A genuine drop below threshold that *would* notify if the cooldown
        // weren't active.
        assert!(!s.should_notify_low_battery(10, 15, 20, false));
    }

    /// Cooldown expired: a fresh drop notifies again. Simulated by setting
    /// the timestamp to longer than COOLDOWN_SECS ago.
    #[test]
    fn cooldown_expires() {
        let mut s = NotificationState::new();
        s.last_low_battery =
            Some(Instant::now() - std::time::Duration::from_secs(COOLDOWN_SECS + 1));
        assert!(s.should_notify_low_battery(10, 15, 20, false));
    }
}
