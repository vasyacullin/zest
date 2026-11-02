use super::*;
use zest_core::geometry::Point;

struct Fixture {
    scene: Scene,
    outer: WidgetId,
    inner: WidgetId,
    wrapper: WidgetId,
    leaf: WidgetId,
    overlay: WidgetId,
}
fn fixture() -> Fixture {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("outer", |ui| {
                ui.container("inner", |ui| {
                    ui.container("wrapper", |ui| {
                        ui.button("leaf", "Leaf");
                    });
                });
            });
            ui.button("overlay", "Overlay");
        })
        .unwrap();
    let outer = scene.roots()[0].clone();
    let overlay = scene.roots()[1].clone();
    let inner = scene.children(&outer).unwrap()[0].clone();
    let wrapper = scene.children(&inner).unwrap()[0].clone();
    let leaf = scene.children(&wrapper).unwrap()[0].clone();
    Fixture {
        scene,
        outer,
        inner,
        wrapper,
        leaf,
        overlay,
    }
}
impl Fixture {
    fn style(&self, id: &WidgetId, outer_clip: bool, inner_clip: bool) -> Style {
        let (width, height, padding, clip_children) = if id == &self.outer {
            (80.5, 60.5, 10.25, outer_clip)
        } else if id == &self.inner {
            (100.0, 80.0, 5.25, inner_clip)
        } else if id == &self.wrapper {
            (150.0, 100.0, 0.0, false)
        } else if id == &self.leaf {
            (150.0, 150.0, 0.0, false)
        } else {
            (80.5, 20.0, 0.0, false)
        };
        Style {
            width: Length::Px(width),
            height: Length::Px(height),
            padding: Edges::all(padding),
            shrink: 0.0,
            clip_children,
            ..Default::default()
        }
    }
}

#[test]
fn nested_clips_intersect_and_propagate_through_nonclipping_containers() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| f.style(id, true, true)).unwrap();
    let frame = tree.compute(Size::new(200.0, 200.0)).unwrap().snapshot;
    let leaf = frame.boxes().iter().find(|node| node.id == f.leaf).unwrap();
    assert_eq!(leaf.clip, Rect::from_xywh(10.25, 10.25, 70.25, 50.25));
    assert_eq!(
        leaf.visible_bounds(),
        Some(Rect::from_xywh(15.5, 15.5, 65.0, 45.0))
    );
    assert_eq!(frame.visible_bounds(&f.leaf), leaf.visible_bounds());
    assert_eq!(frame.bounds(&f.leaf).unwrap().size, Size::new(150.0, 150.0));
    // The inner clip affects its children, not the inner node's own clip.
    let inner = frame
        .boxes()
        .iter()
        .find(|node| node.id == f.inner)
        .unwrap();
    assert_eq!(inner.clip, frame.bounds(&f.outer).unwrap());
    assert_eq!(
        frame.hit_test(Point::new(80.499, 20.0), |id| id == &f.leaf),
        Some(&f.leaf)
    );
    assert!(
        frame
            .hit_test(Point::new(80.5, 20.0), |id| id == &f.leaf)
            .is_none()
    );
    assert!(
        frame
            .hit_test(Point::new(20.0, 60.5), |id| id == &f.leaf)
            .is_none()
    );
}

#[test]
fn clip_only_updates_reuse_geometry_and_old_snapshots_and_failed_sync_is_atomic() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| f.style(id, false, false)).unwrap();
    let viewport = Size::new(200.0, 200.0);
    let old = tree.compute(viewport).unwrap().snapshot;
    let point = Point::new(20.0, 65.0);
    assert_eq!(old.hit_test(point, |_| true), Some(&f.overlay));
    assert_eq!(old.hit_test(point, |id| id == &f.leaf), Some(&f.leaf));
    let sync = tree.sync(&f.scene, |id| f.style(id, true, true)).unwrap();
    assert_eq!(sync.restyled, 2);
    assert!(tree.snapshot().is_none());
    let new = tree.compute(viewport).unwrap();
    assert_eq!(new.stats, ComputeStats::default());
    assert!(!old.same_snapshot(&new.snapshot));
    for node in old.boxes() {
        assert_eq!(new.snapshot.bounds(&node.id), Some(node.bounds));
    }
    assert!(new.snapshot.hit_test(point, |id| id == &f.leaf).is_none());
    assert_eq!(old.hit_test(point, |id| id == &f.leaf), Some(&f.leaf));
    assert!(
        tree.sync(&f.scene, |id| {
            let mut style = f.style(id, false, false);
            if id == &f.leaf {
                style.gap = f32::NAN;
            }
            style
        })
        .is_err()
    );
    assert!(
        tree.compute(viewport)
            .unwrap()
            .snapshot
            .same_snapshot(&new.snapshot)
    );
}

#[test]
fn empty_parent_clips_hide_descendants_but_nonclipping_empty_parents_allow_overflow() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    for clip in [false, true] {
        tree.sync(&f.scene, |id| {
            if id == &f.outer {
                Style {
                    width: Length::Px(0.0),
                    height: Length::Px(0.0),
                    clip_children: clip,
                    shrink: 0.0,
                    ..Default::default()
                }
            } else {
                f.style(id, clip, false)
            }
        })
        .unwrap();
        let frame = tree.compute(Size::new(200.0, 200.0)).unwrap().snapshot;
        assert!(frame.visible_bounds(&f.outer).is_none());
        assert_eq!(frame.visible_bounds(&f.leaf).is_none(), clip);
        assert_eq!(
            frame
                .hit_test(Point::new(10.0, 10.0), |id| id == &f.leaf)
                .is_none(),
            clip
        );
    }
    let empty = tree.compute(Size::new(0.0, 0.0)).unwrap().snapshot;
    assert!(
        empty
            .boxes()
            .iter()
            .all(|node| node.clip.is_empty() && node.visible_bounds().is_none())
    );
}

#[test]
fn viewport_and_parent_clips_share_fractional_edges_after_resize() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| f.style(id, true, true)).unwrap();
    let wide = tree.compute(Size::new(200.0, 200.0)).unwrap().snapshot;
    let narrow = tree.compute(Size::new(40.25, 50.25)).unwrap().snapshot;
    assert_eq!(
        narrow.visible_bounds(&f.leaf),
        Some(Rect::from_xywh(15.5, 15.5, 24.75, 34.75))
    );
    assert!(narrow.hit_test(Point::new(40.25, 20.0), |_| true).is_none());
    assert_eq!(
        wide.visible_bounds(&f.leaf),
        Some(Rect::from_xywh(15.5, 15.5, 65.0, 45.0))
    );
}

#[test]
fn removing_a_clipped_subtree_does_not_leak_its_clip_to_new_scene_roots() {
    let mut f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| f.style(id, true, true)).unwrap();
    let viewport = Size::new(200.0, 200.0);
    let old = tree.compute(viewport).unwrap().snapshot;
    let mut standalone = None;
    f.scene
        .update(|ui| {
            ui.container("outer", |_| {});
            standalone = Some(ui.button("standalone", "Standalone"));
        })
        .unwrap();
    let standalone = standalone.unwrap();
    tree.sync(&f.scene, |id| {
        if id == &standalone {
            Style {
                width: Length::Px(100.0),
                height: Length::Px(100.0),
                shrink: 0.0,
                ..Default::default()
            }
        } else {
            f.style(id, true, true)
        }
    })
    .unwrap();
    let frame = tree.compute(viewport).unwrap().snapshot;
    assert!(frame.bounds(&f.leaf).is_none() && frame.visible_bounds(&f.leaf).is_none());
    assert_eq!(
        frame.hit_test(Point::new(70.0, 80.0), |_| true),
        Some(&standalone)
    );
    let node = frame
        .boxes()
        .iter()
        .find(|node| node.id == standalone)
        .unwrap();
    assert_eq!(node.clip, Rect::from_xywh(0.0, 0.0, 200.0, 200.0));
    assert!(old.visible_bounds(&f.leaf).is_some());
}

#[cfg(feature = "text")]
#[test]
fn clip_changes_do_not_remeasure_retained_shaped_text() {
    use zest_text::raster::{Rasterizer, ShapeOptions};
    let f = fixture();
    let mut raster = Rasterizer::new();
    raster
        .register_font(include_bytes!("../../../assets/fonts/NotoSans.ttf").to_vec())
        .unwrap();
    let content = TextContent::new(
        raster
            .shape(
                "Clip this retained paragraph across narrow lines",
                &["Noto Sans"],
                ShapeOptions::default(),
            )
            .unwrap(),
    );
    let mut tree = LayoutTree::default();
    let viewport = Size::new(200.0, 200.0);
    for clip in [false, true, false] {
        tree.sync_with_text(&f.scene, |id| {
            let style = f.style(id, clip, clip);
            (
                if id == &f.leaf {
                    Style {
                        width: Length::Px(80.0),
                        ..Default::default()
                    }
                } else {
                    style
                },
                (id == &f.leaf).then(|| content.clone()),
            )
        })
        .unwrap();
        let computed = tree.compute(viewport).unwrap();
        if clip {
            assert_eq!(computed.stats, ComputeStats::default());
        }
    }
}
