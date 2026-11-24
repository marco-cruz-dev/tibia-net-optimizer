//! The shield icon rasterizer and an ICO writer.
//!
//! Dependency-free on purpose: `build.rs` includes this file with `#[path]`
//! to generate the Windows `.ico` from the same drawing as the window and
//! tray icons. Unit coordinates: 0..1 across the icon.

/// Shield palette.
pub const OUTLINE: [u8; 3] = [0x0B, 0x1A, 0x33];
pub const SHIELD_TOP: [u8; 3] = [0x25, 0x63, 0xEB];
pub const SHIELD_BOTTOM: [u8; 3] = [0x0F, 0x76, 0x6E];
pub const PULSE: [u8; 3] = [0xFF, 0xFF, 0xFF];

/// Status-dot geometry (unit coordinates).
pub const BADGE_CENTRE: (f32, f32) = (0.76, 0.76);
pub const BADGE_RADIUS: f32 = 0.24;

/// Sizes embedded in the Windows `.ico`.
pub const ICO_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

/// A Windows `.ico` file from square RGBA images (`(size, rgba)`), stored as
/// 32-bit BMP entries (BGRA, bottom-up, plus an all-zero AND mask).
pub fn ico_file(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let entries: Vec<Vec<u8>> = images.iter().map(|(size, rgba)| bmp_entry(*size, rgba)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for ((size, _), data) in images.iter().zip(&entries) {
        let dim = if *size >= 256 { 0 } else { *size as u8 }; // 0 means 256
        out.extend_from_slice(&[dim, dim, 0, 0]); // width, height, palette, reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for data in entries {
        out.extend_from_slice(&data);
    }
    out
}

/// BITMAPINFOHEADER + BGRA rows bottom-up + an all-zero AND mask.
fn bmp_entry(size: u32, rgba: &[u8]) -> Vec<u8> {
    let s = size as usize;
    let mask_row = s.div_ceil(32) * 4; // 1 bpp, rows padded to 32 bits
    let mut out = Vec::with_capacity(40 + s * s * 4 + mask_row * s);
    out.extend_from_slice(&40u32.to_le_bytes()); // header size
    out.extend_from_slice(&size.to_le_bytes()); // width
    out.extend_from_slice(&(size * 2).to_le_bytes()); // height: colour + mask
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bpp
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&((s * s * 4 + mask_row * s) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // resolution and palette fields
    for y in (0..s).rev() {
        for x in 0..s {
            let i = (y * s + x) * 4;
            out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        }
    }
    out.resize(out.len() + mask_row * s, 0); // alpha channel is authoritative
    out
}

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

/// `size`×`size` RGBA of the shield, with an optional status dot colour.
pub fn render(size: u32, badge: Option<[u8; 3]>) -> Vec<u8> {
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

    fn u16le(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }
    fn u32le(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
    }

    fn solid(size: u32, rgba: [u8; 4]) -> Vec<u8> {
        std::iter::repeat_n(rgba, (size * size) as usize)
            .flatten()
            .collect()
    }

    #[test]
    fn ico_header_and_directory() {
        let ico = ico_file(&[(16, solid(16, [1, 2, 3, 4])), (256, solid(256, [0; 4]))]);
        assert_eq!(
            (u16le(&ico, 0), u16le(&ico, 2), u16le(&ico, 4)),
            (0, 1, 2),
            "reserved, type=icon, count"
        );
        // Entry 0: 16x16
        assert_eq!((ico[6], ico[7]), (16, 16));
        assert_eq!((u16le(&ico, 10), u16le(&ico, 12)), (1, 32), "planes, bpp");
        // Entry 1: 256 is stored as 0
        assert_eq!((ico[22], ico[23]), (0, 0));
        // Data offsets/sizes chain without gaps and cover the file.
        let (size0, off0) = (u32le(&ico, 14) as usize, u32le(&ico, 18) as usize);
        let (size1, off1) = (u32le(&ico, 30) as usize, u32le(&ico, 34) as usize);
        assert_eq!(off0, 6 + 2 * 16);
        assert_eq!(off1, off0 + size0);
        assert_eq!(ico.len(), off1 + size1);
    }

    #[test]
    fn ico_entry_is_a_32bit_bmp_with_mask() {
        let ico = ico_file(&[(16, solid(16, [10, 20, 30, 40]))]);
        let off = u32le(&ico, 18) as usize;
        let bmp = &ico[off..];
        assert_eq!(u32le(bmp, 0), 40, "BITMAPINFOHEADER size");
        assert_eq!(u32le(bmp, 4), 16, "width");
        assert_eq!(u32le(bmp, 8), 32, "height doubled (XOR + AND)");
        assert_eq!((u16le(bmp, 12), u16le(bmp, 14)), (1, 32));
        // First pixel: BGRA.
        assert_eq!(&bmp[40..44], &[30, 20, 10, 40]);
        // 16x16x4 colour bytes + AND mask: 16 rows × 4 bytes (32-bit padded).
        assert_eq!(u32le(&ico, 14) as usize, 40 + 16 * 16 * 4 + 16 * 4);
    }

    #[test]
    fn ico_rows_are_bottom_up() {
        let mut img = solid(16, [0, 0, 0, 255]);
        img[..4].copy_from_slice(&[255, 0, 0, 255]); // top-left pixel red
        let ico = ico_file(&[(16, img)]);
        let off = u32le(&ico, 18) as usize + 40;
        let last_row = off + 15 * 16 * 4;
        assert_eq!(
            &ico[last_row..last_row + 4],
            &[0, 0, 255, 255],
            "top row stored last (BGRA red)"
        );
    }
}
