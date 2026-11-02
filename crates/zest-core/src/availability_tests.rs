use super::*;
use input::{ActivationKey, ActivationState};

fn fixture() -> (Scene, WidgetId, WidgetId, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("panel", |ui| {
                ui.button("a", "A");
                ui.container("nested", |ui| {
                    ui.button("b", "B");
                });
            });
            ui.button("outside", "Outside");
        })
        .unwrap();
    let panel = scene.roots()[0].clone();
    let outside = scene.roots()[1].clone();
    let children = scene.children(&panel).unwrap();
    let a = children[0].clone();
    let b = scene.children(&children[1]).unwrap()[0].clone();
    (scene, panel, a, b, outside)
}

#[test]
fn disabling_a_container_clears_interaction_and_preserves_local_child_settings() {
    let (mut scene, panel, a, b, outside) = fixture();
    let archetypes = scene.stats().archetypes;
    let mut input = ActivationState::default();
    scene.focus(&a);
    scene.set_hovered(&a, true);
    input.pointer_press(&mut scene, Some(&a));
    scene.set_enabled(&b, false);
    scene.set_enabled(&panel, false);
    assert!(scene.focused().is_none());
    for id in [&panel, &a, &b] {
        let state = scene.state(id).unwrap();
        assert!(!state.enabled() && !state.hovered() && !state.focused() && !state.pressed());
        assert!(state.paint_dirty());
    }
    assert_eq!(scene.locally_enabled(&a), Some(true));
    assert_eq!(scene.locally_enabled(&b), Some(false));
    scene.focus(&outside);
    assert!(!scene.focus(&a) && !scene.set_hovered(&a, true));
    assert_eq!(scene.focused(), Some(&outside));
    scene.set_enabled(&panel, true);
    assert!(scene.state(&a).unwrap().enabled());
    assert!(!scene.state(&b).unwrap().enabled());
    assert!(!scene.state(&a).unwrap().hovered() && !scene.state(&a).unwrap().pressed());
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_none()
    );
    scene.set_enabled(&panel, false);
    scene.set_enabled(&b, true);
    assert_eq!(scene.locally_enabled(&b), Some(true));
    assert!(!scene.state(&b).unwrap().enabled());
    scene.set_enabled(&panel, true);
    assert!(scene.state(&b).unwrap().enabled());
    assert_eq!(scene.stats().archetypes, archetypes);
}

#[test]
fn new_partial_children_inherit_disabled_ancestors_without_reenabling_retained_nodes() {
    let (mut scene, panel, a, b, outside) = fixture();
    scene.set_enabled(&panel, false);
    scene.set_enabled(&b, false);
    scene
        .update_children(&panel, |ui| {
            ui.container("nested", |ui| {
                ui.button("b", "B renamed");
                ui.button("new", "New child");
            });
            ui.button("a", "A renamed");
        })
        .unwrap();
    let nested = scene.children(&panel).unwrap()[0].clone();
    let new = scene.children(&nested).unwrap()[1].clone();
    assert!(!scene.state(&new).unwrap().enabled());
    assert_eq!(scene.locally_enabled(&new), Some(true));
    assert!(scene.state(&outside).unwrap().enabled());
    scene.set_enabled(&panel, true);
    assert!(scene.state(&new).unwrap().enabled() && scene.state(&a).unwrap().enabled());
    assert!(!scene.state(&b).unwrap().enabled());
    scene
        .update(|ui| {
            ui.button("outside", "Outside");
            ui.container("panel", |ui| {
                ui.container("nested", |ui| {
                    ui.button("b", "B");
                });
            });
        })
        .unwrap();
    assert!(!scene.state(&b).unwrap().enabled());
    scene.update_children(&panel, |_| {}).unwrap();
    scene
        .update_children(&panel, |ui| {
            ui.container("nested", |ui| {
                ui.button("b", "Fresh B");
            });
        })
        .unwrap();
    assert!(scene.state(&b).unwrap().enabled());
    assert!(!scene.set_enabled(&a, false));
    assert_eq!(scene.locally_enabled(&a), None);
}

#[test]
fn transient_disable_or_focus_loss_cannot_resurrect_captured_input() {
    let (mut scene, panel, a, _, outside) = fixture();
    let mut input = ActivationState::default();
    input.pointer_press(&mut scene, Some(&a));
    input.pointer_move(&mut scene, None);
    scene.set_enabled(&panel, false);
    scene.set_enabled(&panel, true);
    assert!(!input.pointer_move(&mut scene, Some(&a)));
    assert!(input.captured().is_none());
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_none()
    );
    scene.focus(&a);
    input.key(
        &mut scene,
        ActivationKey::Enter,
        true,
        false,
        false,
        Some(&a),
    );
    scene.set_enabled(&panel, false);
    scene.set_enabled(&panel, true);
    scene.focus(&a);
    assert!(
        input
            .key(
                &mut scene,
                ActivationKey::Enter,
                false,
                false,
                false,
                Some(&a)
            )
            .activated
            .is_none()
    );
    input.key(
        &mut scene,
        ActivationKey::Space,
        true,
        false,
        false,
        Some(&a),
    );
    scene.focus(&outside);
    scene.focus(&a);
    // No controller reconcile happened between the two focus changes.
    assert!(
        input
            .key(
                &mut scene,
                ActivationKey::Space,
                false,
                false,
                false,
                Some(&a)
            )
            .activated
            .is_none()
    );
    scene.set_enabled(&a, false);
    assert!(!input.pointer_press(&mut scene, Some(&a)).changed);
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_none()
    );
    assert!(
        !input
            .key(
                &mut scene,
                ActivationKey::Enter,
                true,
                false,
                false,
                Some(&a)
            )
            .changed
    );
    assert!(!scene.state(&a).unwrap().pressed());
}

#[test]
fn focus_traversal_is_preorder_wraps_and_filters_disabled_subtrees_and_visibility() {
    let (mut scene, panel, a, b, outside) = fixture();
    for expected in [&a, &b, &outside, &a] {
        assert!(scene.focus_next(FocusDirection::Forward, |_| true));
        assert_eq!(scene.focused(), Some(expected));
    }
    assert!(scene.focus_next(FocusDirection::Backward, |_| true));
    assert_eq!(scene.focused(), Some(&outside));
    scene.set_enabled(&panel, false);
    assert!(!scene.focus_next(FocusDirection::Forward, |_| true));
    assert_eq!(scene.focused(), Some(&outside));
    scene.set_enabled(&panel, true);
    assert!(scene.focus_next(FocusDirection::Forward, |id| id != &a));
    assert_eq!(scene.focused(), Some(&b));
    assert!(scene.focus_next(FocusDirection::Forward, |_| false));
    assert!(scene.focused().is_none());
    assert!(!scene.focus_next(FocusDirection::Backward, |_| false));
    assert!(scene.focus_next(FocusDirection::Backward, |_| true));
    assert_eq!(scene.focused(), Some(&outside));
    scene.focus(&panel);
    scene.focus_next(FocusDirection::Forward, |_| true);
    assert_eq!(scene.focused(), Some(&a));
}

#[test]
fn focus_traversal_follows_reconciled_order_and_handles_a_removed_focus() {
    let (mut scene, panel, a, b, outside) = fixture();
    scene.focus(&a);
    scene
        .update_children(&panel, |ui| {
            ui.container("nested", |ui| {
                ui.button("b", "B");
            });
            ui.button("a", "A");
        })
        .unwrap();
    scene.focus_next(FocusDirection::Backward, |_| true);
    assert_eq!(scene.focused(), Some(&b));
    scene.update_children(&panel, |_| {}).unwrap();
    scene.focus_next(FocusDirection::Forward, |_| true);
    assert_eq!(scene.focused(), Some(&outside));
}

#[test]
fn availability_changes_touch_only_their_subtree_and_invalid_updates_are_atomic() {
    let (mut scene, panel, _, _, outside) = fixture();
    for &entity in scene.entities.values() {
        scene
            .world
            .get_mut::<WidgetState>(entity)
            .unwrap()
            .set(WidgetState::PAINT_DIRTY, false);
    }
    scene.focus(&outside);
    let before = scene.state(&outside).unwrap();
    scene.set_enabled(&panel, false);
    assert_eq!(scene.state(&outside), Some(before));
    assert_eq!(scene.focused(), Some(&outside));
    let disabled = scene.state(&panel).unwrap();
    assert!(matches!(
        scene.update_children(&panel, |ui| {
            ui.button("duplicate", "One");
            ui.button("duplicate", "Two");
        }),
        Err(ReconcileError::DuplicateId(_))
    ));
    assert_eq!(scene.state(&panel), Some(disabled));
    assert_eq!(scene.state(&outside), Some(before));
    assert!(scene.set_enabled(&panel, false));
    assert_eq!(scene.state(&panel), Some(disabled));
}
