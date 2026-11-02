use super::*;

const SANS: &[u8] = include_bytes!("../../../../assets/fonts/NotoSans.ttf");
const ARABIC: &[u8] = include_bytes!("../../../../assets/fonts/NotoSansArabic.ttf");
const FAMILIES: &[&str] = &["Noto Sans", "Noto Sans Arabic"];

fn rasterizer() -> Rasterizer {
    let mut raster = Rasterizer::new();
    assert_eq!(raster.register_font(SANS.to_vec()).unwrap(), ["Noto Sans"]);
    assert_eq!(
        raster.register_font(ARABIC.to_vec()).unwrap(),
        ["Noto Sans Arabic"]
    );
    raster
}

#[test]
fn mixed_scripts_use_explicit_fallback_and_rtl_shaping() {
    let mut raster = rasterizer();
    let text = raster
        .rasterize(
            "Zest office\nПривет, мир!\nالعربية",
            FAMILIES,
            RasterOptions::default(),
        )
        .unwrap();
    assert_eq!(text.metrics.lines, 3);
    assert_eq!(text.metrics.fonts, 2);
    assert_eq!(text.metrics.missing_glyphs, 0);
    assert!(text.metrics.rtl_runs > 0);
    assert!(text.metrics.glyphs > 16);
    assert!(text.rgba8.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255));
    assert!(
        text.rgba8
            .chunks_exact(4)
            .filter(|p| p[3] > 0)
            .all(|p| p[..3] == [255; 3])
    );
    let ligature = raster
        .rasterize("fi", FAMILIES, RasterOptions::default())
        .unwrap();
    assert!(
        ligature.metrics.glyphs < 2,
        "the fixture's Latin fi ligature must be shaped"
    );
    assert_eq!(ligature.metrics.missing_glyphs, 0);
    // This Noto Arabic face joins lam/alef using contextual forms, retaining
    // two glyphs. A zero-width non-joiner must change the resulting ink.
    let joined = raster
        .rasterize("لا", FAMILIES, RasterOptions::default())
        .unwrap();
    let separated = raster
        .rasterize("ل\u{200c}ا", FAMILIES, RasterOptions::default())
        .unwrap();
    assert_ne!(joined.rgba8, separated.rgba8);
}

#[test]
fn wrapping_and_fractional_dpi_preserve_logical_metrics() {
    let mut raster = rasterizer();
    let sample = "Привет office e\u{301} العربية";
    let wide = raster
        .rasterize(sample, FAMILIES, RasterOptions::default())
        .unwrap();
    let dpi = raster
        .rasterize(
            sample,
            FAMILIES,
            RasterOptions {
                scale: 1.5,
                ..Default::default()
            },
        )
        .unwrap();
    assert!((wide.metrics.width - dpi.metrics.width).abs() < 1.0);
    assert!((wide.metrics.height - dpi.metrics.height).abs() < 1.0);
    assert_eq!(wide.metrics.lines, dpi.metrics.lines);
    assert_eq!(wide.metrics.glyphs, dpi.metrics.glyphs);
    assert!(dpi.width > wide.width && dpi.height > wide.height);
    let narrow = raster
        .rasterize(
            sample,
            FAMILIES,
            RasterOptions {
                width: 95.0,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(narrow.metrics.lines > wide.metrics.lines);
    assert!(narrow.height > wide.height);
}

#[test]
fn negative_overhangs_are_included_and_repeated_calls_reproduce_pixels() {
    let mut raster = rasterizer();
    let options = RasterOptions {
        font_size: 32.0,
        scale: 1.25,
        ..Default::default()
    };
    let before = raster.rasterize("j\u{301}", FAMILIES, options).unwrap();
    assert!(
        before.origin_x < 0,
        "fixture must exercise a left glyph overhang"
    );
    raster
        .rasterize("عربي", FAMILIES, RasterOptions::default())
        .unwrap();
    let after = raster.rasterize("j\u{301}", FAMILIES, options).unwrap();
    assert_eq!(before, after);
    assert_eq!(
        before.rgba8.len(),
        before.width as usize * before.height as usize * 4
    );
    assert!(before.rgba8.chunks_exact(4).any(|p| p[3] != 0));
}

#[test]
fn missing_characters_are_reported_and_whitespace_is_transparent() {
    let mut raster = rasterizer();
    let missing = raster
        .rasterize("日本👩", FAMILIES, RasterOptions::default())
        .unwrap();
    assert!(missing.metrics.missing_glyphs >= 3);
    for sample in ["", "  ", "\n"] {
        let blank = raster
            .rasterize(sample, FAMILIES, RasterOptions::default())
            .unwrap();
        assert!(blank.rgba8.chunks_exact(4).all(|p| p[3] == 0));
        assert!(blank.width >= 1 && blank.height >= 1);
    }
}

#[test]
fn invalid_inputs_and_budgets_fail_without_poisoning_later_work() {
    let mut raster = Rasterizer::new();
    assert_eq!(
        raster.register_font(vec![0; 100]),
        Err(RasterError::InvalidFont)
    );
    assert_eq!(
        raster.register_font(vec![0; MAX_FONT_BYTES + 1]),
        Err(RasterError::FontBudgetExceeded)
    );
    assert!(matches!(
        raster.rasterize("x", &["Noto Sans"], RasterOptions::default()),
        Err(RasterError::UnknownFamily(_))
    ));
    raster.register_font(SANS.to_vec()).unwrap();
    for options in [
        RasterOptions {
            font_size: f32::NAN,
            ..Default::default()
        },
        RasterOptions {
            font_size: 0.0,
            ..Default::default()
        },
        RasterOptions {
            font_size: 257.0,
            ..Default::default()
        },
        RasterOptions {
            scale: f32::INFINITY,
            ..Default::default()
        },
        RasterOptions {
            scale: -1.0,
            ..Default::default()
        },
        RasterOptions {
            scale: 0.0,
            ..Default::default()
        },
        RasterOptions {
            width: f32::NAN,
            ..Default::default()
        },
        RasterOptions {
            width: 4097.0,
            ..Default::default()
        },
    ] {
        assert_eq!(
            raster.rasterize("x", &["Noto Sans"], options).unwrap_err(),
            RasterError::InvalidOptions
        );
    }
    assert_eq!(
        raster
            .rasterize("x", &[], RasterOptions::default())
            .unwrap_err(),
        RasterError::InvalidOptions
    );
    assert_eq!(
        raster
            .rasterize(
                &"x".repeat(MAX_TEXT_BYTES + 1),
                &["Noto Sans"],
                RasterOptions::default()
            )
            .unwrap_err(),
        RasterError::InputTooLarge
    );
    assert_eq!(
        raster
            .rasterize(&"\n".repeat(5000), &["Noto Sans"], RasterOptions::default())
            .unwrap_err(),
        RasterError::LayoutTooLarge
    );
    assert!(
        raster
            .rasterize("still works", &["Noto Sans"], RasterOptions::default())
            .is_ok()
    );
}
