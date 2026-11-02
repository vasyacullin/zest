use super::*;

#[test]
fn shrink_minimums_justify_and_padding_follow_flex_constraints() {
    let (scene, root, a, b) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |id| {
        if id == &root {
            Style {
                axis: Axis::Row,
                height: Length::Px(100.0),
                padding: Edges::all(10.0),
                ..Default::default()
            }
        } else {
            Style {
                min_size: Size::new(60.0, 0.0),
                shrink: 1.0,
                ..fixed(100.0, 20.0)
            }
        }
    })
    .unwrap();
    let result = tree.compute(Size::new(180.0, 200.0)).unwrap();
    near(result.snapshot.bounds(&a).unwrap().size.width, 80.0);
    near(result.snapshot.bounds(&b).unwrap().size.width, 80.0);
    for (justify, x) in [
        (Justify::Center, 70.0),
        (Justify::End, 130.0),
        (Justify::SpaceBetween, 10.0),
    ] {
        tree.sync(&scene, |id| {
            if id == &root {
                Style {
                    axis: Axis::Row,
                    height: Length::Px(100.0),
                    padding: Edges::all(10.0),
                    justify,
                    ..Default::default()
                }
            } else {
                fixed(20.0, 20.0)
            }
        })
        .unwrap();
        let result = tree.compute(Size::new(180.0, 200.0)).unwrap();
        near(result.snapshot.bounds(&a).unwrap().origin.x, x);
        if justify == Justify::SpaceBetween {
            near(result.snapshot.bounds(&b).unwrap().origin.x, 150.0);
        }
    }
}

#[test]
fn parent_offsets_accumulate_and_independent_fixed_branches_keep_measurement_caches() {
    let mut scene = Scene::default();
    let (mut outer, mut inner, mut a, mut b, mut leaf) = (None, None, None, None, None);
    scene
        .update(|ui| {
            ui.button("prefix", "prefix");
            outer = Some(ui.container("outer", |ui| {
                inner = Some(ui.container("inner", |ui| {
                    leaf = Some(ui.button("leaf", "leaf"));
                }));
            }));
            a = Some(ui.button("a", "A"));
            b = Some(ui.button("b", "B"));
        })
        .unwrap();
    let (outer, inner, a, b, leaf) = (
        outer.unwrap(),
        inner.unwrap(),
        a.unwrap(),
        b.unwrap(),
        leaf.unwrap(),
    );
    let mut tree = LayoutTree::default();
    let style = |width| {
        let (outer, inner, a) = (&outer, &inner, &a);
        move |id: &WidgetId| {
            if id == outer {
                Style {
                    padding: Edges::all(2.0),
                    ..Default::default()
                }
            } else if id == inner {
                Style {
                    padding: Edges::all(3.0),
                    ..Default::default()
                }
            } else {
                Style {
                    intrinsic_size: Size::new(if id == a { width } else { 20.0 }, 20.0),
                    shrink: 0.0,
                    ..Default::default()
                }
            }
        }
    };
    tree.sync(&scene, style(30.0)).unwrap();
    let before = tree.compute(Size::new(400.0, 400.0)).unwrap();
    assert_eq!(before.snapshot.bounds(&leaf).unwrap().origin.x, 5.0);
    assert_eq!(before.snapshot.bounds(&leaf).unwrap().origin.y, 25.0);
    let identity = tree.entries[&b].node;
    tree.sync(&scene, style(50.0)).unwrap();
    assert!(!tree.tree.dirty(identity).unwrap());
    let after = tree.compute(Size::new(400.0, 400.0)).unwrap();
    assert!(after.stats.measured_leaves < before.stats.measured_leaves);
    assert_eq!(after.snapshot.bounds(&b), before.snapshot.bounds(&b));
    assert_eq!(tree.entries[&b].node, identity);
}

#[test]
fn deleting_and_reintroducing_a_key_creates_a_new_private_node_without_leaks() {
    let (mut scene, root, a, b) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |_| fixed(20.0, 20.0)).unwrap();
    let original = tree.entries[&b].node;
    for _ in 0..20 {
        scene
            .update_children(&root, |ui| {
                ui.button("a", "A");
            })
            .unwrap();
        tree.sync(&scene, |_| fixed(20.0, 20.0)).unwrap();
        scene
            .update_children(&root, |ui| {
                ui.button("a", "A");
                assert_eq!(ui.button("b", "B"), b);
            })
            .unwrap();
        tree.sync(&scene, |_| fixed(20.0, 20.0)).unwrap();
        assert_ne!(tree.entries[&b].node, original);
        assert!(tree.entries.contains_key(&a));
        assert_eq!(tree.tree.total_node_count(), 4);
        tree.compute(Size::new(100.0, 100.0)).unwrap();
    }
}

#[test]
fn overflowing_computed_geometry_is_not_published_and_valid_styles_recover() {
    let (scene, root, a, _) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |_| fixed(20.0, 20.0)).unwrap();
    let old = tree.compute(Size::new(400.0, 200.0)).unwrap().snapshot;
    tree.sync(&scene, |id| {
        if id == &root {
            Style::default()
        } else {
            Style {
                width: Length::Percent(f32::MAX),
                shrink: 0.0,
                ..fixed(0.0, 20.0)
            }
        }
    })
    .unwrap();
    assert!(matches!(
        tree.compute(Size::new(400.0, 200.0)),
        Err(LayoutError::InvalidGeometry(_))
    ));
    assert!(tree.snapshot().is_none());
    assert_eq!(old.bounds(&a).unwrap().size.width, 20.0);
    tree.sync(&scene, |_| fixed(20.0, 20.0)).unwrap();
    let recovered = tree.compute(Size::new(400.0, 200.0)).unwrap();
    assert_eq!(recovered.snapshot.bounds(&a), old.bounds(&a));
}

fn scene() -> (Scene, WidgetId, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    let (mut root, mut a, mut b) = (None, None, None);
    scene
        .update(|ui| {
            root = Some(ui.container("root", |ui| {
                a = Some(ui.button("a", "A"));
                b = Some(ui.button("b", "B"));
            }));
        })
        .unwrap();
    (scene, root.unwrap(), a.unwrap(), b.unwrap())
}
fn fixed(width: f32, height: f32) -> Style {
    Style {
        width: Length::Px(width),
        height: Length::Px(height),
        shrink: 0.0,
        ..Default::default()
    }
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
}

#[test]
fn nested_flex_bounds_are_absolute_and_keep_fractional_logical_coordinates() {
    let (scene, root, a, b) = scene();
    let mut tree = LayoutTree::default();
    let stats = tree
        .sync(&scene, |id| {
            if id == &root {
                Style {
                    axis: Axis::Row,
                    gap: 8.5,
                    padding: Edges::all(10.25),
                    ..Default::default()
                }
            } else {
                fixed(80.5, 30.25)
            }
        })
        .unwrap();
    assert_eq!(stats.created, 3);
    let result = tree.compute(Size::new(400.0, 200.0)).unwrap();
    assert!(result.stats.performed);
    assert_eq!(
        result.snapshot.bounds(&root),
        Some(Rect::from_xywh(0.0, 0.0, 400.0, 50.75))
    );
    assert_eq!(
        result.snapshot.bounds(&a),
        Some(Rect::from_xywh(10.25, 10.25, 80.5, 30.25))
    );
    assert_eq!(
        result.snapshot.bounds(&b),
        Some(Rect::from_xywh(99.25, 10.25, 80.5, 30.25))
    );
    assert_eq!(
        result
            .snapshot
            .boxes()
            .iter()
            .map(|b| &b.id)
            .collect::<Vec<_>>(),
        [&root, &a, &b]
    );
}

#[test]
fn unchanged_and_paint_only_inputs_reuse_nodes_caches_and_the_snapshot() {
    let (mut scene, root, a, _) = scene();
    let mut tree = LayoutTree::default();
    let style = |id: &WidgetId| {
        if id == &root {
            Style::default()
        } else {
            Style {
                intrinsic_size: Size::new(40.25, 20.5),
                ..Default::default()
            }
        }
    };
    tree.sync(&scene, style).unwrap();
    let first = tree.compute(Size::new(400.0, 200.0)).unwrap();
    assert!(first.stats.measured_leaves > 0);
    let node = tree.entries[&a].node;
    scene.focus(&a);
    scene.set_hovered(&a, true);
    assert_eq!(tree.sync(&scene, style).unwrap(), SyncStats::default());
    let next = tree.compute(Size::new(400.0, 200.0)).unwrap();
    assert_eq!(next.stats, ComputeStats::default());
    assert!(next.snapshot.same_snapshot(&first.snapshot));
    assert_eq!(tree.entries[&a].node, node);
    scene
        .update_children(&root, |ui| {
            ui.button("a", "Renamed, same supplied metrics");
            ui.button("b", "B");
        })
        .unwrap();
    assert_eq!(tree.sync(&scene, style).unwrap(), SyncStats::default());
    assert!(
        tree.compute(Size::new(400.0, 200.0))
            .unwrap()
            .snapshot
            .same_snapshot(&first.snapshot)
    );
}

#[test]
fn reorder_and_partial_scene_removal_preserve_retained_layout_nodes() {
    let (mut scene, root, a, b) = scene();
    let mut tree = LayoutTree::default();
    let style = |id: &WidgetId| {
        if id == &root {
            Style {
                axis: Axis::Row,
                ..Default::default()
            }
        } else {
            fixed(40.0, 20.0)
        }
    };
    tree.sync(&scene, style).unwrap();
    let old = tree.compute(Size::new(400.0, 200.0)).unwrap().snapshot;
    let identities = (tree.entries[&a].node, tree.entries[&b].node);
    scene
        .update_children(&root, |ui| {
            assert_eq!(ui.button("b", "B"), b);
            assert_eq!(ui.button("a", "A"), a);
        })
        .unwrap();
    let stats = tree.sync(&scene, style).unwrap();
    assert_eq!((stats.created, stats.removed, stats.restyled), (0, 0, 0));
    assert_eq!(stats.children_changed, 1);
    assert!(tree.snapshot().is_none());
    let reordered = tree.compute(Size::new(400.0, 200.0)).unwrap().snapshot;
    assert_eq!(reordered.bounds(&a).unwrap().origin.x, 40.0);
    assert_eq!(old.bounds(&a).unwrap().origin.x, 0.0);
    assert_eq!((tree.entries[&a].node, tree.entries[&b].node), identities);
    scene
        .update_children(&root, |ui| {
            ui.button("a", "A");
        })
        .unwrap();
    let stats = tree.sync(&scene, style).unwrap();
    assert_eq!(stats.removed, 1);
    let result = tree.compute(Size::new(400.0, 200.0)).unwrap();
    assert_eq!(tree.len(), 2);
    assert_eq!(tree.tree.total_node_count(), 3); // includes viewport root
    assert!(result.snapshot.bounds(&b).is_none());
    assert!(old.bounds(&b).is_some());
    assert_eq!(tree.entries[&a].node, identities.0);
    scene.update(|_| {}).unwrap();
    assert_eq!(tree.sync(&scene, style).unwrap().removed, 2);
    assert!(tree.is_empty());
    assert!(
        tree.compute(Size::new(0.0, 0.0))
            .unwrap()
            .snapshot
            .boxes()
            .is_empty()
    );
}

#[test]
fn intrinsic_and_style_changes_invalidate_ancestors_and_reposition_siblings() {
    let (scene, root, a, b) = scene();
    let mut tree = LayoutTree::default();
    let style = |width| {
        let root = root.clone();
        let a = a.clone();
        move |id: &WidgetId| {
            if id == &root {
                Style {
                    axis: Axis::Row,
                    ..Default::default()
                }
            } else {
                Style {
                    intrinsic_size: Size::new(if id == &a { width } else { 20.0 }, 20.0),
                    shrink: 0.0,
                    ..Default::default()
                }
            }
        }
    };
    tree.sync(&scene, style(30.0)).unwrap();
    let before = tree.compute(Size::new(400.0, 200.0)).unwrap().snapshot;
    assert_eq!(before.bounds(&b).unwrap().origin.x, 30.0);
    let stats = tree.sync(&scene, style(70.0)).unwrap();
    assert_eq!(stats.restyled, 1);
    let after = tree.compute(Size::new(400.0, 200.0)).unwrap();
    assert!(after.stats.performed && after.stats.measured_leaves > 0);
    assert_eq!(after.snapshot.bounds(&b).unwrap().origin.x, 70.0);
    assert_eq!(before.bounds(&b).unwrap().origin.x, 30.0);
}

#[test]
fn percentages_flex_growth_alignment_and_resize_use_the_current_viewport() {
    let (scene, root, a, b) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |id| {
        if id == &root {
            Style {
                axis: Axis::Row,
                height: Length::Px(100.0),
                align: Align::Center,
                ..Default::default()
            }
        } else if id == &a {
            Style {
                width: Length::Percent(0.25),
                ..fixed(0.0, 20.0)
            }
        } else {
            Style {
                grow: 1.0,
                ..fixed(0.0, 30.0)
            }
        }
    })
    .unwrap();
    for width in [400.0, 600.0] {
        let result = tree.compute(Size::new(width, 200.0)).unwrap();
        near(result.snapshot.bounds(&a).unwrap().size.width, width * 0.25);
        near(result.snapshot.bounds(&b).unwrap().size.width, width * 0.75);
        near(result.snapshot.bounds(&a).unwrap().origin.y, 40.0);
        near(result.snapshot.bounds(&b).unwrap().origin.y, 35.0);
    }
}

#[test]
fn invalid_style_or_panicking_callback_does_not_mutate_the_tree_or_snapshot() {
    let (scene, root, a, _) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |_| fixed(20.0, 20.0)).unwrap();
    let first = tree.compute(Size::new(400.0, 200.0)).unwrap().snapshot;
    let node = tree.entries[&a].node;
    let error = tree.sync(&scene, |id| {
        if id == &a {
            Style {
                gap: f32::NAN,
                ..Default::default()
            }
        } else {
            fixed(99.0, 99.0)
        }
    });
    assert!(matches!(
        error,
        Err(LayoutError::InvalidStyle { field: "gap", .. })
    ));
    assert_eq!(tree.entries[&root].style, fixed(20.0, 20.0));
    assert_eq!(tree.entries[&a].node, node);
    assert!(tree.snapshot().unwrap().same_snapshot(&first));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        tree.sync(&scene, |id| {
            assert_ne!(id, &a, "injected style callback panic");
            fixed(88.0, 88.0)
        })
    }));
    assert!(panic.is_err());
    assert!(
        tree.compute(Size::new(400.0, 200.0))
            .unwrap()
            .snapshot
            .same_snapshot(&first)
    );
}

#[test]
fn invalid_dimensions_are_rejected_while_zero_viewports_are_supported() {
    let (scene, _, _, _) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |_| fixed(10.0, 10.0)).unwrap();
    let first = tree.compute(Size::new(100.0, 100.0)).unwrap().snapshot;
    for invalid in [f64::NAN, f64::INFINITY, -1.0, f64::MAX] {
        assert!(matches!(
            tree.compute(Size::new(invalid, 100.0)),
            Err(LayoutError::InvalidViewport)
        ));
        assert!(tree.snapshot().unwrap().same_snapshot(&first));
    }
    for invalid in [
        Style {
            width: Length::Px(-1.0),
            ..Default::default()
        },
        Style {
            height: Length::Percent(f32::INFINITY),
            ..Default::default()
        },
        Style {
            padding: Edges::all(-1.0),
            ..Default::default()
        },
        Style {
            intrinsic_size: Size::new(f64::MAX, 0.0),
            ..Default::default()
        },
        Style {
            grow: f32::NAN,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            tree.sync(&scene, |_| invalid),
            Err(LayoutError::InvalidStyle { .. })
        ));
    }
    let zero = tree.compute(Size::new(0.0, 0.0)).unwrap();
    assert_eq!(zero.snapshot.viewport(), Size::new(0.0, 0.0));
    assert_eq!(zero.snapshot.boxes().len(), 3);
}
