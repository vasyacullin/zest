use super::*;
use crate::{Scene, input::ActivationState, semantics::SemanticProperties};

fn fixture() -> (Scene, WidgetId, WidgetId, WidgetId) {
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

#[test]
fn shared_and_repeated_observations_are_empty_and_comparison_is_read_only() {
    let (mut scene, _, one, _) = fixture();
    let old = scene.semantic_snapshot();
    assert!(old.same_snapshot(&old.clone()));
    assert_eq!(old.changes_since(&old.clone()), SemanticChanges::default());
    scene.take_paint_invalidation();
    scene.invalidate_paint(&one);
    let paint = scene.paint_invalidation();
    let current = scene.semantic_snapshot();
    assert!(!old.same_snapshot(&current));
    assert!(current.changes_since(&old).is_empty());
    assert_eq!(scene.paint_invalidation(), paint);
    assert!(
        scene
            .update(|ui| {
                ui.button("bad", "A");
                ui.button("bad", "B");
            })
            .is_err()
    );
    assert!(scene.semantic_snapshot().changes_since(&old).is_empty());
    scene.focus(&one);
    scene.clear_focus();
    assert!(scene.semantic_snapshot().changes_since(&old).is_empty());
}

#[test]
fn focus_hover_and_press_report_only_retained_changed_nodes_in_current_order() {
    let (mut scene, _, one, two) = fixture();
    scene.focus(&two);
    let old = scene.semantic_snapshot();
    scene.focus(&one);
    scene.set_hovered(&one, true);
    let mut input = ActivationState::default();
    input.pointer_press(&mut scene, Some(&one));
    let current = scene.semantic_snapshot();
    let changes = current.changes_since(&old);
    assert_eq!(changes.changed, [one.clone(), two]);
    assert!(changes.added.is_empty() && changes.removed.is_empty() && changes.replaced.is_empty());
    assert!(!changes.order_changed && !changes.roots_changed && !changes.modal_changed);
    assert!(!old.node(&one).unwrap().state.pressed);
    assert!(current.node(&one).unwrap().state.pressed);
}

#[test]
fn metadata_changes_are_visible_while_overridden_paint_labels_are_not() {
    let mut scene = Scene::default();
    let declare = |ui: &mut crate::Ui, label, name, description, test_id| {
        let id = ui.button("save", label);
        ui.set_semantics(
            &id,
            SemanticProperties::default()
                .named(name)
                .described(description)
                .test_id(test_id),
        );
    };
    scene
        .update(|ui| declare(ui, "💾", "Save", "Old", "save"))
        .unwrap();
    let old = scene.semantic_snapshot();
    let id = scene.roots()[0].clone();
    scene.take_paint_invalidation();
    scene
        .update(|ui| declare(ui, "💾", "Сохранить", "New", "save.document"))
        .unwrap();
    let current = scene.semantic_snapshot();
    assert_eq!(
        current.changes_since(&old).changed,
        std::slice::from_ref(&id)
    );
    assert!(!scene.needs_paint());
    scene
        .update(|ui| declare(ui, "Save file", "Сохранить", "New", "save.document"))
        .unwrap();
    assert!(scene.needs_paint());
    assert!(scene.semantic_snapshot().changes_since(&current).is_empty());
    assert_eq!(old.node(&id).unwrap().name, "Save");
}

#[test]
fn added_removed_children_and_cleared_roots_have_deterministic_lists_and_flags() {
    let (mut scene, root, one, two) = fixture();
    let old = scene.semantic_snapshot();
    scene
        .update_children(&root, |ui| {
            ui.button("new", "New");
            ui.button("two", "Two");
        })
        .unwrap();
    let added = scene.children(&root).unwrap()[0].clone();
    let current = scene.semantic_snapshot();
    let changes = current.changes_since(&old);
    assert_eq!(changes.added, std::slice::from_ref(&added));
    assert_eq!(changes.removed, [one]);
    assert_eq!(changes.changed, std::slice::from_ref(&root));
    assert!(changes.order_changed && !changes.roots_changed && changes.replaced.is_empty());
    scene.update(|_| {}).unwrap();
    let empty = scene.semantic_snapshot();
    let changes = empty.changes_since(&current);
    assert_eq!(changes.removed, [root, added, two]);
    assert!(changes.added.is_empty() && changes.changed.is_empty() && changes.replaced.is_empty());
    assert!(changes.order_changed && changes.roots_changed && !changes.is_empty());
    assert!(empty.changes_since(&scene.semantic_snapshot()).is_empty());
}

#[test]
fn child_and_root_reorders_report_sequence_and_actual_hierarchy_changes() {
    let (mut scene, root, _, _) = fixture();
    let old = scene.semantic_snapshot();
    scene
        .update_children(&root, |ui| {
            ui.button("two", "Two");
            ui.button("one", "One");
        })
        .unwrap();
    let changes = scene.semantic_snapshot().changes_since(&old);
    assert_eq!(changes.changed, [root]);
    assert!(changes.order_changed && !changes.roots_changed && changes.replaced.is_empty());
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.button("one", "One");
            ui.button("two", "Two");
        })
        .unwrap();
    let old = scene.semantic_snapshot();
    scene
        .update(|ui| {
            ui.button("two", "Two");
            ui.button("one", "One");
        })
        .unwrap();
    let changes = scene.semantic_snapshot().changes_since(&old);
    assert!(changes.changed.is_empty() && changes.replaced.is_empty());
    assert!(changes.order_changed && changes.roots_changed);
}

#[test]
fn empty_modal_scope_changes_are_reported_without_inventing_node_updates() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("empty", |_| {});
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let old = scene.semantic_snapshot();
    scene.push_modal(&root);
    let scoped = scene.semantic_snapshot();
    let changes = scoped.changes_since(&old);
    assert!(changes.modal_changed && !changes.is_empty());
    assert!(changes.changed.is_empty() && !changes.order_changed && !changes.roots_changed);
    scene.push_modal(&root); // Innermost instance/data stay equal despite stack depth.
    assert!(scene.semantic_snapshot().changes_since(&scoped).is_empty());
    scene.set_enabled(&root, false); // Closes both scopes.
    let changes = scene.semantic_snapshot().changes_since(&scoped);
    assert!(changes.modal_changed);
    assert_eq!(changes.changed, [root]);
}

#[test]
fn recreated_paths_are_replaced_even_with_identical_data_and_modal_root_name() {
    let (mut scene, root, one, _) = fixture();
    let old = scene.semantic_snapshot();
    scene
        .update_children(&root, |ui| {
            ui.button("two", "Two");
        })
        .unwrap();
    scene
        .update_children(&root, |ui| {
            ui.button("one", "One");
            ui.button("two", "Two");
        })
        .unwrap();
    let current = scene.semantic_snapshot();
    assert_eq!(old.nodes(), current.nodes());
    let changes = current.changes_since(&old);
    assert_eq!(changes.replaced, std::slice::from_ref(&one));
    assert!(changes.changed.is_empty() && changes.added.is_empty() && changes.removed.is_empty());
    assert!(!changes.order_changed && !changes.roots_changed && !changes.modal_changed);
    scene.push_modal(&one);
    let scoped = scene.semantic_snapshot();
    scene
        .update_children(&root, |ui| {
            ui.button("two", "Two");
        })
        .unwrap();
    scene
        .update_children(&root, |ui| {
            ui.button("one", "One");
            ui.button("two", "Two");
        })
        .unwrap();
    scene.push_modal(&one);
    let new_scope = scene.semantic_snapshot();
    assert_eq!(scoped.nodes(), new_scope.nodes());
    let changes = new_scope.changes_since(&scoped);
    assert_eq!(changes.replaced, [one]);
    assert!(changes.modal_changed && changes.changed.is_empty());
}

#[test]
fn foreign_scenes_replace_matching_paths_without_exposing_private_ecs_stamps() {
    let (scene, _, _, _) = fixture();
    let (other, _, _, _) = fixture();
    let old = scene.semantic_snapshot();
    let current = other.semantic_snapshot();
    assert_eq!(old.nodes(), current.nodes());
    let changes = current.changes_since(&old);
    assert_eq!(
        changes.replaced,
        current
            .nodes()
            .iter()
            .map(|node| node.id.clone())
            .collect::<Vec<_>>()
    );
    assert!(changes.added.is_empty() && changes.removed.is_empty() && changes.changed.is_empty());
    assert!(!changes.order_changed && !changes.roots_changed && !changes.modal_changed);
    let debug = format!("{current:?}");
    assert!(!debug.contains("WorldId") && !debug.contains("Entity("));
    assert!(
        Scene::default()
            .semantic_snapshot()
            .changes_since(&Scene::default().semantic_snapshot())
            .is_empty()
    );
}
