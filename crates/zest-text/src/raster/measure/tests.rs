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
