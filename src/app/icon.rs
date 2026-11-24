//! App and tray icons, drawn in memory (no image files at runtime).
//!
//! The mark is a heater shield (RPG / "protection") with a white ping pulse
//! across it (network latency), on a blue→teal gradient with a dark outline
//! so it reads on light and dark taskbars. The tray icon adds a status dot
//! (green / red / amber / grey) in the bottom-right corner.

use super::view::Tone;

pub const GREEN: [u8; 3] = [0x22, 0xC5, 0x5E];
pub const RED: [u8; 3] = [0xEF, 0x44, 0x44];
pub const AMBER: [u8; 3] = [0xF5, 0x9E, 0x0B];
pub const GREY: [u8; 3] = [0x9C, 0xA3, 0xAF];

/// Shield palette.
pub const OUTLINE: [u8; 3] = [0x0B, 0x1A, 0x33];
pub const SHIELD_TOP: [u8; 3] = [0x25, 0x63, 0xEB];
pub const SHIELD_BOTTOM: [u8; 3] = [0x0F, 0x76, 0x6E];
pub const PULSE: [u8; 3] = [0xFF, 0xFF, 0xFF];

pub const TRAY_SIZE: u32 = 32;
/// Window/taskbar icon; large enough for Alt-Tab and high-DPI taskbars.
pub const WINDOW_ICON_SIZE: u32 = 128;

/// Status-dot geometry (unit coordinates), shared with tests.
pub const BADGE_CENTRE: (f32, f32) = (0.76, 0.76);
pub const BADGE_RADIUS: f32 = 0.24;

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

// --- rasterizer (unit coordinates: 0..1 across the icon) -------------------

type Pt = (f32, f32);

const OUTLINE_WIDTH: f32 = 0.05;
const PULSE_HALF_WIDTH: f32 = 0.045;
const BADGE_RING: f32 = 0.06;
const SHIELD_TOP_Y: f32 = 0.10;
const SHIELD_TIP_Y: f32 = 0.94;

/// Ping pulse: flat, spike up, dip down, settle.
const PULSE_POINTS: [Pt; 7] = [
    (0.24, 0.50),
    (0.38, 0.50),
    (0.45, 0.32),
    (0.54, 0.68),
    (0.61, 0.44),
    (0.66, 0.50),
    (0.76, 0.50),
];

fn quad(a: Pt, c: Pt, b: Pt, t: f32) -> Pt {
    let u = 1.0 - t;
    (
        u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0,
        u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1,
    )
}

/// Heater shield: flat top, straight sides, curved sides meeting at a point.
fn shield_polygon() -> Vec<Pt> {
    let mut pts = vec![(0.16, SHIELD_TOP_Y), (0.84, SHIELD_TOP_Y), (0.84, 0.50)];
    for i in 1..=16 {
        pts.push(quad(
            (0.84, 0.50),
            (0.82, 0.80),
            (0.5, SHIELD_TIP_Y),
            i as f32 / 16.0,
        ));
    }
    for i in 1..=16 {
        pts.push(quad(
            (0.5, SHIELD_TIP_Y),
            (0.18, 0.80),
            (0.16, 0.50),
            i as f32 / 16.0,
        ));
    }
    pts
}

fn segment_distance(p: Pt, a: Pt, b: Pt) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    let (dx, dy) = (p.0 - (a.0 + t * abx), p.1 - (a.1 + t * aby));
    (dx * dx + dy * dy).sqrt()
}

/// Negative inside, positive outside.
fn polygon_signed_distance(p: Pt, poly: &[Pt]) -> f32 {
    let mut d = f32::INFINITY;
    let mut inside = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        d = d.min(segment_distance(p, a, b));
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) {
            inside = !inside;
        }
    }
    if inside {
        -d
    } else {
        d
    }
}

fn polyline_distance(p: Pt, line: &[Pt]) -> f32 {
    line.windows(2)
        .map(|w| segment_distance(p, w[0], w[1]))
        .fold(f32::INFINITY, f32::min)
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| a[i] as f32 + (b[i] as f32 - a[i] as f32) * t)
}

/// "Over" compositing of `rgb` with `coverage` onto a straight-alpha pixel.
fn over(dst: &mut [f32; 4], rgb: [f32; 3], coverage: f32) {
    if coverage <= 0.0 {
        return;
    }
    let da = dst[3];
    let out_a = coverage + da * (1.0 - coverage);
    for i in 0..3 {
        dst[i] = (rgb[i] * coverage + dst[i] * da * (1.0 - coverage)) / out_a;
    }
    dst[3] = out_a;
}

fn render(size: u32, badge: Option<[u8; 3]>) -> Vec<u8> {
    let px = 1.0 / size as f32; // one pixel in unit coordinates
                                // Coverage from a signed distance: a 1 px anti-aliased edge.
    let cov = |sd: f32| (0.5 - sd / px).clamp(0.0, 1.0);
    let shield = shield_polygon();
    let pulse_half = PULSE_HALF_WIDTH.max(0.6 * px);
    let rgb = |c: [u8; 3]| c.map(f32::from);

    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let p = ((x as f32 + 0.5) * px, (y as f32 + 0.5) * px);
            let mut c = [0.0f32; 4];

            let sd = polygon_signed_distance(p, &shield);
            over(&mut c, rgb(OUTLINE), cov(sd));
            let t = (p.1 - SHIELD_TOP_Y) / (SHIELD_TIP_Y - SHIELD_TOP_Y);
            over(
                &mut c,
                lerp_rgb(SHIELD_TOP, SHIELD_BOTTOM, t),
                cov(sd + OUTLINE_WIDTH),
            );
            over(
                &mut c,
                rgb(PULSE),
                cov(polyline_distance(p, &PULSE_POINTS) - pulse_half),
            );

            if let Some(dot) = badge {
                let d = ((p.0 - BADGE_CENTRE.0).powi(2) + (p.1 - BADGE_CENTRE.1).powi(2)).sqrt();
                over(&mut c, rgb(OUTLINE), cov(d - BADGE_RADIUS));
                over(&mut c, rgb(dot), cov(d - (BADGE_RADIUS - BADGE_RING)));
            }

            let a = (c[3] * 255.0).round() as u8;
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                out.extend_from_slice(&[c[0].round() as u8, c[1].round() as u8, c[2].round() as u8, a]);
            }
        }
    }
    out
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
