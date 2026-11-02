use super::*;

struct Fixture {
    scene: Scene,
    root: WidgetId,
    inner: WidgetId,
    target: WidgetId,
}
fn fixture() -> Fixture {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("scroll", |ui| {
                ui.button("outer-prefix", "Prefix");
                ui.container("inner", |ui| {
                    ui.button("inner-prefix", "Prefix");
                    ui.button("target", "Target");
                });
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let inner = scene.children(&root).unwrap()[1].clone();
    let target = scene.children(&inner).unwrap()[1].clone();
    Fixture {
        scene,
        root,
        inner,
        target,
    }
}
impl Fixture {
    fn style(&self, id: &WidgetId, adjustments: &[ScrollAdjustment]) -> Style {
        let (width, height, clip) = if id == &self.root {
            (100.0, 80.0, true)
        } else if id == &self.inner {
            (80.0, 60.0, true)
        } else if id == &self.target {
            (30.0, 20.0, false)
        } else if self.scene.children(&self.root).unwrap()[0] == *id {
            (100.0, 60.0, false)
        } else {
            (200.0, 100.0, false)
        };
        Style {
            width: Length::Px(width),
            height: Length::Px(height),
            shrink: 0.0,
            clip_children: clip,
            scroll_offset: adjustments
                .iter()
                .find(|change| change.widget == *id)
                .map_or(Point::new(0.0, 0.0), |change| change.offset),
            ..Default::default()
        }
    }
}

#[test]
fn nested_reveal_plans_apply_inside_out_and_predict_the_final_visible_bounds() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let viewport = Size::new(100.0, 80.0);
    tree.sync(&f.scene, |id| f.style(id, &[])).unwrap();
    let old = tree.compute(viewport).unwrap().snapshot;
    assert!(old.visible_bounds(&f.target).is_none());
    let plan = old.reveal_plan(&f.target).unwrap();
    assert_eq!(
        plan.adjustments,
        vec![
            ScrollAdjustment {
                widget: f.inner.clone(),
                offset: Point::new(0.0, 60.0)
            },
            ScrollAdjustment {
                widget: f.root.clone(),
                offset: Point::new(0.0, 40.0)
            },
        ]
    );
    assert_eq!(plan.visible_bounds, Rect::from_xywh(0.0, 60.0, 30.0, 20.0));
    assert!(old.visible_bounds(&f.target).is_none()); // Read-only planning.
    tree.sync(&f.scene, |id| f.style(id, &plan.adjustments))
        .unwrap();
    let new = tree.compute(viewport).unwrap();
    assert_eq!(new.stats, ComputeStats::default());
    assert_eq!(
        new.snapshot.visible_bounds(&f.target),
        Some(plan.visible_bounds)
    );
    assert_eq!(
        new.snapshot
            .hit_test(Point::new(5.0, 65.0), |id| id == &f.target),
        Some(&f.target)
    );
    assert!(
        new.snapshot
            .reveal_plan(&f.target)
            .unwrap()
            .adjustments
            .is_empty()
    );
}

#[test]
fn both_axes_use_nearest_edges_and_preserve_fractional_positions() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("root", |ui| {
                ui.container("content", |ui| {
                    ui.button("target", "Target");
                });
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let content = scene.children(&root).unwrap()[0].clone();
    let target = scene.children(&content).unwrap()[0].clone();
    let mut tree = LayoutTree::default();
    let styles = |offset| {
        let (root, content) = (&root, &content);
        move |id: &WidgetId| {
            if id == root {
                Style {
                    width: Length::Px(100.25),
                    height: Length::Px(80.25),
                    clip_children: true,
                    scroll_offset: offset,
                    ..Default::default()
                }
            } else if id == content {
                Style {
                    width: Length::Px(200.0),
                    height: Length::Px(160.0),
                    shrink: 0.0,
                    padding: Edges {
                        left: 150.5,
                        top: 110.5,
                        ..Default::default()
                    },
                    ..Default::default()
                }
            } else {
                Style {
                    width: Length::Px(30.0),
                    height: Length::Px(20.0),
                    shrink: 0.0,
                    ..Default::default()
                }
            }
        }
    };
    let viewport = Size::new(300.0, 300.0);
    tree.sync(&scene, styles(Point::new(0.0, 0.0))).unwrap();
    let plan = tree
        .compute(viewport)
        .unwrap()
        .snapshot
        .reveal_plan(&target)
        .unwrap();
    assert_eq!(plan.adjustments[0].offset, Point::new(80.25, 50.25));
    assert_eq!(
        plan.visible_bounds,
        Rect::from_xywh(70.25, 60.25, 30.0, 20.0)
    );
    tree.sync(&scene, styles(plan.adjustments[0].offset))
        .unwrap();
    assert!(
        tree.compute(viewport)
            .unwrap()
            .snapshot
            .reveal_plan(&target)
            .unwrap()
            .adjustments
            .is_empty()
    );
}

#[test]
fn oversized_targets_fill_the_port_without_oscillating_when_they_already_span_it() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("root", |ui| {
                ui.button("prefix", "Prefix");
                ui.button("target", "Target");
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let target = scene.children(&root).unwrap()[1].clone();
    let mut tree = LayoutTree::default();
    for offset in [0.0, 30.0, 100.0, 150.0] {
        tree.sync(&scene, |id| {
            if id == &root {
                Style {
                    width: Length::Px(100.0),
                    height: Length::Px(80.0),
                    clip_children: true,
                    scroll_offset: Point::new(0.0, offset),
                    ..Default::default()
                }
            } else {
                Style {
                    width: Length::Px(100.0),
                    height: Length::Px(if id == &target { 200.0 } else { 30.0 }),
                    shrink: 0.0,
                    ..Default::default()
                }
            }
        })
        .unwrap();
        let frame = tree.compute(Size::new(100.0, 80.0)).unwrap().snapshot;
        let plan = frame.reveal_plan(&target).unwrap();
        assert_eq!(plan.visible_bounds, Rect::from_xywh(0.0, 0.0, 100.0, 80.0));
        if offset == 0.0 {
            assert_eq!(plan.adjustments[0].offset.y, 30.0);
        } else {
            assert!(plan.adjustments.is_empty());
        }
    }
}

#[test]
fn outer_scrollers_use_only_the_part_visible_through_an_inner_clip() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| {
        let mut style = f.style(id, &[]);
        if id == &f.target {
            style.height = Length::Px(200.0);
        }
        style
    })
    .unwrap();
    let frame = tree.compute(Size::new(100.0, 80.0)).unwrap().snapshot;
    let plan = frame.reveal_plan(&f.target).unwrap();
    assert_eq!(plan.adjustments[0].widget, f.inner);
    assert_eq!(plan.adjustments[0].offset.y, 100.0);
    assert_eq!(plan.adjustments[1].offset.y, 40.0);
    assert_eq!(plan.visible_bounds, Rect::from_xywh(0.0, 20.0, 30.0, 60.0));
}

#[test]
fn empty_unknown_and_unreachable_targets_return_no_plan() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let mut other = Scene::default();
    other
        .update(|ui| {
            ui.button("unknown", "Unknown");
        })
        .unwrap();
    for empty in [false, true] {
        tree.sync(&f.scene, |id| {
            let mut style = f.style(id, &[]);
            if empty && id == &f.inner {
                style.height = Length::Px(0.0);
            }
            style
        })
        .unwrap();
        let frame = tree.compute(Size::new(100.0, 80.0)).unwrap().snapshot;
        assert!(frame.reveal_plan(&other.roots()[0]).is_none());
        if empty {
            assert!(frame.reveal_plan(&f.target).is_none());
        }
    }
    assert!(
        tree.compute(Size::new(0.0, 0.0))
            .unwrap()
            .snapshot
            .reveal_plan(&f.target)
            .is_none()
    );
}

#[test]
fn fixed_viewport_clipping_can_prevent_reveal_even_when_an_inner_scroll_range_exists() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    // The inner scrollport starts at y=60 and cannot itself be moved because
    // the outer node no longer scrolls. Nothing can enter a 50px viewport.
    tree.sync(&f.scene, |id| {
        let mut style = f.style(id, &[]);
        if id == &f.root {
            style.clip_children = false;
        }
        style
    })
    .unwrap();
    let frame = tree.compute(Size::new(100.0, 50.0)).unwrap().snapshot;
    assert!(frame.reveal_plan(&f.target).is_none());
}

#[test]
fn a_partially_offscreen_scrollport_can_reveal_content_already_inside_its_full_border() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("fixed", |ui| {
                ui.container("scroll", |ui| {
                    ui.button("prefix", "prefix");
                    ui.button("target", "target");
                    ui.button("tail", "tail");
                });
            });
        })
        .unwrap();
    let fixed = scene.roots()[0].clone();
    let scroll = scene.children(&fixed).unwrap()[0].clone();
    let target = scene.children(&scroll).unwrap()[1].clone();
    let style = |offset| {
        let (fixed, scroll, target) = (&fixed, &scroll, &target);
        let scene = &scene;
        move |id: &WidgetId| {
            let mut style = Style {
                width: Length::Px(100.0),
                shrink: 0.0,
                ..Default::default()
            };
            if id == fixed {
                style.height = Length::Px(50.0);
                style.justify = Justify::End;
            } else if id == scroll {
                style.height = Length::Px(100.0);
                style.clip_children = true;
                style.scroll_offset.y = offset;
            } else {
                style.height = Length::Px(if id == target {
                    20.0
                } else if scene.label(id) == Some("prefix") {
                    80.0
                } else {
                    100.0
                });
            }
            style
        }
    };
    let mut tree = LayoutTree::default();
    let viewport = Size::new(100.0, 100.0);
    tree.sync(&scene, style(60.0)).unwrap();
    let old = tree.compute(viewport).unwrap().snapshot;
    assert_eq!(old.bounds(&scroll).unwrap().origin.y, -50.0);
    assert!(old.visible_bounds(&target).is_none());
    let plan = old.reveal_plan(&target).unwrap();
    assert_eq!(plan.adjustments[0].offset.y, 30.0);
    tree.sync(&scene, style(plan.adjustments[0].offset.y))
        .unwrap();
    let new = tree.compute(viewport).unwrap().snapshot;
    assert_eq!(
        new.visible_bounds(&target),
        Some(Rect::from_xywh(0.0, 0.0, 100.0, 20.0))
    );
    assert_eq!(new.visible_bounds(&target), Some(plan.visible_bounds));
}

#[test]
fn bounded_outer_scroll_exposes_the_maximum_reachable_part_of_a_nested_target() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("root", |ui| {
                ui.button("outer-prefix", "outer-prefix");
                ui.container("inner", |ui| {
                    ui.button("inner-prefix", "inner-prefix");
                    ui.button("target", "target");
                    ui.button("inner-tail", "inner-tail");
                });
                ui.button("outer-tail", "outer-tail");
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let inner = scene.children(&root).unwrap()[1].clone();
    let target = scene.children(&inner).unwrap()[1].clone();
    let style = |adjustments: Vec<ScrollAdjustment>| {
        let (root, inner) = (&root, &inner);
        let scene = &scene;
        move |id: &WidgetId| {
            let height = if id == root {
                100.0
            } else if id == inner {
                60.0
            } else {
                match scene.label(id).unwrap() {
                    "outer-prefix" => 40.0,
                    "inner-prefix" => 80.0,
                    "target" => 20.0,
                    "inner-tail" => 60.0,
                    _ => 4.0,
                }
            };
            Style {
                width: Length::Px(100.0),
                height: Length::Px(height),
                shrink: 0.0,
                clip_children: id == root || id == inner,
                scroll_offset: adjustments
                    .iter()
                    .find(|a| a.widget == *id)
                    .map_or(Point::new(0.0, 0.0), |a| a.offset),
                ..Default::default()
            }
        }
    };
    let mut tree = LayoutTree::default();
    let viewport = Size::new(100.0, 50.0);
    tree.sync(&scene, style(vec![])).unwrap();
    let old = tree.compute(viewport).unwrap().snapshot;
    let plan = old.reveal_plan(&target).unwrap();
    assert_eq!(
        plan.adjustments
            .iter()
            .map(|a| a.offset.y)
            .collect::<Vec<_>>(),
        vec![80.0, 4.0]
    );
    assert_eq!(plan.visible_bounds, Rect::from_xywh(0.0, 36.0, 100.0, 14.0));
    tree.sync(&scene, style(plan.adjustments.clone())).unwrap();
    let new = tree.compute(viewport).unwrap().snapshot;
    assert_eq!(new.visible_bounds(&target), Some(plan.visible_bounds));
    assert!(new.reveal_plan(&target).unwrap().adjustments.is_empty());
}
