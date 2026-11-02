use super::*;
use crate::{ComputeStats, LayoutTree, Length, Style};
use zest_core::{WidgetId, geometry::Size, semantics::SemanticQuery};

struct Fixture {
    scene: Scene,
    root: WidgetId,
    inner: WidgetId,
    prefix: WidgetId,
    target: WidgetId,
}
fn fixture() -> Fixture {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("root", |ui| {
                ui.button("prefix", "Prefix");
                ui.container("inner", |ui| {
                    ui.button("prefix", "Prefix");
                    ui.button("target", "Target");
                });
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let prefix = scene.children(&root).unwrap()[0].clone();
    let inner = scene.children(&root).unwrap()[1].clone();
    let target = scene.children(&inner).unwrap()[1].clone();
    Fixture {
        scene,
        root,
        inner,
        prefix,
        target,
    }
}
impl Fixture {
    fn style(&self, id: &WidgetId, adjustments: &[SemanticScrollAdjustment]) -> Style {
        let (width, height, clip) = if id == &self.root {
            (100.0, 80.0, true)
        } else if id == &self.inner {
            (80.0, 60.0, true)
        } else if id == &self.target {
            (30.0, 20.0, false)
        } else if id == &self.prefix {
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
                .find(|change| change.owner().id() == id)
                .map_or(Point::new(0.0, 0.0), SemanticScrollAdjustment::offset),
            ..Default::default()
        }
    }
    fn frame(
        &self,
        tree: &mut LayoutTree,
        adjustments: &[SemanticScrollAdjustment],
    ) -> crate::ComputedLayout {
        tree.sync(&self.scene, |id| self.style(id, adjustments))
            .unwrap();
        tree.compute(Size::new(100.0, 80.0)).unwrap()
    }
}

#[test]
fn nested_plans_bind_target_and_changed_owners_and_keep_planning_read_only() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let frame = f.frame(&mut tree, &[]).snapshot;
    let view = frame.semantic_view(&f.scene.semantic_snapshot());
    let target = view
        .semantics()
        .find_target(SemanticQuery::new().named("Target"))
        .unwrap();
    let paint = f.scene.paint_invalidation();
    let plan = view.reveal_target(&target).unwrap();
    assert_eq!(plan.target(), &target);
    assert!(plan.source_layout().same_snapshot(&frame));
    assert!(plan.can_apply(&f.scene, &frame));
    assert_eq!(
        plan.adjustments()
            .iter()
            .map(|change| change.owner().id())
            .collect::<Vec<_>>(),
        [&f.inner, &f.root]
    );
    assert_eq!(
        plan.adjustments()
            .iter()
            .map(SemanticScrollAdjustment::offset)
            .collect::<Vec<_>>(),
        [Point::new(0.0, 60.0), Point::new(0.0, 40.0)]
    );
    assert_eq!(
        plan.visible_bounds(),
        Rect::from_xywh(0.0, 60.0, 30.0, 20.0)
    );
    assert_eq!(
        frame.reveal_plan(target.id()).unwrap().visible_bounds,
        plan.visible_bounds()
    );
    assert!(view.node(target.id()).unwrap().visible_bounds().is_none());
    assert_eq!(f.scene.paint_invalidation(), paint);
    assert!(f.scene.focused().is_none());
}

#[test]
fn applying_offsets_recomputes_projection_and_requires_a_new_plan_before_reuse() {
    let mut f = fixture();
    let mut tree = LayoutTree::default();
    let old = f.frame(&mut tree, &[]).snapshot;
    let view = old.semantic_view(&f.scene.semantic_snapshot());
    let target = view.semantics().target(&f.target).unwrap();
    let plan = view.reveal_target(&target).unwrap();
    let current = f.frame(&mut tree, plan.adjustments());
    assert_eq!(current.stats, ComputeStats::default());
    assert_eq!(
        current.snapshot.visible_bounds(&f.target),
        Some(plan.visible_bounds())
    );
    assert!(!plan.can_apply(&f.scene, &current.snapshot));
    assert!(old.visible_bounds(&f.target).is_none());
    let visible = current
        .snapshot
        .semantic_view(&f.scene.semantic_snapshot())
        .reveal_target(&target)
        .unwrap();
    assert!(visible.adjustments().is_empty() && visible.can_apply(&f.scene, &current.snapshot));
    assert!(visible.target().focus(&mut f.scene));
    assert!(visible.can_apply(&f.scene, &current.snapshot));
}

#[test]
fn exact_source_identity_rejects_new_geometry_even_when_computed_data_are_equal() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let frame = f.frame(&mut tree, &[]).snapshot;
    let plan = frame
        .semantic_view(&f.scene.semantic_snapshot())
        .reveal_target(&f.scene.semantic_snapshot().target(&f.target).unwrap())
        .unwrap();
    assert!(plan.can_apply(&f.scene, &frame.clone()));
    let mut other = LayoutTree::default();
    let equal = f.frame(&mut other, &[]).snapshot;
    assert!(equal.changes_since(&frame).is_empty());
    assert!(!plan.can_apply(&f.scene, &equal));
    let resized = tree.compute(Size::new(120.0, 100.0)).unwrap().snapshot;
    assert!(!plan.can_apply(&f.scene, &resized));
}

#[test]
fn current_availability_and_modal_rules_guard_target_and_all_changed_owners() {
    let mut f = fixture();
    let mut tree = LayoutTree::default();
    let frame = f.frame(&mut tree, &[]).snapshot;
    let target = f.scene.semantic_snapshot().target(&f.target).unwrap();
    let plan = frame
        .semantic_view(&f.scene.semantic_snapshot())
        .reveal_target(&target)
        .unwrap();
    f.scene.set_enabled(&f.target, false);
    assert!(!plan.can_apply(&f.scene, &frame));
    f.scene.set_enabled(&f.target, true);
    assert!(plan.can_apply(&f.scene, &frame));
    f.scene.push_modal(&f.inner);
    assert!(target.input_allowed(&f.scene));
    assert!(!plan.can_apply(&f.scene, &frame)); // Outer owner is outside the modal scope.
    f.scene.pop_modal();
    f.scene.push_modal(&f.root);
    assert!(plan.can_apply(&f.scene, &frame));
}

#[test]
fn visible_empty_plans_need_no_permission_to_edit_unchanged_outer_owners() {
    let mut f = fixture();
    let mut tree = LayoutTree::default();
    let frame = f.frame(&mut tree, &[]).snapshot;
    let target = f.scene.semantic_snapshot().target(&f.target).unwrap();
    let plan = frame
        .semantic_view(&f.scene.semantic_snapshot())
        .reveal_target(&target)
        .unwrap();
    let revealed = f.frame(&mut tree, plan.adjustments()).snapshot;
    f.scene.push_modal(&f.target);
    let visible = revealed
        .semantic_view(&f.scene.semantic_snapshot())
        .reveal_target(&target)
        .unwrap();
    assert!(visible.adjustments().is_empty());
    assert!(visible.can_apply(&f.scene, &revealed));
}

#[test]
fn recreated_and_foreign_bindings_never_upgrade_to_a_different_instance() {
    let mut f = fixture();
    let mut tree = LayoutTree::default();
    let frame = f.frame(&mut tree, &[]).snapshot;
    let view = frame.semantic_view(&f.scene.semantic_snapshot());
    let stale = view.semantics().target(&f.target).unwrap();
    let plan = view.reveal_target(&stale).unwrap();
    f.scene
        .update_children(&f.inner, |ui| {
            ui.button("prefix", "Prefix");
        })
        .unwrap();
    f.scene
        .update_children(&f.inner, |ui| {
            ui.button("prefix", "Prefix");
            ui.button("target", "Target");
        })
        .unwrap();
    assert!(!plan.can_apply(&f.scene, &frame));
    let fresh = f.scene.semantic_snapshot().target(&f.target).unwrap();
    assert!(view.reveal_target(&fresh).is_none());
    let current = f.frame(&mut tree, &[]).snapshot;
    assert!(current.same_snapshot(&frame));
    let fresh_plan = current
        .semantic_view(&f.scene.semantic_snapshot())
        .reveal_target(&fresh)
        .unwrap();
    assert!(fresh_plan.can_apply(&f.scene, &current));
    let other = fixture();
    let foreign = other
        .scene
        .semantic_snapshot()
        .target(&other.target)
        .unwrap();
    assert!(view.reveal_target(&foreign).is_none());
    assert!(!fresh_plan.can_apply(&other.scene, &current));
}

#[test]
fn missing_semantics_or_geometry_yields_no_bound_plan() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let frame = f.frame(&mut tree, &[]).snapshot;
    let semantics = f.scene.semantic_snapshot();
    let target = semantics.target(&f.target).unwrap();
    assert!(
        frame
            .semantic_view(&Scene::default().semantic_snapshot())
            .reveal_target(&target)
            .is_none()
    );
    let mut empty_tree = LayoutTree::default();
    empty_tree
        .sync(&Scene::default(), |_| Style::default())
        .unwrap();
    let empty = empty_tree.compute(Size::new(100.0, 80.0)).unwrap().snapshot;
    assert!(
        empty
            .semantic_view(&semantics)
            .reveal_target(&target)
            .is_none()
    );
}

#[test]
fn empty_zero_viewport_and_unreachable_unclipped_geometry_yield_no_plan() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&f.scene, |id| {
        let mut style = f.style(id, &[]);
        if id == &f.target {
            style.width = Length::Px(0.0);
        }
        style
    })
    .unwrap();
    let empty = tree.compute(Size::new(100.0, 80.0)).unwrap().snapshot;
    let semantics = f.scene.semantic_snapshot();
    let target = semantics.target(&f.target).unwrap();
    assert!(
        empty
            .semantic_view(&semantics)
            .reveal_target(&target)
            .is_none()
    );
    let zero = tree.compute(Size::new(0.0, 0.0)).unwrap().snapshot;
    assert!(
        zero.semantic_view(&semantics)
            .reveal_target(&target)
            .is_none()
    );
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.button("prefix", "Prefix");
            ui.button("target", "Target");
        })
        .unwrap();
    let id = scene.roots()[1].clone();
    tree.sync(&scene, |node| Style {
        width: Length::Px(100.0),
        height: Length::Px(if node == &id { 20.0 } else { 120.0 }),
        shrink: 0.0,
        ..Default::default()
    })
    .unwrap();
    let unreachable = tree.compute(Size::new(100.0, 50.0)).unwrap().snapshot;
    let semantics = scene.semantic_snapshot();
    assert!(
        unreachable
            .semantic_view(&semantics)
            .reveal_target(&semantics.target(&id).unwrap())
            .is_none()
    );
}
