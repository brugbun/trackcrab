//! Dumps the generated window icon as a PPM, composited on the app's panel
//! colour and upscaled, so it can actually be looked at:
//!
//! ```sh
//! cargo run --example render_icon > icon.ppm
//! ```

use std::io::Write as _;

/// Upscale factor, so the 64px icon is legible on screen.
const SCALE: u32 = 6;
/// The panel colour the icon is composited over.
const BG: [f32; 3] = [26.0, 29.0, 34.0];

fn main() {
    let rgba = trackcrab::icon::rgba();
    let n = trackcrab::icon::size();
    let mut out = Vec::with_capacity((n * SCALE * n * SCALE * 3) as usize);

    for y in 0..n * SCALE {
        for x in 0..n * SCALE {
            let i = (((y / SCALE) * n + (x / SCALE)) * 4) as usize;
            let alpha = f32::from(rgba[i + 3]) / 255.0;
            for (channel, bg) in BG.iter().enumerate() {
                let value = f32::from(rgba[i + channel]) * alpha + bg * (1.0 - alpha);
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "value is a blend of two bytes, so within 0..255"
                )]
                out.push(value.round() as u8);
            }
        }
    }

    let mut stdout = std::io::stdout().lock();
    write!(stdout, "P6\n{} {}\n255\n", n * SCALE, n * SCALE).expect("write header");
    stdout.write_all(&out).expect("write pixels");
}
