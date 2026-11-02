use super::*;
use crate::*;
use zest_core::{Scene, geometry::Size};
use zest_text::raster::{MeasureWidth, Rasterizer, ShapeOptions};

fn rasterizer() -> Rasterizer {
    let mut raster = Rasterizer::new();
    raster
        .register_font(include_bytes!("../../../../assets/fonts/NotoSans.ttf").to_vec())
        .unwrap();
    raster
        .register_font(include_bytes!("../../../../assets/fonts/NotoSansArabic.ttf").to_vec())
        .unwrap();
    raster
}
fn content(raster: &mut Rasterizer, text: &str) -> TextContent {
    TextContent::new(
        raster
            .shape(
                text,
                &["Noto Sans", "Noto Sans Arabic"],
                ShapeOptions::default(),
            )
            .unwrap(),
    )
}
fn scene() -> (Scene, zest_core::WidgetId, zest_core::WidgetId) {
    let mut scene = Scene::default();
    let (mut text, mut footer) = (None, None);
    scene
        .update(|ui| {
            text = Some(ui.button("text", "text"));
            footer = Some(ui.button("footer", "footer"));
        })
        .unwrap();
    (scene, text.unwrap(), footer.unwrap())
}

#[test]
fn text_wraps_to_available_width_and_repositions_the_following_sibling() {
    let mut raster = rasterizer();
    let content = content(
        &mut raster,
        "one two Привет العربية e\u{301} three four five six seven",
    );
    let (scene, text, footer) = scene();
    let mut tree = LayoutTree::default();
    tree.sync_with_text(&scene, |id| {
        if id == &text {
            (
                Style {
                    shrink: 0.0,
                    ..Default::default()
                },
                Some(content.clone()),
            )
        } else {
            (
                Style {
                    height: Length::Px(20.0),
                    shrink: 0.0,
                    ..Default::default()
                },
                None,
            )
        }
    })
    .unwrap();
    let wide = tree.compute(Size::new(400.0, 1000.0)).unwrap();
    let narrow = tree.compute(Size::new(120.0, 1000.0)).unwrap();
    assert!(
        narrow.snapshot.bounds(&text).unwrap().size.height
            > wide.snapshot.bounds(&text).unwrap().size.height
    );
    assert_eq!(
        narrow.snapshot.bounds(&footer).unwrap().origin.y,
        narrow.snapshot.bounds(&text).unwrap().size.height
    );
    let expected = content
        .shaped()
        .measure(MeasureWidth::Definite(120.0))
        .unwrap();
    assert!(
        (narrow.snapshot.bounds(&text).unwrap().size.height - f64::from(expected.height)).abs()
            < 1e-4
    );
    let cached = tree.compute(Size::new(120.0, 1000.0)).unwrap();
    assert_eq!(cached.stats, ComputeStats::default());
    assert!(cached.snapshot.same_snapshot(&narrow.snapshot));
    assert!(!narrow.snapshot.same_snapshot(&wide.snapshot));
}

#[test]
fn shared_content_keeps_caches_and_replacement_invalidates_only_content_inputs() {
    let mut raster = rasterizer();
    let original = content(&mut raster, "one two three");
    let replacement = content(&mut raster, "one two three four five six seven eight nine");
    assert_eq!(original, original.clone());
    assert_ne!(original, replacement);
    let (mut scene, text, _) = scene();
    let mut tree = LayoutTree::default();
    let sync = |tree: &mut LayoutTree, scene: &Scene, content: &TextContent| {
        tree.sync_with_text(scene, |id| {
            (
                Style {
                    shrink: 0.0,
                    ..Default::default()
                },
                (id == &text).then(|| content.clone()),
            )
        })
        .unwrap()
    };
    sync(&mut tree, &scene, &original);
    let first = tree.compute(Size::new(130.0, 1000.0)).unwrap();
    scene.focus(&text);
    scene.set_hovered(&text, true);
    assert_eq!(sync(&mut tree, &scene, &original), SyncStats::default());
    assert!(
        tree.compute(Size::new(130.0, 1000.0))
            .unwrap()
            .snapshot
            .same_snapshot(&first.snapshot)
    );
    let stats = sync(&mut tree, &scene, &replacement);
    assert_eq!((stats.content_changed, stats.restyled), (1, 0));
    let changed = tree.compute(Size::new(130.0, 1000.0)).unwrap();
    assert!(changed.stats.measured_leaves > 0);
    assert!(
        changed.snapshot.bounds(&text).unwrap().size.height
            > first.snapshot.bounds(&text).unwrap().size.height
    );
    assert_eq!(
        tree.sync(&scene, |_| Style::default())
            .unwrap()
            .content_changed,
        1
    );
    assert_eq!(
        tree.compute(Size::new(130.0, 1000.0))
            .unwrap()
            .snapshot
            .bounds(&text)
            .unwrap()
            .size
            .height,
        0.0
    );
}

#[test]
fn padding_and_known_sizes_are_respected_for_empty_multiline_and_rtl_content() {
    let mut raster = rasterizer();
    let (scene, text, _) = scene();
    let mut tree = LayoutTree::default();
    for source in ["", "one\n", "العربية one e\u{301} 👩🏽‍💻 two three four"] {
        let content = content(&mut raster, source);
        tree.sync_with_text(&scene, |id| {
            (
                Style {
                    padding: Edges::all(10.0),
                    shrink: 0.0,
                    ..Default::default()
                },
                (id == &text).then(|| content.clone()),
            )
        })
        .unwrap();
        let result = tree.compute(Size::new(200.0, 2000.0)).unwrap();
        let expected = content
            .shaped()
            .measure(MeasureWidth::Definite(180.0))
            .unwrap();
        assert!(
            (result.snapshot.bounds(&text).unwrap().size.height
                - f64::from(expected.height)
                - 20.0)
                .abs()
                < 1e-4
        );
        tree.sync_with_text(&scene, |id| {
            (
                Style {
                    width: Length::Px(200.0),
                    height: Length::Px(25.0),
                    padding: Edges::all(10.0),
                    shrink: 0.0,
                    ..Default::default()
                },
                (id == &text).then(|| content.clone()),
            )
        })
        .unwrap();
        assert_eq!(
            tree.compute(Size::new(400.0, 1000.0))
                .unwrap()
                .snapshot
                .bounds(&text)
                .unwrap()
                .size
                .height,
            25.0
        );
    }
}

#[test]
fn text_on_nonempty_containers_is_rejected_before_mutation() {
    let mut raster = rasterizer();
    let content = content(&mut raster, "text");
    let mut scene = Scene::default();
    let mut root = None;
    scene
        .update(|ui| {
            root = Some(ui.container("root", |ui| {
                ui.button("leaf", "leaf");
            }));
        })
        .unwrap();
    let root = root.unwrap();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |_| Style::default()).unwrap();
    let old = tree.compute(Size::new(200.0, 200.0)).unwrap().snapshot;
    assert!(matches!(
        tree.sync_with_text(&scene, |id| (
            Style {
                gap: 50.0,
                ..Default::default()
            },
            (id == &root).then(|| content.clone())
        )),
        Err(LayoutError::TextOnContainer(_))
    ));
    assert!(tree.snapshot().unwrap().same_snapshot(&old));
}

#[test]
fn measurement_failure_clears_placeholder_caches_and_recovers_without_publishing_bad_bounds() {
    let mut raster = rasterizer();
    let content = TextContent::new(
        raster
            .shape(
                "one two three",
                &["Noto Sans"],
                ShapeOptions {
                    font_size: 20.0,
                    scale: 2.0,
                },
            )
            .unwrap(),
    );
    let (scene, text, _) = scene();
    let mut tree = LayoutTree::default();
    let sync = |tree: &mut LayoutTree, width| {
        tree.sync_with_text(&scene, |id| {
            (
                Style {
                    width: Length::Px(width),
                    shrink: 0.0,
                    ..Default::default()
                },
                (id == &text).then(|| content.clone()),
            )
        })
        .unwrap()
    };
    sync(&mut tree, 100.0);
    let old = tree.compute(Size::new(400.0, 1000.0)).unwrap().snapshot;
    sync(&mut tree, f32::MAX);
    for _ in 0..2 {
        assert!(matches!(
            tree.compute(Size::new(400.0, 1000.0)),
            Err(LayoutError::TextMeasurement { .. })
        ));
        assert!(tree.snapshot().is_none());
    }
    sync(&mut tree, 100.0);
    let recovered = tree.compute(Size::new(400.0, 1000.0)).unwrap();
    assert_eq!(recovered.snapshot.bounds(&text), old.bounds(&text));
    assert!(recovered.stats.measured_leaves > 0);
}
