//! The generated window icon.
//!
//! Drawn in code, so these guard the properties that make it look like a crab
//! rather than a red square, and that make it a valid buffer for eframe.

use trackcrab::icon;

fn pixel(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let n = icon::size();
    let i = ((y * n + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn the_buffer_is_exactly_the_size_eframe_expects() {
    let rgba = icon::rgba();
    let n = icon::size();
    assert_eq!(rgba.len(), (n * n * 4) as usize);

    let data = icon::icon_data();
    assert_eq!(data.width, n);
    assert_eq!(data.height, n);
    assert_eq!(data.rgba.len(), (data.width * data.height * 4) as usize);
}

#[test]
fn the_corners_are_transparent_so_it_is_a_crab_not_a_square() {
    let rgba = icon::rgba();
    let last = icon::size() - 1;
    for (x, y) in [(0, 0), (last, 0), (0, last), (last, last)] {
        assert_eq!(
            pixel(&rgba, x, y)[3],
            0,
            "corner ({x}, {y}) should be fully transparent"
        );
    }
}

#[test]
fn the_middle_is_opaque_shell_red() {
    let rgba = icon::rgba();
    let centre = icon::size() / 2;
    // Slightly below centre, between the eyes and the lower body edge.
    let [r, g, b, a] = pixel(&rgba, centre, centre + 6);
    assert_eq!(a, 255, "the body should be fully opaque");
    assert!(
        r > g && r > b && r > 150,
        "the body should be shell red, got ({r}, {g}, {b})"
    );
}

#[test]
fn the_eyes_are_darker_than_the_shell_around_them() {
    let rgba = icon::rgba();
    let n = icon::size();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "constants well within u32"
    )]
    let at = |fx: f32, fy: f32| -> [u8; 4] {
        #[expect(clippy::cast_precision_loss, reason = "64 is exact in f32")]
        let n = n as f32;
        pixel(&rgba, (fx * n) as u32, (fy * n) as u32)
    };
    let eye = at(0.41, 0.48);
    let shell = at(0.50, 0.62);
    assert!(
        u32::from(eye[0]) + u32::from(eye[1]) + u32::from(eye[2])
            < u32::from(shell[0]) + u32::from(shell[1]) + u32::from(shell[2]),
        "the eye at 0.41,0.48 should be darker than the shell"
    );
}

#[test]
fn something_is_drawn_across_most_of_the_canvas() {
    // A crab with claws and legs should cover a good part of the square without
    // filling it, which catches both an empty buffer and a solid one.
    let rgba = icon::rgba();
    let total = icon::size() * icon::size();
    let opaque = u32::try_from(rgba.chunks_exact(4).filter(|p| p[3] > 128).count())
        .expect("the icon has far fewer than u32::MAX pixels");
    let fraction = f64::from(opaque) / f64::from(total);
    assert!(
        (0.20..0.75).contains(&fraction),
        "coverage of {fraction:.2} looks wrong for a crab silhouette"
    );
}

#[test]
fn generating_it_twice_gives_the_same_bytes() {
    assert_eq!(icon::rgba(), icon::rgba(), "the icon must be deterministic");
}
