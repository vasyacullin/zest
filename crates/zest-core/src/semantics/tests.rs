use super::*;
use crate::input::ActivationState;

struct Fixture {
    scene: Scene,
    root: WidgetId,
    save: WidgetId,
    inner: WidgetId,
    nested: WidgetId,
    outside: WidgetId,
}
fn fixture() -> Fixture {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.push_id("window", |ui| {
                ui.container("root", |ui| {
                    ui.button("save", "Сохранить");
                    ui.push_id("component", |ui| {
                        ui.container("inner", |ui| {
                            ui.button("save", "Сохранить");
                        });
                    });
                });
                ui.button("outside", "Outside");
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let outside = scene.roots()[1].clone();
    let save = scene.children(&root).unwrap()[0].clone();
    let inner = scene.children(&root).unwrap()[1].clone();
    let nested = scene.children(&inner).unwrap()[0].clone();
    Fixture {
        scene,
        root,
        save,
        inner,
        nested,
        outside,
    }
}

#[test]
fn snapshot_preserves_hierarchy_preorder_and_omits_identity_scopes() {
    let f = fixture();
    let snapshot = f.scene.semantic_snapshot();
    assert_eq!(snapshot.roots(), [f.root.clone(), f.outside.clone()]);
    assert_eq!(
        snapshot
            .nodes()
            .iter()
            .map(|node| &node.id)
            .collect::<Vec<_>>(),
        [&f.root, &f.save, &f.inner, &f.nested, &f.outside]
    );
    let root = snapshot.node(&f.root).unwrap();
    assert_eq!(root.children, [f.save.clone(), f.inner.clone()]);
    assert_eq!(root.parent, None);
    assert_eq!(root.role, SemanticRole::Group);
    assert!(root.name.is_empty());
    let save = snapshot.node(&f.nested).unwrap();
    assert_eq!(save.parent.as_ref(), Some(&f.inner));
    assert_eq!(save.role, SemanticRole::Button);
    assert!(save.children.is_empty());
}

#[test]
fn unique_queries_diagnose_ambiguity_and_match_unicode_names_exactly() {
    let f = fixture();
    let snapshot = f.scene.semantic_snapshot();
    let query = SemanticQuery::new()
        .role(SemanticRole::Button)
        .named("Сохранить");
    assert_eq!(
        snapshot.find(query),
        Err(FindError::Ambiguous(vec![f.save, f.nested]))
    );
    assert_eq!(snapshot.find_all(query).len(), 2);
    for name in ["сохранить", "Сохранить ", "Save", ""] {
        assert_eq!(snapshot.find(query.named(name)), Err(FindError::NotFound));
    }
    assert_eq!(snapshot.find(query.named("Outside")).unwrap().id, f.outside);
    assert!(FindError::NotFound.to_string().contains("no widgets"));
    assert!(
        FindError::Ambiguous(vec![f.outside])
            .to_string()
            .contains("1 widgets")
    );
}

#[test]
fn scoped_id_role_name_and_state_filters_are_combined_inclusively() {
    let f = fixture();
    let snapshot = f.scene.semantic_snapshot();
    let within = SemanticQuery::new().within(&f.inner);
    assert_eq!(
        snapshot
            .find_all(within)
            .iter()
            .map(|node| &node.id)
            .collect::<Vec<_>>(),
        [&f.inner, &f.nested]
    );
    assert_eq!(
        snapshot.find(within.role(SemanticRole::Group)).unwrap().id,
        f.inner
    );
    assert_eq!(
        snapshot
            .find(
                within
                    .role(SemanticRole::Button)
                    .named("Сохранить")
                    .id(&f.nested)
                    .enabled(true)
                    .input_allowed(true)
            )
            .unwrap()
            .id,
        f.nested
    );
    assert_eq!(snapshot.find(within.id(&f.save)), Err(FindError::NotFound));
    assert_eq!(
        snapshot.find(within.id(&f.inner).named("Сохранить")),
        Err(FindError::NotFound)
    );
    let mut other = Scene::default();
    other
        .update(|ui| {
            ui.button("missing", "Missing");
        })
        .unwrap();
    assert!(
        snapshot
            .find_all(SemanticQuery::new().within(&other.roots()[0]))
            .is_empty()
    );
}

#[test]
fn modal_and_inherited_availability_are_observed_without_hiding_blocked_nodes() {
    let mut f = fixture();
    f.scene.push_modal(&f.inner);
    let modal = f.scene.semantic_snapshot();
    assert_eq!(modal.modal_root(), Some(&f.inner));
    assert_eq!(modal.nodes().len(), 5);
    assert!(modal.node(&f.outside).unwrap().state.enabled);
    assert!(!modal.node(&f.outside).unwrap().state.input_allowed);
    assert_eq!(
        modal
            .find_all(SemanticQuery::new().input_allowed(true))
            .len(),
        2
    );
    f.scene.set_enabled(&f.root, false);
    let disabled = f.scene.semantic_snapshot();
    assert!(disabled.modal_root().is_none());
    assert_eq!(
        disabled.find_all(SemanticQuery::new().enabled(false)).len(),
        4
    );
    assert_eq!(
        disabled
            .find(SemanticQuery::new().input_allowed(true))
            .unwrap()
            .id,
        f.outside
    );
    assert!(modal.node(&f.nested).unwrap().state.enabled);
    assert!(modal.node(&f.nested).unwrap().state.focused);
}

#[test]
fn focus_hover_and_pressed_snapshots_follow_gestures_and_remain_immutable() {
    let mut f = fixture();
    f.scene.focus(&f.save);
    f.scene.set_hovered(&f.save, true);
    let mut input = ActivationState::default();
    input.pointer_press(&mut f.scene, Some(&f.save));
    let pressed = f.scene.semantic_snapshot();
    let state = pressed.node(&f.save).unwrap().state;
    assert!(state.focused && state.hovered && state.pressed);
    input.pointer_move(&mut f.scene, None);
    f.scene.set_hovered(&f.save, false);
    assert!(
        !f.scene
            .semantic_snapshot()
            .node(&f.save)
            .unwrap()
            .state
            .pressed
    );
    assert!(
        input
            .pointer_release(&mut f.scene, None)
            .activated
            .is_none()
    );
    f.scene.clear_focus();
    let released = f.scene.semantic_snapshot();
    let state = released.node(&f.save).unwrap().state;
    assert!(!state.focused && !state.hovered && !state.pressed);
    assert!(pressed.node(&f.save).unwrap().state.pressed);
}

#[test]
fn reads_and_paint_bookkeeping_do_not_change_semantic_data_or_scene_state() {
    let mut f = fixture();
    let paint = f.scene.paint_invalidation();
    let stats = f.scene.stats();
    let first = f.scene.semantic_snapshot();
    let clone = first.clone();
    assert!(std::ptr::eq(first.nodes().as_ptr(), clone.nodes().as_ptr()));
    assert_eq!(f.scene.paint_invalidation(), paint);
    assert_eq!(f.scene.stats(), stats);
    f.scene.take_paint_invalidation();
    f.scene.invalidate_paint(&f.nested);
    assert_eq!(first.nodes(), f.scene.semantic_snapshot().nodes());
    assert!(f.scene.focused().is_none());
}

#[test]
fn old_snapshots_survive_reorder_rename_removal_and_recreation() {
    let mut f = fixture();
    let old = f.scene.semantic_snapshot();
    f.scene
        .update_children(&f.root, |ui| {
            ui.push_id("component", |ui| {
                ui.container("inner", |ui| {
                    ui.button("save", "Сохранить");
                });
            });
            ui.button("save", "Renamed");
        })
        .unwrap();
    let new = f.scene.semantic_snapshot();
    assert_eq!(
        new.node(&f.root).unwrap().children,
        [f.inner.clone(), f.save.clone()]
    );
    assert_eq!(new.node(&f.save).unwrap().name, "Renamed");
    assert_eq!(old.node(&f.save).unwrap().name, "Сохранить");
    f.scene.update_children(&f.root, |_| {}).unwrap();
    assert!(f.scene.semantic_snapshot().node(&f.save).is_none());
    assert!(old.node(&f.save).is_some());
    f.scene
        .update_children(&f.root, |ui| {
            ui.button("save", "New generation");
        })
        .unwrap();
    assert_eq!(
        f.scene.semantic_snapshot().node(&f.save).unwrap().name,
        "New generation"
    );
    assert_eq!(new.node(&f.save).unwrap().name, "Renamed");
}

#[test]
fn empty_and_failed_declarations_have_consistent_snapshots() {
    let mut scene = Scene::default();
    let empty = scene.semantic_snapshot();
    assert!(empty.nodes().is_empty() && empty.roots().is_empty() && empty.modal_root().is_none());
    assert_eq!(empty.find(SemanticQuery::new()), Err(FindError::NotFound));
    scene
        .update(|ui| {
            ui.button("one", "One");
        })
        .unwrap();
    let accepted = scene.semantic_snapshot();
    assert!(
        scene
            .update(|ui| {
                ui.button("one", "A");
                ui.button("one", "B");
            })
            .is_err()
    );
    assert_eq!(accepted.nodes(), scene.semantic_snapshot().nodes());
    scene.update(|_| {}).unwrap();
    assert!(scene.semantic_snapshot().nodes().is_empty());
    assert_eq!(accepted.nodes()[0].name, "One");
}
