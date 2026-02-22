//! Embedded battery icons, rasterized from SVG at build time.
//!
//! `build.rs` reads `assets/icons/*.svg`, rasterizes them at 16×16, 22×22,
//! and 32×32 pixels, converts to ARGB32 network byte order, and writes the
//! resulting byte arrays into `$OUT_DIR/icons_generated.rs`.
//!
//! This module includes that generated file and exposes [`get_pixmaps`],
//! which returns the correct set of [`ksni::Icon`] structs for the current
//! battery state. Providing all three sizes lets the compositor pick the
//! best fit for the panel's actual icon size without scaling artifacts.

include!(concat!(env!("OUT_DIR"), "/icons_generated.rs"));

/// Return the embedded icon pixmaps for the given battery state.
///
/// `level` is clamped to 0–100. `is_charging` selects the charging variant.
/// All three rasterized sizes (16, 22, 32px) are returned so the compositor
/// can choose the best fit for the panel density.
pub fn get_pixmaps(level: u8, is_charging: bool) -> Vec<ksni::Icon> {
    let sizes: &[(&EmbeddedIcon, &EmbeddedIcon, &EmbeddedIcon)] = if is_charging {
        &[(
            charging_icon(level, 16),
            charging_icon(level, 22),
            charging_icon(level, 32),
        )]
    } else {
        &[(
            discharging_icon(level, 16),
            discharging_icon(level, 22),
            discharging_icon(level, 32),
        )]
    };

    let (s16, s22, s32) = sizes[0];
    vec![
        embedded_to_ksni(s16),
        embedded_to_ksni(s22),
        embedded_to_ksni(s32),
    ]
}

fn embedded_to_ksni(icon: &EmbeddedIcon) -> ksni::Icon {
    ksni::Icon {
        width: icon.width,
        height: icon.height,
        data: icon.argb32.to_vec(),
    }
}

/// Map a battery level to the correct discharging icon for a given pixel size.
fn discharging_icon(level: u8, size: u32) -> &'static EmbeddedIcon {
    if level >= 100 {
        return match size {
            16 => &BATTERY_FULL_16,
            22 => &BATTERY_FULL_22,
            _ => &BATTERY_FULL_32,
        };
    }
    match (level, size) {
        (0..=16, 16) => &BATTERY_0_16,
        (0..=16, 22) => &BATTERY_0_22,
        (0..=16, _) => &BATTERY_0_32,
        (17..=33, 16) => &BATTERY_1_16,
        (17..=33, 22) => &BATTERY_1_22,
        (17..=33, _) => &BATTERY_1_32,
        (34..=50, 16) => &BATTERY_2_16,
        (34..=50, 22) => &BATTERY_2_22,
        (34..=50, _) => &BATTERY_2_32,
        (51..=67, 16) => &BATTERY_3_16,
        (51..=67, 22) => &BATTERY_3_22,
        (51..=67, _) => &BATTERY_3_32,
        (68..=84, 16) => &BATTERY_4_16,
        (68..=84, 22) => &BATTERY_4_22,
        (68..=84, _) => &BATTERY_4_32,
        (_, 16) => &BATTERY_5_16, // 85–99
        (_, 22) => &BATTERY_5_22,
        _ => &BATTERY_5_32,
    }
}

/// Map a battery level to the correct charging icon for a given pixel size.
fn charging_icon(level: u8, size: u32) -> &'static EmbeddedIcon {
    if level >= 100 {
        return match size {
            16 => &BATTERY_CHARGING_FULL_16,
            22 => &BATTERY_CHARGING_FULL_22,
            _ => &BATTERY_CHARGING_FULL_32,
        };
    }
    match (level, size) {
        (0..=16, 16) => &BATTERY_CHARGING_0_16,
        (0..=16, 22) => &BATTERY_CHARGING_0_22,
        (0..=16, _) => &BATTERY_CHARGING_0_32,
        (17..=33, 16) => &BATTERY_CHARGING_1_16,
        (17..=33, 22) => &BATTERY_CHARGING_1_22,
        (17..=33, _) => &BATTERY_CHARGING_1_32,
        (34..=50, 16) => &BATTERY_CHARGING_2_16,
        (34..=50, 22) => &BATTERY_CHARGING_2_22,
        (34..=50, _) => &BATTERY_CHARGING_2_32,
        (51..=67, 16) => &BATTERY_CHARGING_3_16,
        (51..=67, 22) => &BATTERY_CHARGING_3_22,
        (51..=67, _) => &BATTERY_CHARGING_3_32,
        (68..=84, 16) => &BATTERY_CHARGING_4_16,
        (68..=84, 22) => &BATTERY_CHARGING_4_22,
        (68..=84, _) => &BATTERY_CHARGING_4_32,
        (_, 16) => &BATTERY_CHARGING_5_16, // 85–99
        (_, 22) => &BATTERY_CHARGING_5_22,
        _ => &BATTERY_CHARGING_5_32,
    }
}
