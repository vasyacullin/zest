use super::*;
fn rasterizer() -> Rasterizer {
    let mut raster = Rasterizer::new();
    raster
        .register_font(include_bytes!("../../../../../assets/fonts/NotoSans.ttf").to_vec())
        .unwrap();
    raster
        .register_font(include_bytes!("../../../../../assets/fonts/NotoSansArabic.ttf").to_vec())
        .unwrap();
    raster
}
const FAMILIES: &[&str] = &["Noto Sans", "Noto Sans Arabic"];

#[test]
fn rasterization_from_a_shape_matches_direct_paragraphs_at_multiple_widths_and_dpi() {
    let source = "office e\u{301} Привет\nلا العربية one two three";
    let mut raster = rasterizer();
    for scale in [1.0, 1.25, 1.5] {
        let shaped = raster
            .shape(
                source,
                FAMILIES,
                ShapeOptions {
                    font_size: 18.0,
                    scale,
                },
            )
            .unwrap();
        assert_eq!(shaped.scale(), scale);
        for width in [110.0, 400.0, 110.0] {
            let cached = raster.paragraph_from_shape(&shaped, width).unwrap();
            let direct = raster
                .paragraph(
                    source,
                    FAMILIES,
                    RasterOptions {
                        font_size: 18.0,
                        scale,
                        width,
                    },
                )
                .unwrap();
            assert_eq!(cached.bitmap(), direct.bitmap());
            assert_eq!(cached.text(), source);
            assert_eq!(
                cached.caret(source.len()).unwrap(),
                direct.caret(source.len()).unwrap()
            );
            let measured = shaped.measure(MeasureWidth::Definite(width)).unwrap();
            assert_eq!(measured.height, cached.bitmap().metrics.height);
            assert_eq!(measured.lines, cached.bitmap().metrics.lines);
        }
    }
}

#[test]
fn rasterization_from_a_shape_needs_no_registered_fonts_in_the_destination() {
    let mut source = rasterizer();
    let shaped = source
        .shape("Привет العربية e\u{301}", FAMILIES, ShapeOptions::default())
        .unwrap();
    let expected = source.paragraph_from_shape(&shaped, 200.0).unwrap();
    drop(source);
    let mut destination = Rasterizer::new();
    assert!(
        destination
            .paragraph(shaped.text(), FAMILIES, RasterOptions::default())
            .is_err()
    );
    let restored = destination.paragraph_from_shape(&shaped, 200.0).unwrap();
    assert_eq!(restored.bitmap(), expected.bitmap());
}

#[test]
fn invalid_raster_widths_and_bitmap_budget_failures_leave_shapes_and_paragraphs_valid() {
    let mut raster = rasterizer();
    let shaped = raster
        .shape(
            "e\u{301} one two three",
            FAMILIES,
            ShapeOptions {
                font_size: 18.0,
                scale: 1.25,
            },
        )
        .unwrap();
    let paragraph = raster.paragraph_from_shape(&shaped, 100.0).unwrap();
    let guard = paragraph
        .layout_selection(crate::Selection::caret(0))
        .unwrap();
    for width in [0.0, -1.0, f32::NAN, f32::INFINITY, 4096.0] {
        assert!(raster.paragraph_from_shape(&shaped, width).is_err());
    }
    assert!(shaped.measure(MeasureWidth::Definite(0.0)).is_ok());
    let next = raster.paragraph_from_shape(&shaped, 100.0).unwrap();
    assert_eq!(paragraph.bitmap(), next.bitmap());
    paragraph.selection_caret(&guard).unwrap();
    assert!(matches!(
        next.selection_caret(&guard),
        Err(RasterError::ForeignSelection)
    ));
    let large = raster
        .shape(&"one\n".repeat(5000), FAMILIES, ShapeOptions::default())
        .unwrap();
    assert!(matches!(
        raster.paragraph_from_shape(&large, 200.0),
        Err(RasterError::LayoutTooLarge)
    ));
    assert!(large.measure(MeasureWidth::Definite(200.0)).is_ok());
    assert_eq!(
        raster
            .paragraph_from_shape(&shaped, 100.0)
            .unwrap()
            .bitmap(),
        paragraph.bitmap()
    );
}

#[test]
fn measuring_reflows_the_same_shape_and_matches_rasterized_line_metrics() {
    let text = "office Привет e\u{301} العربية one two three four five\nlast ";
    let mut raster = rasterizer();
    for scale in [1.0, 1.25, 1.5] {
        let shape = raster
            .shape(
                text,
                FAMILIES,
                ShapeOptions {
                    font_size: 18.0,
                    scale,
                },
            )
            .unwrap();
        let wide = shape.measure(MeasureWidth::Definite(480.0)).unwrap();
        let narrow = shape.measure(MeasureWidth::Definite(120.0)).unwrap();
        assert!(narrow.lines > wide.lines && narrow.height > wide.height);
        assert_eq!(shape.measure(MeasureWidth::Definite(480.0)).unwrap(), wide);
        for width in [120.0, 480.0] {
            let measured = shape.measure(MeasureWidth::Definite(width)).unwrap();
            let paragraph = raster
                .paragraph(
                    text,
                    FAMILIES,
                    RasterOptions {
                        font_size: 18.0,
                        scale,
                        width,
                    },
                )
                .unwrap();
            assert_eq!(measured.lines, paragraph.bitmap().metrics.lines);
            assert_eq!(measured.height, paragraph.bitmap().metrics.height);
            assert!((measured.width - paragraph.layout.full_width() / scale).abs() < 1e-4);
        }
        assert_eq!(shape.text(), text);
    }
}

#[test]
fn intrinsic_widths_use_real_line_breaks_and_preserve_whitespace_and_empty_lines() {
    let mut raster = rasterizer();
    for text in ["", "one\n", "  one two  ", "one العربية e\u{301} 👩🏽‍💻 two"] {
        let shape = raster
            .shape(text, FAMILIES, ShapeOptions::default())
            .unwrap();
        let min = shape.measure(MeasureWidth::MinContent).unwrap();
        let max = shape.measure(MeasureWidth::MaxContent).unwrap();
        assert!(min.width <= max.width + 1e-4, "{text:?}, {min:?}, {max:?}");
        assert!(min.lines >= max.lines);
        assert_eq!(shape.measure(MeasureWidth::Definite(0.0)).unwrap(), min);
        assert!(max.lines >= if text.ends_with('\n') { 2 } else { 1 });
    }
}

#[test]
fn measurement_is_independent_of_bitmap_budgets_and_keeps_font_data_alive() {
    let mut raster = rasterizer();
    let text = "one\n".repeat(5000);
    let shape = raster
        .shape(&text, FAMILIES, ShapeOptions::default())
        .unwrap();
    assert!(
        raster
            .paragraph(&text, FAMILIES, RasterOptions::default())
            .is_err()
    );
    drop(raster);
    let measurement = shape.measure(MeasureWidth::Definite(200.0)).unwrap();
    assert_eq!(measurement.lines, 5001);
    assert!(measurement.height > 4096.0);
}

#[test]
fn invalid_inputs_do_not_poison_subsequent_shaping_or_measurement() {
    let mut raster = rasterizer();
    assert!(matches!(
        raster.shape("text", &["missing"], ShapeOptions::default()),
        Err(RasterError::UnknownFamily(_))
    ));
    assert!(raster.shape("text", &[], ShapeOptions::default()).is_err());
    assert!(
        raster
            .shape(
                &"x".repeat(MAX_TEXT_BYTES + 1),
                FAMILIES,
                ShapeOptions::default()
            )
            .is_err()
    );
    for options in [
        ShapeOptions {
            font_size: f32::NAN,
            scale: 1.0,
        },
        ShapeOptions {
            font_size: 0.0,
            scale: 1.0,
        },
        ShapeOptions {
            font_size: 18.0,
            scale: 0.0,
        },
    ] {
        assert!(raster.shape("text", FAMILIES, options).is_err());
    }
    let shape = raster
        .shape("text", FAMILIES, ShapeOptions::default())
        .unwrap();
    let valid = shape.measure(MeasureWidth::Definite(100.0)).unwrap();
    for width in [f32::NAN, -1.0, f32::INFINITY] {
        assert!(shape.measure(MeasureWidth::Definite(width)).is_err());
    }
    assert_eq!(shape.measure(MeasureWidth::Definite(100.0)).unwrap(), valid);
}
