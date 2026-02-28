//! Embedded battery icons, rasterized from SVG at build time.
//!
//! `build.rs` reads `assets/icons/battery_{level}.svg` and
//! `assets/icons/battery_charging_{level}.svg` for levels
//! 0, 5, 10 … 95, 100. Each SVG is rasterized at 16, 22, and 32 pixels
//! and written as ARGB32 byte arrays into `$OUT_DIR/icons_generated.rs`.
//! That file also contains a `get_icon(level, charging, size)` lookup
//! function that snaps the level to the nearest 5% step.

include!(concat!(env!("OUT_DIR"), "/icons_generated.rs"));

/// Return the embedded icon pixmaps for the given battery state.
///
/// All three rasterized sizes (16, 22, 32 px) are returned so the
/// compositor can pick the best fit for the panel density without scaling.
pub fn get_pixmaps(level: u8, is_charging: bool) -> Vec<ksni::Icon> {
    vec![
        embedded_to_ksni(get_icon(level, is_charging, 16)),
        embedded_to_ksni(get_icon(level, is_charging, 22)),
        embedded_to_ksni(get_icon(level, is_charging, 32)),
    ]
}

fn embedded_to_ksni(icon: &EmbeddedIcon) -> ksni::Icon {
    ksni::Icon {
        width: icon.width,
        height: icon.height,
        data: icon.argb32.to_vec(),
    }
}
