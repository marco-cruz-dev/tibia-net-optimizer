//! App and tray icons, drawn in memory (no image files at runtime).
//!
//! The mark is a heater shield (RPG / "protection") with a white ping pulse
//! across it (network latency), on a blue→teal gradient with a dark outline
//! so it reads on light and dark taskbars. The tray icon adds a status dot
//! (green / red / amber / grey) in the bottom-right corner.

use super::view::Tone;
pub use crate::app::icon_raster::{
    ico_file, render, BADGE_CENTRE, BADGE_RADIUS, OUTLINE, PULSE, SHIELD_BOTTOM, SHIELD_TOP,
};

pub const GREEN: [u8; 3] = [0x22, 0xC5, 0x5E];
pub const RED: [u8; 3] = [0xEF, 0x44, 0x44];
pub const AMBER: [u8; 3] = [0xF5, 0x9E, 0x0B];
pub const GREY: [u8; 3] = [0x9C, 0xA3, 0xAF];

pub const TRAY_SIZE: u32 = 32;
/// Window/taskbar icon; large enough for Alt-Tab and high-DPI taskbars.
pub const WINDOW_ICON_SIZE: u32 = 128;

pub fn tone_rgb(tone: Tone) -> [u8; 3] {
    match tone {
        Tone::Good => GREEN,
        Tone::Bad => RED,
        Tone::Warn => AMBER,
        Tone::Neutral => GREY,
    }
}

/// `size`×`size` RGBA: the app icon (shield + pulse).
pub fn app_icon_rgba(size: u32) -> Vec<u8> {
    render(size, None)
}

/// `size`×`size` RGBA: the app icon with a status dot for the tray.
pub fn tray_icon_rgba(size: u32, tone: Tone) -> Vec<u8> {
    render(size, Some(tone_rgb(tone)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(buf: &[u8], size: u32, u: f32, v: f32) -> [u8; 4] {
        let (x, y) = ((u * size as f32) as u32, (v * size as f32) as u32);
        let i = ((y * size + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn tones_map_to_spec_colours() {
        assert_eq!(tone_rgb(Tone::Good), GREEN);
        assert_eq!(tone_rgb(Tone::Bad), RED);
        assert_eq!(tone_rgb(Tone::Warn), AMBER);
        assert_eq!(tone_rgb(Tone::Neutral), GREY);
        assert_eq!(GREEN, [34, 197, 94]); // #22C55E
        assert_eq!(RED, [239, 68, 68]); // #EF4444
    }

    #[test]
    fn buffers_have_rgba_per_pixel() {
        assert_eq!(app_icon_rgba(64).len(), 64 * 64 * 4);
        assert_eq!(tray_icon_rgba(32, Tone::Good).len(), 32 * 32 * 4);
        assert_eq!(tray_icon_rgba(16, Tone::Bad).len(), 16 * 16 * 4);
    }

    #[test]
    fn shield_is_opaque_and_blue_with_transparent_surroundings() {
        let icon = app_icon_rgba(64);
        let upper = pixel(&icon, 64, 0.5, 0.2);
        assert_eq!(upper[3], 255, "inside the shield");
        assert!(upper[2] > upper[0] + 80, "blue-ish fill: {upper:?}");
        for (u, v) in [
            (0.01, 0.01),
            (0.99, 0.01),
            (0.01, 0.99),
            (0.99, 0.99),
            (0.5, 0.99),
        ] {
            assert_eq!(pixel(&icon, 64, u, v)[3], 0, "outside at {u},{v}");
        }
    }

    #[test]
    fn fill_shades_from_blue_to_teal() {
        let icon = app_icon_rgba(128);
        let top = pixel(&icon, 128, 0.5, 0.18);
        let low = pixel(&icon, 128, 0.5, 0.82);
        assert!(
            top[2] > low[2] && low[1] > top[1],
            "top {top:?} vs bottom {low:?}"
        );
    }

    #[test]
    fn pulse_line_is_white() {
        let icon = app_icon_rgba(128);
        let p = pixel(&icon, 128, 0.30, 0.50);
        assert!(
            p.iter().take(3).all(|c| *c > 220) && p[3] == 255,
            "pulse pixel {p:?}"
        );
    }

    #[test]
    fn edges_are_anti_aliased() {
        let icon = app_icon_rgba(64);
        let soft = (0..64 * 64)
            .filter(|i| matches!(icon[i * 4 + 3], 1..=254))
            .count();
        assert!(
            soft >= 30,
            "expected a soft silhouette, got {soft} partial pixels"
        );
    }

    #[test]
    fn tray_icon_shows_status_dot_in_tone_colour() {
        for tone in [Tone::Good, Tone::Bad, Tone::Warn, Tone::Neutral] {
            let icon = tray_icon_rgba(64, tone);
            let [r, g, b, a] = pixel(&icon, 64, BADGE_CENTRE.0, BADGE_CENTRE.1);
            assert_eq!(([r, g, b], a), (tone_rgb(tone), 255), "{tone:?}");
        }
        let plain = app_icon_rgba(64);
        let at_badge = pixel(&plain, 64, BADGE_CENTRE.0, BADGE_CENTRE.1);
        assert_ne!(&at_badge[..3], &GREEN[..], "no dot on the app icon");
    }

    /// Dumps raw RGBA previews to `target/icon-preview/` for visual review:
    /// `cargo test --lib app::icon -- --ignored`
    #[test]
    #[ignore]
    fn write_icon_previews() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/icon-preview");
        std::fs::create_dir_all(&dir).unwrap();
        for size in [256, 64, 32, 16] {
            std::fs::write(dir.join(format!("app-{size}.rgba")), app_icon_rgba(size)).unwrap();
        }
        for (name, tone) in [
            ("good", Tone::Good),
            ("bad", Tone::Bad),
            ("warn", Tone::Warn),
            ("neutral", Tone::Neutral),
        ] {
            for size in [32, 16] {
                std::fs::write(
                    dir.join(format!("tray-{name}-{size}.rgba")),
                    tray_icon_rgba(size, tone),
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn tray_icon_keeps_the_shield_recognisable() {
        let tray = tray_icon_rgba(32, Tone::Good);
        let app = app_icon_rgba(32);
        assert_eq!(
            pixel(&tray, 32, 0.3, 0.2),
            pixel(&app, 32, 0.3, 0.2),
            "shield untouched away from the dot"
        );
    }
}
