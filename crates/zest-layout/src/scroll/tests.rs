use super::*;

struct Fixture {
    scene: Scene,
    root: WidgetId,
    inner: WidgetId,
    leaf: WidgetId,
    outside: WidgetId,
}
fn fixture() -> Fixture {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("scroll", |ui| {
                ui.container("inner", |ui| {
                    ui.button("leaf", "Leaf");
                });
                ui.button("footer", "Footer");
            });
            ui.button("outside", "Outside");
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let outside = scene.roots()[1].clone();
    let inner = scene.children(&root).unwrap()[0].clone();
    let leaf = scene.children(&inner).unwrap()[0].clone();
    Fixture {
        scene,
        root,
        inner,
        leaf,
        outside,
    }
}
impl Fixture {
    fn style(
        &self,
        id: &WidgetId,
        offset: Point<Logical>,
        inner_clip: bool,
        inner_offset: Point<Logical>,
    ) -> Style {
        if id == &self.root {
            Style {
                width: Length::Percent(1.0),
                height: Length::Percent(1.0),
                padding: Edges::all(10.0),
                clip_children: true,
                scroll_offset: offset,
                shrink: 0.0,
                ..Default::default()
            }
        } else if id == &self.inner {
            Style {
                width: Length::Px(150.0),
                height: Length::Px(120.0),
                padding: Edges::all(5.0),
                clip_children: inner_clip,
                scroll_offset: inner_offset,
                shrink: 0.0,
                ..Default::default()
            }
        } else if id == &self.leaf {
            Style {
                width: Length::Px(200.0),
                height: Length::Px(160.0),
                shrink: 0.0,
                ..Default::default()
            }
        } else {
            Style {
                width: Length::Px(30.0),
                height: Length::Px(10.0),
                shrink: 0.0,
                ..Default::default()
            }
        }
    }
}
fn zero() -> Point<Logical> {
    Point::new(0.0, 0.0)
}

#[test]
fn positive_overflow_includes_unclipped_descendants_and_trailing_padding() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| f.style(id, zero(), false, zero()))
        .unwrap();
    let frame = tree.compute(Size::new(100.0, 80.0)).unwrap().snapshot;
    assert_eq!(
        frame.scroll(&f.root).unwrap().max_offset,
        Point::new(130.0, 110.0)
    );
    assert_eq!(frame.scroll(&f.inner).unwrap(), ScrollMetrics::default());
    assert_eq!(frame.scroll(&f.leaf).unwrap(), ScrollMetrics::default());
    tree.sync(&f.scene, |id| f.style(id, zero(), true, zero()))
        .unwrap();
    let nested = tree.compute(Size::new(100.0, 80.0)).unwrap();
    assert_eq!(nested.stats, ComputeStats::default());
    assert_eq!(
        nested.snapshot.scroll(&f.root).unwrap().max_offset,
        Point::new(70.0, 70.0)
    );
    assert_eq!(
        nested.snapshot.scroll(&f.inner).unwrap().max_offset,
        Point::new(60.0, 50.0)
    );
}

#[test]
fn fractional_nested_scrolls_project_children_and_clips_but_not_layout_bounds_or_sibling_roots() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let viewport = Size::new(100.0, 80.0);
    tree.sync(&f.scene, |id| f.style(id, zero(), true, zero()))
        .unwrap();
    let old = tree.compute(viewport).unwrap().snapshot;
    tree.sync(&f.scene, |id| {
        f.style(id, Point::new(20.25, 30.5), true, Point::new(6.5, 7.25))
    })
    .unwrap();
    let computed = tree.compute(viewport).unwrap();
    assert_eq!(computed.stats, ComputeStats::default());
    let frame = computed.snapshot;
    assert_eq!(frame.bounds(&f.root), old.bounds(&f.root));
    assert_eq!(frame.bounds(&f.outside), old.bounds(&f.outside));
    assert_eq!(
        frame.bounds(&f.inner).unwrap().origin,
        Point::new(-10.25, -20.5)
    );
    assert_eq!(
        frame.bounds(&f.leaf).unwrap().origin,
        Point::new(-11.75, -22.75)
    );
    for node in frame.boxes() {
        assert_eq!(node.layout_bounds, old.bounds(&node.id).unwrap());
    }
    assert_eq!(
        frame.visible_bounds(&f.leaf),
        Some(Rect::from_xywh(0.0, 0.0, 100.0, 80.0))
    );
    assert_eq!(
        frame.hit_test(Point::new(1.0, 1.0), |id| id == &f.leaf),
        Some(&f.leaf)
    );
    assert!(
        old.hit_test(Point::new(1.0, 1.0), |id| id == &f.leaf)
            .is_none()
    );
    assert!(
        tree.compute(viewport)
            .unwrap()
            .snapshot
            .same_snapshot(&frame)
    );
}

#[test]
fn oversized_offsets_clamp_and_resizing_to_fit_content_removes_scroll_without_poisoning_old_snapshots()
 {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| {
        f.style(id, Point::new(f64::MAX, f64::MAX), false, zero())
    })
    .unwrap();
    let small = tree.compute(Size::new(100.0, 80.0)).unwrap().snapshot;
    assert_eq!(
        small.scroll(&f.root).unwrap().offset,
        Point::new(130.0, 110.0)
    );
    let large = tree.compute(Size::new(240.0, 200.0)).unwrap().snapshot;
    assert_eq!(large.scroll(&f.root).unwrap(), ScrollMetrics::default());
    assert_eq!(
        small.scroll(&f.root).unwrap().offset,
        Point::new(130.0, 110.0)
    );
    let empty = tree.compute(Size::new(0.0, 0.0)).unwrap().snapshot;
    assert!(
        empty
            .boxes()
            .iter()
            .all(|node| node.visible_bounds().is_none()
                && node.bounds.origin.x.is_finite()
                && node.bounds.origin.y.is_finite())
    );
}

#[test]
fn invalid_scroll_inputs_are_rejected_before_mutating_clips_geometry_or_snapshot() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| f.style(id, zero(), false, zero()))
        .unwrap();
    let viewport = Size::new(100.0, 80.0);
    let old = tree.compute(viewport).unwrap().snapshot;
    for invalid in [
        Point::new(-1.0, 0.0),
        Point::new(f64::NAN, 0.0),
        Point::new(0.0, f64::INFINITY),
    ] {
        let result = tree.sync(&f.scene, |id| {
            f.style(id, Point::new(30.0, 30.0), false, invalid)
        });
        assert!(matches!(result, Err(LayoutError::InvalidStyle { .. })));
        assert!(tree.compute(viewport).unwrap().snapshot.same_snapshot(&old));
    }
}

#[test]
fn nonclipping_offsets_are_inert_and_partial_removal_reclamps_a_retained_scroller() {
    let mut f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| {
        f.style(id, Point::new(40.0, 40.0), false, Point::new(80.0, 80.0))
    })
    .unwrap();
    let viewport = Size::new(100.0, 80.0);
    let old = tree.compute(viewport).unwrap().snapshot;
    assert_eq!(old.scroll(&f.inner).unwrap(), ScrollMetrics::default());
    f.scene
        .update_children(&f.root, |ui| {
            ui.button("footer", "Footer");
        })
        .unwrap();
    tree.sync(&f.scene, |id| {
        f.style(id, Point::new(40.0, 40.0), false, zero())
    })
    .unwrap();
    let frame = tree.compute(viewport).unwrap().snapshot;
    assert_eq!(frame.scroll(&f.root).unwrap(), ScrollMetrics::default());
    assert!(frame.scroll(&f.leaf).is_none());
    assert_eq!(old.scroll(&f.root).unwrap().offset, Point::new(40.0, 40.0));
}

#[cfg(feature = "text")]
#[test]
fn scrolling_retained_shaped_text_does_not_execute_layout_or_measurement() {
    use zest_text::raster::{Rasterizer, ShapeOptions};
    let f = fixture();
    let mut raster = Rasterizer::new();
    raster
        .register_font(include_bytes!("../../../../assets/fonts/NotoSans.ttf").to_vec())
        .unwrap();
    let text = TextContent::new(
        raster
            .shape(
                "Retained paragraph with several wrapped lines of text",
                &["Noto Sans"],
                ShapeOptions::default(),
            )
            .unwrap(),
    );
    let mut tree = LayoutTree::default();
    for offset in [0.0, 15.5, 80.25] {
        tree.sync_with_text(&f.scene, |id| {
            let style = if id == &f.leaf {
                Style {
                    width: Length::Px(100.0),
                    shrink: 0.0,
                    ..Default::default()
                }
            } else {
                f.style(id, Point::new(0.0, offset), false, zero())
            };
            (style, (id == &f.leaf).then(|| text.clone()))
        })
        .unwrap();
        let computed = tree.compute(Size::new(100.0, 80.0)).unwrap();
        if offset > 0.0 {
            assert_eq!(computed.stats, ComputeStats::default());
        }
    }
}
