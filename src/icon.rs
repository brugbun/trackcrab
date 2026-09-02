//! The window and taskbar icon, drawn in code.
//!
//! Generated rather than shipped as a PNG so there is no binary asset to keep in
//! step with the source, and no image decoding dependency for one 64px picture.

/// Edge length of the generated icon.
const SIZE: u32 = 64;
/// Supersampling factor. Coverage is averaged over a 3x3 grid per pixel, which
/// is what keeps the curves from looking like stairs at this size.
const SS: u32 = 3;

/// Shell red.
const SHELL: [u8; 3] = [0xe8, 0x57, 0x4c];
/// A darker red for the legs and claw outlines, so they read against the body.
const SHELL_DARK: [u8; 3] = [0xb8, 0x3a, 0x33];
/// Eye colour, matching the app's Open status.
const EYE: [u8; 3] = [0x14, 0x16, 0x1a];

/// Signed coverage test for a shape, in 0..1 icon space.
type Shape = fn(f32, f32) -> bool;

fn body(x: f32, y: f32) -> bool {
    ellipse(x, y, 0.5, 0.56, 0.30, 0.22)
}

fn claws(x: f32, y: f32) -> bool {
    ellipse(x, y, 0.16, 0.34, 0.13, 0.11) || ellipse(x, y, 0.84, 0.34, 0.13, 0.11)
}

/// Six legs, three each side, as short thick strokes fanning out and down.
fn legs(x: f32, y: f32) -> bool {
    const LEGS: [(f32, f32, f32, f32); 6] = [
        (0.26, 0.58, 0.06, 0.72),
        (0.30, 0.66, 0.13, 0.82),
        (0.38, 0.72, 0.26, 0.90),
        (0.74, 0.58, 0.94, 0.72),
        (0.70, 0.66, 0.87, 0.82),
        (0.62, 0.72, 0.74, 0.90),
    ];
    LEGS.iter()
        .any(|(x0, y0, x1, y1)| stroke(x, y, *x0, *y0, *x1, *y1, 0.035))
}

fn eyes(x: f32, y: f32) -> bool {
    ellipse(x, y, 0.41, 0.48, 0.045, 0.05) || ellipse(x, y, 0.59, 0.48, 0.045, 0.05)
}

fn ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    let dx = (x - cx) / rx;
    let dy = (y - cy) / ry;
    dx * dx + dy * dy <= 1.0
}

/// Distance from a point to a segment, thresholded to a thickness.
fn stroke(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, half_width: f32) -> bool {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq <= f32::EPSILON {
        0.0
    } else {
        (((x - x0) * dx + (y - y0) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let (px, py) = (x0 + t * dx, y0 + t * dy);
    let (ex, ey) = (x - px, y - py);
    ex * ex + ey * ey <= half_width * half_width
}

/// Averaged coverage of one shape over a single pixel.
fn coverage(shape: Shape, px: u32, py: u32) -> f32 {
    let mut hits = 0u32;
    for sy in 0..SS {
        for sx in 0..SS {
            #[expect(clippy::cast_precision_loss, reason = "values are far below 2^24")]
            let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / SIZE as f32;
            #[expect(clippy::cast_precision_loss, reason = "values are far below 2^24")]
            let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / SIZE as f32;
            if shape(x, y) {
                hits += 1;
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "values are far below 2^24")]
    let total = (SS * SS) as f32;
    #[expect(clippy::cast_precision_loss, reason = "values are far below 2^24")]
    let hits = hits as f32;
    hits / total
}

/// Alpha blends `colour` over an RGBA pixel at the given coverage.
fn blend(pixel: &mut [u8], colour: [u8; 3], alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..255"
    )]
    let mix = |dst: u8, src: u8| -> u8 {
        (f32::from(src) * alpha + f32::from(dst) * (1.0 - alpha)).round() as u8
    };
    pixel[0] = mix(pixel[0], colour[0]);
    pixel[1] = mix(pixel[1], colour[1]);
    pixel[2] = mix(pixel[2], colour[2]);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..255"
    )]
    let a = (f32::from(pixel[3]) + 255.0 * alpha).min(255.0).round() as u8;
    pixel[3] = a;
}

/// The icon as premultiplied-free RGBA, which is what eframe expects.
#[must_use]
pub fn rgba() -> Vec<u8> {
    let mut buf = vec![0u8; (SIZE * SIZE * 4) as usize];
    // Back to front: legs, then claws, then body over their inner ends, then
    // eyes on top.
    let layers: [(Shape, [u8; 3]); 4] = [
        (legs, SHELL_DARK),
        (claws, SHELL),
        (body, SHELL),
        (eyes, EYE),
    ];
    for (shape, colour) in layers {
        for py in 0..SIZE {
            for px in 0..SIZE {
                let alpha = coverage(shape, px, py);
                let index = ((py * SIZE + px) * 4) as usize;
                blend(&mut buf[index..index + 4], colour, alpha);
            }
        }
    }
    buf
}

/// Ready for [`eframe::egui::ViewportBuilder::with_icon`].
#[must_use]
pub fn icon_data() -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: rgba(),
        width: SIZE,
        height: SIZE,
    }
}

/// Edge length, exposed for tests.
#[must_use]
pub const fn size() -> u32 {
    SIZE
}
