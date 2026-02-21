/// Return the freedesktop icon name for the current battery state.
///
/// Uses the standard freedesktop stepped icon names:
/// Discharging: "battery-level-{0,10,20,...,90}-symbolic"
/// Charging:    "battery-level-{0,10,20,...,90}-charging-symbolic"
/// Full:        "battery-level-100-charged-symbolic"  (level == 100 only)
pub fn get_battery_icon_name(level: u8, is_charging: bool) -> String {
    // Over-reported firmware values or a true 100% reading.
    if level >= 100 {
        return "battery-level-100-charged-symbolic".to_string();
    }

    // Widen to u16 before adding 5, (255u8 + 5) wraps to 4 in u8 arithmetic.
    // This branch is only reached for level 0–99, so the result fits in u8.
    let rounded = ((((level as u16) + 5) / 10) * 10).min(90) as u8;

    if is_charging {
        format!("battery-level-{}-charging-symbolic", rounded)
    } else {
        format!("battery-level-{}-symbolic", rounded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_battery_uses_charged_icon() {
        assert_eq!(
            get_battery_icon_name(100, false),
            "battery-level-100-charged-symbolic"
        );
        assert_eq!(
            get_battery_icon_name(100, true),
            "battery-level-100-charged-symbolic"
        );
    }

    #[test]
    fn over_reported_level_uses_charged_icon() {
        // Firmware sometimes reports > 100. Must not overflow u8 arithmetic.
        assert_eq!(
            get_battery_icon_name(255, false),
            "battery-level-100-charged-symbolic"
        );
        assert_eq!(
            get_battery_icon_name(200, false),
            "battery-level-100-charged-symbolic"
        );
        assert_eq!(
            get_battery_icon_name(101, false),
            "battery-level-100-charged-symbolic"
        );
    }

    #[test]
    fn near_full_discharging_does_not_show_charging_icon() {
        // 95–99% discharging must not return the "charged" icon, that icon
        // renders with a bolt/plug in most themes and implies the device is
        // charging. The correct icon is the 90% step.
        assert_eq!(
            get_battery_icon_name(99, false),
            "battery-level-90-symbolic"
        );
        assert_eq!(
            get_battery_icon_name(95, false),
            "battery-level-90-symbolic"
        );
    }

    #[test]
    fn near_full_charging_uses_charging_icon() {
        assert_eq!(
            get_battery_icon_name(95, true),
            "battery-level-90-charging-symbolic"
        );
        assert_eq!(
            get_battery_icon_name(99, true),
            "battery-level-90-charging-symbolic"
        );
    }

    #[test]
    fn charging_uses_charging_suffix() {
        assert_eq!(
            get_battery_icon_name(50, true),
            "battery-level-50-charging-symbolic"
        );
        assert_eq!(
            get_battery_icon_name(30, true),
            "battery-level-30-charging-symbolic"
        );
    }

    #[test]
    fn discharging_uses_plain_suffix() {
        assert_eq!(
            get_battery_icon_name(50, false),
            "battery-level-50-symbolic"
        );
    }

    #[test]
    fn rounding_boundaries() {
        // 94% rounds to 90, 45% rounds to 50, 44% rounds to 40
        assert_eq!(
            get_battery_icon_name(94, false),
            "battery-level-90-symbolic"
        );
        assert_eq!(
            get_battery_icon_name(45, false),
            "battery-level-50-symbolic"
        );
        assert_eq!(
            get_battery_icon_name(44, false),
            "battery-level-40-symbolic"
        );
    }

    #[test]
    fn zero_percent_is_valid() {
        assert_eq!(get_battery_icon_name(0, false), "battery-level-0-symbolic");
    }
}
