//! Status icons generated in memory: anti-aliased filled circles.

use super::view::Tone;

pub const GREEN: [u8; 3] = [0x22, 0xC5, 0x5E];
pub const RED: [u8; 3] = [0xEF, 0x44, 0x44];
pub const AMBER: [u8; 3] = [0xF5, 0x9E, 0x0B];
pub const GREY: [u8; 3] = [0x9C, 0xA3, 0xAF];

pub const TRAY_SIZE: u32 = 32;

pub fn tone_rgb(tone: Tone) -> [u8; 3] {
    match tone {
        Tone::Good => GREEN,
        Tone::Bad => RED,
        Tone::Warn => AMBER,
        Tone::Neutral => GREY,
    }
}

/// `size`×`size` RGBA pixels: a circle of colour `rgb` with a 1 px soft edge
/// on a transparent background.
pub fn circle_rgba(size: u32, rgb: [u8; 3]) -> Vec<u8> {
    let centre = size as f32 / 2.0;
    let radius = centre - 1.0;
    let mut buf = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - centre, y as f32 + 0.5 - centre);
            let distance = (dx * dx + dy * dy).sqrt();
            // Coverage falls from 1 to 0 across one pixel at the rim.
            let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
            buf.extend_from_slice(&[rgb[0], rgb[1], rgb[2], (coverage * 255.0).round() as u8]);
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(buf: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
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
    fn buffer_has_rgba_per_pixel() {
        assert_eq!(circle_rgba(32, GREEN).len(), 32 * 32 * 4);
        assert_eq!(circle_rgba(16, RED).len(), 16 * 16 * 4);
    }

    #[test]
    fn centre_is_opaque_colour_and_corners_transparent() {
        let buf = circle_rgba(32, RED);
        assert_eq!(pixel(&buf, 32, 16, 16), [239, 68, 68, 255]);
        for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31)] {
            assert_eq!(pixel(&buf, 32, x, y)[3], 0, "corner {x},{y}");
        }
    }

    #[test]
    fn edge_is_anti_aliased() {
        let buf = circle_rgba(32, GREEN);
        let soft = (0..32 * 32)
            .map(|i| buf[i * 4 + 3])
            .filter(|a| *a > 0 && *a < 255)
            .count();
        assert!(soft >= 16, "expected a soft edge, got {soft} partial pixels");
    }
}
