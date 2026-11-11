use avilon_egui::optics::{LensError, LensOptions, render_lens};
use egui::{Color32, ColorImage};

fn gradient(w: usize, h: usize) -> ColorImage {
    ColorImage::new(
        [w, h],
        (0..h)
            .flat_map(|y| {
                (0..w).map(move |x| {
                    Color32::from_rgb((x * 255 / (w - 1)) as u8, (y * 255 / (h - 1)) as u8, 80)
                })
            })
            .collect(),
    )
}
#[test]
fn zero_effect_is_identity_and_outside_lens_is_unchanged() {
    let source = gradient(64, 48);
    let zero = LensOptions {
        refraction: 0.0,
        blur: 0.0,
        chromatic: 0.0,
        ..Default::default()
    };
    assert_eq!(render_lens(&source, source.size, zero).unwrap(), source);
    let result = render_lens(&source, source.size, LensOptions::default()).unwrap();
    assert_eq!(result.pixels[0], source.pixels[0]);
    assert_eq!(result.pixels[47 * 64 + 63], source.pixels[47 * 64 + 63]);
}
#[test]
fn gradient_is_refracted_at_edges_and_sampling_clamps_to_image() {
    let source = gradient(64, 64);
    let options = LensOptions {
        rect: [0.2, 0.2, 0.8, 0.8],
        corner_radius: 0.15,
        refraction: 12.0,
        blur: 0.0,
        chromatic: 0.0,
        highlight: 0.0,
    };
    let result = render_lens(&source, source.size, options).unwrap();
    let i = 32 * 64 + 13;
    assert!(result.pixels[i].r() > source.pixels[i].r());
    assert_eq!(result.pixels[32 * 64 + 32], source.pixels[32 * 64 + 32]);
    let edge = LensOptions {
        rect: [0.0, 0.0, 1.0, 1.0],
        refraction: 64.0,
        blur: 8.0,
        chromatic: 1.0,
        ..options
    };
    assert_eq!(
        render_lens(&source, [64, 64], edge).unwrap().pixels.len(),
        4096
    );
}
#[test]
fn rejects_bad_sizes_sources_and_parameters() {
    let source = gradient(4, 4);
    for size in [[0, 4], [4, 0], [513, 1], [1, 513], [usize::MAX, 2]] {
        assert_eq!(
            render_lens(&source, size, Default::default()).unwrap_err(),
            LensError::InvalidSize
        );
    }
    let mut invalid = LensOptions::default();
    invalid.refraction = f32::NAN;
    assert_eq!(
        render_lens(&source, [4, 4], invalid).unwrap_err(),
        LensError::InvalidOptions
    );
    invalid = LensOptions::default();
    invalid.rect = [0.8, 0.0, 0.2, 1.0];
    assert_eq!(
        render_lens(&source, [4, 4], invalid).unwrap_err(),
        LensError::InvalidOptions
    );
    invalid = LensOptions::default();
    invalid.blur = f32::INFINITY;
    assert_eq!(
        render_lens(&source, [4, 4], invalid).unwrap_err(),
        LensError::InvalidOptions
    );
    invalid = LensOptions::default();
    invalid.corner_radius = 0.51;
    assert_eq!(
        render_lens(&source, [4, 4], invalid).unwrap_err(),
        LensError::InvalidOptions
    );
    let malformed = ColorImage {
        size: [4, 4],
        pixels: vec![],
        source_size: egui::vec2(4.0, 4.0),
    };
    assert_eq!(
        render_lens(&malformed, [4, 4], Default::default()).unwrap_err(),
        LensError::InvalidSource
    );
}
