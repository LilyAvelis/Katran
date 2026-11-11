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
        for text in ["🤡⚰️💀🐄🦭".repeat(6), "👩🏽‍💻🇰🇿🌈🐄".repeat(5)] {
            let image = emoji_render::render(&text, size).unwrap_or_else(|e| panic!("size {size}: {e}"));
            assert!(image.width <= 1024 && image.height <= 256);
            assert_eq!(image.rgba.len(), image.width as usize * image.height as usize * 4);
            assert!(image.rgba.chunks_exact(4).any(|p| p[3] > 32 && (p[0].abs_diff(p[1]) > 8 || p[1].abs_diff(p[2]) > 8)));
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
