use katran::emoji_render;
#[test]
fn input_limits_are_explicit() {
    assert!(emoji_render::render("", 32.0).is_err());
    assert!(emoji_render::render("x", f32::NAN).is_err());
    assert!(emoji_render::render(&"x".repeat(129), 32.0).is_err());
    assert!(emoji_render::render("x\ny", 32.0).is_err());
}
#[cfg(windows)]
#[test]
fn native_color_emoji_renders_in_memory() {
    for text in ["🐄", "🌈", "👩🏽‍💻", "🇰🇿", "🇨🇳", "🇷🇺", "👩🏽‍💻 🇰🇿 🌈"]
    {
        let image =
            emoji_render::render(text, 64.0).unwrap_or_else(|error| panic!("{text}: {error}"));
        assert!(image.width > 0 && image.width <= 1024 && image.height > 0 && image.height <= 256);
        assert_eq!(
            image.rgba.len(),
            image.width as usize * image.height as usize * 4
        );
        assert!(
            image
                .rgba
                .chunks_exact(4)
                .any(|p| p[3] > 32 && (p[0].abs_diff(p[1]) > 8 || p[1].abs_diff(p[2]) > 8)),
            "{text} should contain visible color"
        );
    }
}
#[cfg(windows)]
#[test]
fn long_combinations_render_in_color_at_all_card_sizes() {
    for size in [32.0, 64.0, 128.0] {
        for text in ["🤡⚰️💀🐄🦭".repeat(6), "👩🏽‍💻🇰🇿🌈🐄".repeat(5)]
        {
            let image =
                emoji_render::render(&text, size).unwrap_or_else(|e| panic!("size {size}: {e}"));
            assert!(image.width <= 1024 && image.height <= 256);
            assert_eq!(
                image.rgba.len(),
                image.width as usize * image.height as usize * 4
            );
            assert!(
                image
                    .rgba
                    .chunks_exact(4)
                    .any(|p| p[3] > 32 && (p[0].abs_diff(p[1]) > 8 || p[1].abs_diff(p[2]) > 8))
            );
        }
    }
}
#[cfg(not(windows))]
#[test]
fn unsupported_platform_is_explicit() {
    assert!(
        emoji_render::render("🐄", 32.0)
            .unwrap_err()
            .contains("Windows")
    );
}

#[cfg(windows)]
#[test]
fn private_font_rejects_missing_and_invalid_files_even_for_flags() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing.ttf");
    assert!(emoji_render::probe_font(&missing).is_err());
    assert!(emoji_render::render_with_font("🐄", 32.0, Some(&missing)).is_err());
    assert!(emoji_render::render_with_font("🇰🇿", 32.0, Some(&missing)).is_err());
    let invalid = dir.path().join("invalid.ttf");
    std::fs::write(&invalid, b"synthetic invalid font").unwrap();
    assert!(emoji_render::probe_font(&invalid).is_err());
    assert!(emoji_render::render_with_font("🐄", 32.0, Some(&invalid)).is_err());
}

#[cfg(windows)]
#[test]
#[ignore = "manual native test; requires KATRAN_EMOJI_FONT_TEST"]
fn locally_selected_private_font() {
    use std::{path::PathBuf, time::Instant};
    let path = PathBuf::from(
        std::env::var_os("KATRAN_EMOJI_FONT_TEST").expect("KATRAN_EMOJI_FONT_TEST required"),
    );
    println!(
        "family: {}",
        emoji_render::probe_font(&path).expect("private font")
    );
    let start = Instant::now();
    let long = "🐄🤡🐡👩🏽\u{200d}💻🌈".repeat(5);
    let examples = ["🐡", "🐄", "🤡", "👩🏽‍💻", "🌈", long.as_str()];
    let mut images = Vec::new();
    for text in examples {
        let rendered =
            emoji_render::render_with_font(text, 64.0, Some(&path)).expect("private rendering");
        assert!(
            rendered.width > 0
                && rendered.width <= 1024
                && rendered.height > 0
                && rendered.height <= 256
        );
        assert_eq!(
            rendered.rgba.len(),
            rendered.width as usize * rendered.height as usize * 4
        );
        assert!(
            rendered
                .rgba
                .chunks_exact(4)
                .any(|p| p[3] > 32 && (p[0].abs_diff(p[1]) > 8 || p[1].abs_diff(p[2]) > 8)),
            "no color for {text}"
        );
        images.push(rendered);
    }
    let system = emoji_render::render("🐄", 64.0).expect("system rendering");
    assert!(
        images[1].width != system.width
            || images[1].height != system.height
            || images[1].rgba != system.rgba,
        "private and system cow rendered identically"
    );
    let width = images
        .iter()
        .map(|r| r.width)
        .chain([system.width])
        .max()
        .unwrap();
    let height = images.iter().map(|r| r.height + 12).sum::<u32>() + system.height + 12;
    let mut sheet = image::RgbaImage::from_pixel(width, height, image::Rgba([240, 240, 240, 255]));
    let mut y = 0;
    for rendered in images.iter().chain(std::iter::once(&system)) {
        let src =
            image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.rgba.clone())
                .unwrap();
        image::imageops::overlay(&mut sheet, &src, 0, y as i64);
        y += rendered.height + 12;
    }
    std::fs::create_dir_all("target").unwrap();
    sheet.save("target/apple-emoji-proof.png").unwrap();
    println!(
        "six renders, probe, comparison and PNG: {:?}",
        start.elapsed()
    );
    let repeated = Instant::now();
    for _ in 0..10 {
        emoji_render::render_with_font("🐄", 64.0, Some(&path)).unwrap();
    }
    println!("10 repeated private renders: {:?}", repeated.elapsed());
}
