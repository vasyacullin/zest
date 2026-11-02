use super::*;
struct Fixture {
    scene: Scene,
    root: WidgetId,
    inner: WidgetId,
    leaf: WidgetId,
    footer: WidgetId,
}
fn fixture() -> Fixture {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("root", |ui| {
                ui.button("prefix", "Prefix");
                ui.container("inner", |ui| {
                    ui.button("leaf", "Leaf");
                });
                ui.button("footer", "Footer");
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let children = scene.children(&root).unwrap();
    let inner = children[1].clone();
    let footer = children[2].clone();
    let leaf = scene.children(&inner).unwrap()[0].clone();
    Fixture {
        scene,
        root,
        inner,
        leaf,
        footer,
    }
}
impl Fixture {
    fn style(&self, id: &WidgetId, changes: &[ScrollAdjustment]) -> Style {
        let (width, height, clip) = if id == &self.root {
            (100.0, 80.0, true)
        } else if id == &self.inner {
            (100.0, 60.0, true)
        } else if id == &self.leaf {
            (200.0, 160.0, false)
        } else if id == &self.footer {
            (100.0, 30.0, false)
        } else {
            (100.0, 60.0, false)
        };
        Style {
            width: Length::Px(width),
            height: Length::Px(height),
            shrink: 0.0,
            clip_children: clip,
            scroll_offset: changes
                .iter()
                .find(|change| change.widget == *id)
                .map_or(Point::new(0.0, 0.0), |change| change.offset),
            ..Default::default()
        }
    }
    fn tree(&self, changes: &[ScrollAdjustment]) -> (LayoutTree, LayoutSnapshot) {
        let mut tree = LayoutTree::default();
        tree.sync(&self.scene, |id| self.style(id, changes))
            .unwrap();
        let snapshot = tree.compute(Size::new(100.0, 80.0)).unwrap().snapshot;
        (tree, snapshot)
    }
}

#[test]
fn inner_consumption_and_outer_remainder_are_independent_per_axis_and_cache_only() {
    let f = fixture();
    let (mut tree, old) = f.tree(&[]);
    let plan = old
        .scroll_plan(&f.leaf, Point::new(60.0, 150.0), None)
        .unwrap();
    assert_eq!(
        plan.adjustments,
        vec![
            ScrollAdjustment {
                widget: f.inner.clone(),
                offset: Point::new(60.0, 100.0)
            },
            ScrollAdjustment {
                widget: f.root.clone(),
                offset: Point::new(0.0, 50.0)
            },
        ]
    );
    assert_eq!(plan.remaining, Point::new(0.0, 0.0));
    assert_eq!(old.scroll(&f.inner).unwrap().offset, Point::new(0.0, 0.0));
    tree.sync(&f.scene, |id| f.style(id, &plan.adjustments))
        .unwrap();
    let new = tree.compute(old.viewport()).unwrap();
    assert_eq!(new.stats, ComputeStats::default());
    assert_eq!(
        new.snapshot.scroll(&f.inner).unwrap().offset,
        Point::new(60.0, 100.0)
    );
    assert_eq!(
        new.snapshot.scroll(&f.root).unwrap().offset,
        Point::new(0.0, 50.0)
    );
    for node in old.boxes() {
        assert_eq!(
            node.layout_bounds,
            new.snapshot
                .boxes()
                .iter()
                .find(|other| other.id == node.id)
                .unwrap()
                .layout_bounds
        );
    }
}

#[test]
fn exhausted_ranges_return_the_rest_and_reverse_motion_uses_available_space() {
    let f = fixture();
    let (_, frame) = f.tree(&[]);
    let down = frame
        .scroll_plan(&f.leaf, Point::new(160.0, 250.0), None)
        .unwrap();
    assert_eq!(down.remaining, Point::new(60.0, 80.0));
    let (_, at_end) = f.tree(&down.adjustments);
    let same = at_end
        .scroll_plan(&f.leaf, Point::new(10.0, 10.0), None)
        .unwrap();
    assert!(same.adjustments.is_empty());
    assert_eq!(same.remaining, Point::new(10.0, 10.0));
    let up = at_end
        .scroll_plan(&f.leaf, Point::new(-160.0, -250.0), None)
        .unwrap();
    assert_eq!(up.remaining, Point::new(-60.0, -80.0));
    assert!(
        up.adjustments
            .iter()
            .all(|change| change.offset == Point::new(0.0, 0.0))
    );
}

#[test]
fn inclusive_boundaries_stop_bubbling_and_unrelated_or_unknown_boundaries_fail_closed() {
    let f = fixture();
    let (_, frame) = f.tree(&[]);
    let delta = Point::new(60.0, 150.0);
    let plan = frame.scroll_plan(&f.leaf, delta, Some(&f.inner)).unwrap();
    assert_eq!(plan.adjustments.len(), 1);
    assert_eq!(plan.adjustments[0].widget, f.inner);
    assert_eq!(plan.remaining, Point::new(0.0, 50.0));
    let leaf = frame.scroll_plan(&f.leaf, delta, Some(&f.leaf)).unwrap();
    assert!(leaf.adjustments.is_empty());
    assert_eq!(leaf.remaining, delta);
    assert!(frame.scroll_plan(&f.leaf, delta, Some(&f.footer)).is_none());
    let mut other = Scene::default();
    other
        .update(|ui| {
            ui.button("missing", "Missing");
        })
        .unwrap();
    assert!(
        frame
            .scroll_plan(&f.leaf, delta, Some(&other.roots()[0]))
            .is_none()
    );
}

#[test]
fn routing_includes_the_starting_scroller_and_never_enters_a_sibling_branch() {
    let f = fixture();
    let (_, frame) = f.tree(&[]);
    let direct = frame
        .scroll_plan(&f.inner, Point::new(0.0, 40.0), None)
        .unwrap();
    assert_eq!(direct.adjustments.len(), 1);
    assert_eq!(direct.adjustments[0].widget, f.inner);
    let sibling = frame
        .scroll_plan(&f.footer, Point::new(0.0, 40.0), None)
        .unwrap();
    assert_eq!(sibling.adjustments.len(), 1);
    assert_eq!(sibling.adjustments[0].widget, f.root);
}

#[test]
fn invalid_and_zero_deltas_do_not_mutate_the_snapshot_and_large_finite_deltas_are_bounded() {
    let f = fixture();
    let (mut tree, frame) = f.tree(&[]);
    for delta in [Point::new(f64::NAN, 1.0), Point::new(0.0, f64::INFINITY)] {
        assert!(frame.scroll_plan(&f.leaf, delta, None).is_none());
    }
    let zero = frame
        .scroll_plan(&f.leaf, Point::new(0.0, 0.0), None)
        .unwrap();
    assert!(zero.adjustments.is_empty());
    let huge = frame
        .scroll_plan(&f.leaf, Point::new(f64::MAX, f64::MAX), None)
        .unwrap();
    assert!(
        huge.adjustments
            .iter()
            .all(|change| change.offset.x.is_finite() && change.offset.y.is_finite())
    );
    assert!(huge.remaining.x.is_finite() && huge.remaining.y.is_finite());
    assert!(
        tree.compute(frame.viewport())
            .unwrap()
            .snapshot
            .same_snapshot(&frame)
    );
}

#[test]
fn float_rounding_never_sends_opposite_sign_residuals_to_an_ancestor() {
    for current in [0.0, 1e10, 1e30] {
        for delta in [-1.0, 1.0, -1e15, 1e15] {
            let (next, rest) = consume(current, 1e31, delta);
            assert!(next.is_finite() && (0.0..=1e31).contains(&next));
            assert!(
                rest.is_finite()
                    && (rest == 0.0 || rest.is_sign_positive() == delta.is_sign_positive())
            );
        }
    }
}
