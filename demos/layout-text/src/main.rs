use zest::{
    Scene,
    geometry::Size,
    layout::{Edges, LayoutTree, Length, Style, TextContent},
    text::raster::{Rasterizer, ShapeOptions},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "Zest • Привет, мир! office e\u{301} • العربية one two three four five six seven";
    let mut raster = Rasterizer::new();
    raster.register_font(include_bytes!("../../../assets/fonts/NotoSans.ttf").to_vec())?;
    raster.register_font(include_bytes!("../../../assets/fonts/NotoSansArabic.ttf").to_vec())?;
    let content = TextContent::new(raster.shape(
        source,
        &["Noto Sans", "Noto Sans Arabic"],
        ShapeOptions {
            font_size: 18.0,
            scale: 1.25,
        },
    )?);
    let mut scene = Scene::default();
    let (mut label, mut footer) = (None, None);
    scene.update(|ui| {
        label = Some(ui.button("paragraph", source));
        footer = Some(ui.button("footer", "Below paragraph"));
    })?;
    let (label, footer) = (label.unwrap(), footer.unwrap());
    let mut layout = LayoutTree::default();
    let styles = |id: &_| {
        if id == &label {
            (
                Style {
                    padding: Edges::all(8.0),
                    shrink: 0.0,
                    ..Default::default()
                },
                Some(content.clone()),
            )
        } else {
            (
                Style {
                    height: Length::Px(24.0),
                    shrink: 0.0,
                    ..Default::default()
                },
                None,
            )
        }
    };
    layout.sync_with_text(&scene, styles)?;
    let wide = layout.compute(Size::new(480.0, 1000.0))?;
    let narrow = layout.compute(Size::new(180.0, 1000.0))?;
    let wide_height = wide.snapshot.bounds(&label).unwrap().size.height;
    let narrow_height = narrow.snapshot.bounds(&label).unwrap().size.height;
    assert!(narrow_height > wide_height);
    assert_eq!(
        narrow.snapshot.bounds(&footer).unwrap().origin.y,
        narrow_height
    );
    scene.focus(&label);
    scene.set_hovered(&label, true);
    assert_eq!(layout.sync_with_text(&scene, styles)?, Default::default());
    let cached = layout.compute(Size::new(180.0, 1000.0))?;
    assert!(cached.snapshot.same_snapshot(&narrow.snapshot) && !cached.stats.performed);
    let replacement_source = "Updated • Новый текст";
    let replacement = TextContent::new(raster.shape(
        replacement_source,
        &["Noto Sans", "Noto Sans Arabic"],
        ShapeOptions {
            font_size: 18.0,
            scale: 1.25,
        },
    )?);
    scene.update(|ui| {
        assert_eq!(ui.button("paragraph", replacement_source), label);
        assert_eq!(ui.button("footer", "Below paragraph"), footer);
    })?;
    let sync = layout.sync_with_text(&scene, |id| {
        let (style, text) = styles(id);
        (style, text.map(|_| replacement.clone()))
    })?;
    assert_eq!((sync.content_changed, sync.restyled), (1, 0));
    let changed = layout.compute(Size::new(180.0, 1000.0))?;
    assert!(changed.stats.performed && changed.stats.measured_leaves > 0);
    assert_eq!(scene.focused(), Some(&label));
    println!(
        "Text layout: width 480 → height {wide_height:.2}, width 180 → height {narrow_height:.2}"
    );
    println!(
        "Text/layout smoke passed: shared shaping, width-dependent wrapping, sibling movement, padding, paint-only cache reuse and content invalidation without GPU or glyph rasterization."
    );
    Ok(())
}
