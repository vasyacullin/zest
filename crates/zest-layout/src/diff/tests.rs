use super::*;
use crate::{ComputeStats, LayoutTree, Length, Style};
use zest_core::{
    Scene,
    geometry::{Point, Size},
};

fn scene() -> (Scene, WidgetId, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("root", |ui| {
                ui.button("one", "One");
                ui.button("two", "Two");
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let one = scene.children(&root).unwrap()[0].clone();
    let two = scene.children(&root).unwrap()[1].clone();
    (scene, root, one, two)
}
fn style(id: &WidgetId, root: &WidgetId) -> Style {
    let (width, height) = if id == root {
        (100.0, 50.0)
    } else {
        (80.0, 40.0)
    };
    Style {
        width: Length::Px(width),
        height: Length::Px(height),
        shrink: 0.0,
        ..Default::default()
    }
}
fn viewport() -> Size<zest_core::geometry::Logical> {
    Size::new(200.0, 200.0)
}

#[test]
fn shared_and_independently_equal_snapshots_have_no_changes() {
    let (scene, root, _, _) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |id| style(id, &root)).unwrap();
    let old = tree.compute(viewport()).unwrap().snapshot;
    let cached = tree.compute(viewport()).unwrap().snapshot;
    assert!(old.same_snapshot(&cached));
    assert_eq!(cached.changes_since(&old), LayoutChanges::default());
    let mut other = LayoutTree::default();
    other.sync(&scene, |id| style(id, &root)).unwrap();
    let equal = other.compute(viewport()).unwrap().snapshot;
    assert!(!old.same_snapshot(&equal));
    assert!(equal.changes_since(&old).is_empty());
}

#[test]
fn resizing_one_node_reports_it_and_its_moved_sibling_in_current_order() {
    let (scene, root, one, two) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |id| style(id, &root)).unwrap();
    let old = tree.compute(viewport()).unwrap().snapshot;
    tree.sync(&scene, |id| {
        let mut s = style(id, &root);
        if id == &one {
            s.height = Length::Px(60.0);
        }
        s
    })
    .unwrap();
    let new = tree.compute(viewport()).unwrap().snapshot;
    let changes = new.changes_since(&old);
    assert_eq!(changes.changed, [one, two.clone()]);
    assert!(changes.added.is_empty() && changes.removed.is_empty());
    assert!(!changes.viewport_changed && !changes.order_changed);
    assert_eq!(old.bounds(&two).unwrap().origin.y, 40.0);
    assert_eq!(new.bounds(&two).unwrap().origin.y, 60.0);
}

#[test]
fn clipping_policy_is_observable_even_when_all_boxes_and_clips_match() {
    let (scene, root, _, _) = scene();
    let mut tree = LayoutTree::default();
    let fitting = |id: &WidgetId, clip| {
        let mut s = style(id, &root);
        if id == &root {
            s.height = Length::Px(100.0);
            s.clip_children = clip;
        }
        s
    };
    tree.sync(&scene, |id| fitting(id, false)).unwrap();
    let old = tree.compute(Size::new(100.0, 100.0)).unwrap().snapshot;
    tree.sync(&scene, |id| fitting(id, true)).unwrap();
    let new = tree.compute(Size::new(100.0, 100.0)).unwrap();
    assert_eq!(new.stats, ComputeStats::default());
    assert_eq!(new.snapshot.boxes(), old.boxes());
    assert_eq!(new.snapshot.changes_since(&old).changed, [root]);
}

#[test]
fn scrolling_reports_owner_metrics_and_projected_children_without_relayout() {
    let (scene, root, one, two) = scene();
    let mut tree = LayoutTree::default();
    let scrolling = |id: &WidgetId, offset| {
        let mut s = style(id, &root);
        if id == &root {
            s.clip_children = true;
            s.scroll_offset = Point::new(0.0, offset);
        }
        s
    };
    tree.sync(&scene, |id| scrolling(id, 0.0)).unwrap();
    let old = tree.compute(viewport()).unwrap().snapshot;
    tree.sync(&scene, |id| scrolling(id, 25.0)).unwrap();
    let new = tree.compute(viewport()).unwrap();
    assert_eq!(new.stats, ComputeStats::default());
    assert_eq!(
        new.snapshot.changes_since(&old).changed,
        [root.clone(), one, two]
    );
    assert_eq!(old.scroll(&root).unwrap().offset.y, 0.0);
    assert_eq!(new.snapshot.scroll(&root).unwrap().offset.y, 25.0);
    assert_eq!(
        new.snapshot.boxes()[1].layout_bounds,
        old.boxes()[1].layout_bounds
    );
}

#[test]
fn zero_size_reorder_changes_sequence_without_changing_retained_boxes() {
    let (mut scene, root, _, _) = scene();
    let mut tree = LayoutTree::default();
    let zero = |_: &WidgetId| Style {
        width: Length::Px(0.0),
        height: Length::Px(0.0),
        ..Default::default()
    };
    tree.sync(&scene, zero).unwrap();
    let old = tree.compute(viewport()).unwrap().snapshot;
    scene
        .update_children(&root, |ui| {
            ui.button("two", "Two");
            ui.button("one", "One");
        })
        .unwrap();
    tree.sync(&scene, zero).unwrap();
    let new = tree.compute(viewport()).unwrap().snapshot;
    let changes = new.changes_since(&old);
    assert!(changes.changed.is_empty() && changes.added.is_empty() && changes.removed.is_empty());
    assert!(changes.order_changed && !changes.viewport_changed && !changes.is_empty());
}

#[test]
fn additions_removals_and_empty_snapshots_keep_deterministic_identity_lists() {
    let (mut scene, root, one, two) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |id| style(id, &root)).unwrap();
    let old = tree.compute(viewport()).unwrap().snapshot;
    scene
        .update_children(&root, |ui| {
            ui.button("new", "New");
            ui.button("two", "Two");
        })
        .unwrap();
    let added = scene.children(&root).unwrap()[0].clone();
    tree.sync(&scene, |id| style(id, &root)).unwrap();
    let new = tree.compute(viewport()).unwrap().snapshot;
    let changes = new.changes_since(&old);
    assert_eq!(changes.added, std::slice::from_ref(&added));
    assert_eq!(changes.removed, [one]);
    assert!(changes.changed.is_empty() && changes.order_changed);
    scene.update(|_| {}).unwrap();
    tree.sync(&scene, |_| Style::default()).unwrap();
    let empty = tree.compute(viewport()).unwrap().snapshot;
    let changes = empty.changes_since(&new);
    assert_eq!(changes.removed, [root, added, two]);
    assert!(changes.added.is_empty() && changes.changed.is_empty());
    assert!(changes.order_changed && !changes.is_empty());
}

#[test]
fn viewport_changes_are_reported_even_for_empty_or_fully_clipped_layouts() {
    let mut tree = LayoutTree::default();
    tree.sync(&Scene::default(), |_| Style::default()).unwrap();
    let old = tree.compute(viewport()).unwrap().snapshot;
    let new = tree.compute(Size::new(0.0, 0.0)).unwrap().snapshot;
    let changes = new.changes_since(&old);
    assert!(changes.viewport_changed && !changes.order_changed && !changes.is_empty());
    assert!(changes.changed.is_empty() && changes.added.is_empty() && changes.removed.is_empty());
    let (scene, root, one, two) = scene();
    tree.sync(&scene, |id| style(id, &root)).unwrap();
    let old = tree.compute(viewport()).unwrap().snapshot;
    let new = tree.compute(Size::new(0.0, 0.0)).unwrap().snapshot;
    assert_eq!(new.changes_since(&old).changed, [root, one, two]);
}

#[test]
fn scene_appearance_recreation_and_failed_style_inputs_are_not_layout_data_changes() {
    let (mut scene, root, one, _) = scene();
    let mut tree = LayoutTree::default();
    tree.sync(&scene, |id| style(id, &root)).unwrap();
    let old = tree.compute(viewport()).unwrap().snapshot;
    scene.focus(&one);
    scene.set_hovered(&one, true);
    scene.set_enabled(&one, false);
    scene
        .update_children(&root, |ui| {
            ui.button("one", "Renamed");
            ui.button("two", "Two");
        })
        .unwrap();
    tree.sync(&scene, |id| style(id, &root)).unwrap();
    assert!(
        tree.compute(viewport())
            .unwrap()
            .snapshot
            .changes_since(&old)
            .is_empty()
    );
    assert!(
        tree.sync(&scene, |_| Style {
            width: Length::Px(f32::NAN),
            ..Default::default()
        })
        .is_err()
    );
    assert!(
        tree.compute(viewport())
            .unwrap()
            .snapshot
            .changes_since(&old)
            .is_empty()
    );
    scene.update(|_| {}).unwrap();
    scene
        .update(|ui| {
            ui.container("root", |ui| {
                ui.button("one", "One");
                ui.button("two", "Two");
            });
        })
        .unwrap();
    tree.sync(&scene, |id| style(id, &root)).unwrap();
    assert!(
        tree.compute(viewport())
            .unwrap()
            .snapshot
            .changes_since(&old)
            .is_empty()
    );
    assert!(scene.paint_invalidation().structure_changed);
}
