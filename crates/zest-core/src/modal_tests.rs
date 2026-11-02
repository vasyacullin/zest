use super::*;
use input::{ActivationKey, ActivationState};

fn scene() -> (Scene, WidgetId, WidgetId, WidgetId, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.button("open", "Open");
            ui.container("dialog", |ui| {
                ui.button("first", "First");
                ui.container("nested", |ui| {
                    ui.button("last", "Last");
                });
            });
        })
        .unwrap();
    let open = scene.roots()[0].clone();
    let dialog = scene.roots()[1].clone();
    let first = scene.children(&dialog).unwrap()[0].clone();
    let nested = scene.children(&dialog).unwrap()[1].clone();
    let last = scene.children(&nested).unwrap()[0].clone();
    (scene, open, dialog, first, nested, last)
}

#[test]
fn modal_bounds_focus_hover_and_activation_without_disabling_outside_widgets() {
    let (mut scene, open, dialog, first, _, last) = scene();
    scene.focus(&open);
    scene.set_hovered(&open, true);
    let archetypes = scene.stats().archetypes;
    for &entity in scene.entities.values() {
        scene
            .world
            .get_mut::<WidgetState>(entity)
            .unwrap()
            .set(WidgetState::PAINT_DIRTY, false);
    }
    assert!(scene.push_modal(&dialog));
    assert_eq!(scene.modal_root(), Some(&dialog));
    assert_eq!(scene.focused(), Some(&first));
    assert!(scene.state(&dialog).unwrap().paint_dirty());
    assert!(scene.state(&open).unwrap().enabled());
    assert!(!scene.state(&open).unwrap().hovered());
    assert!(!scene.input_allowed(&open) && !scene.focus(&open) && !scene.set_hovered(&open, true));
    let mut input = ActivationState::default();
    assert!(!input.pointer_press(&mut scene, Some(&open)).changed);
    assert!(
        input
            .pointer_release(&mut scene, Some(&open))
            .activated
            .is_none()
    );
    scene.focus_next(FocusDirection::Forward, |_| true);
    assert_eq!(scene.focused(), Some(&last));
    scene.focus_next(FocusDirection::Forward, |_| true);
    assert_eq!(scene.focused(), Some(&first));
    scene.focus_next(FocusDirection::Backward, |_| true);
    assert_eq!(scene.focused(), Some(&last));
    assert!(scene.pop_modal());
    assert_eq!(scene.focused(), Some(&open));
    assert!(scene.modal_root().is_none() && scene.input_allowed(&open));
    assert_eq!(scene.stats().archetypes, archetypes);
    assert!(!scene.pop_modal());
}

#[test]
fn nested_modal_scopes_restore_each_previous_focus_and_reject_escape_roots_atomically() {
    let (mut scene, open, dialog, first, nested, last) = scene();
    scene.focus(&open);
    scene.push_modal(&dialog);
    scene.push_modal(&nested);
    assert_eq!(scene.focused(), Some(&last));
    assert!(!scene.push_modal(&open) && !scene.push_modal(&dialog));
    assert_eq!(scene.modal_root(), Some(&nested));
    assert_eq!(scene.focused(), Some(&last));
    scene
        .update_children(&nested, |ui| {
            ui.button("new", "New");
            ui.button("last", "Last renamed");
        })
        .unwrap();
    assert_eq!(scene.modal_root(), Some(&nested));
    assert_eq!(scene.focused(), Some(&last));
    // Nesting the same root is allowed and still pairs pushes with pops.
    scene.push_modal(&nested);
    scene.pop_modal();
    assert_eq!(scene.modal_root(), Some(&nested));
    assert_eq!(scene.focused(), Some(&last));
    scene.pop_modal();
    assert_eq!(scene.modal_root(), Some(&dialog));
    assert_eq!(scene.focused(), Some(&first));
    scene.pop_modal();
    assert_eq!(scene.focused(), Some(&open));
    scene.set_enabled(&dialog, false);
    assert!(!scene.push_modal(&dialog));
    assert!(scene.modal_root().is_none());
}

#[test]
fn removing_an_outer_scope_closes_its_nested_stack_and_restores_a_retained_focus() {
    let (mut scene, open, dialog, _, nested, _) = scene();
    scene.focus(&open);
    scene.push_modal(&dialog);
    scene.push_modal(&nested);
    scene
        .update(|ui| {
            ui.button("open", "Open renamed");
        })
        .unwrap();
    assert!(scene.modal_root().is_none());
    assert_eq!(scene.focused(), Some(&open));
    assert!(!scene.push_modal(&dialog) && !scene.pop_modal());
}

#[test]
fn inner_removal_restores_only_inside_the_remaining_scope_and_invalid_updates_preserve_stack() {
    let (mut scene, open, dialog, first, nested, _) = scene();
    scene.focus(&open);
    scene.push_modal(&dialog);
    scene.push_modal(&nested);
    assert!(
        scene
            .update_children(&dialog, |ui| {
                ui.button("duplicate", "A");
                ui.button("duplicate", "B");
            })
            .is_err()
    );
    assert_eq!(scene.modal_root(), Some(&nested));
    scene
        .update_children(&dialog, |ui| {
            ui.button("first", "First");
        })
        .unwrap();
    assert_eq!(scene.modal_root(), Some(&dialog));
    assert_eq!(scene.focused(), Some(&first));
    scene.pop_modal();
    assert_eq!(scene.focused(), Some(&open));
}

#[test]
fn disabled_scope_closes_immediately_and_reenabling_does_not_reopen_it() {
    let (mut scene, open, dialog, first, nested, _) = scene();
    scene.focus(&open);
    scene.push_modal(&dialog);
    scene.push_modal(&nested);
    scene.set_enabled(&nested, false);
    assert_eq!(scene.modal_root(), Some(&dialog));
    assert_eq!(scene.focused(), Some(&first));
    scene.set_enabled(&dialog, false);
    assert!(scene.modal_root().is_none());
    assert_eq!(scene.focused(), Some(&open));
    scene.set_enabled(&dialog, true);
    assert!(scene.modal_root().is_none());
    scene.push_modal(&dialog);
    scene.set_enabled(&open, false);
    scene.pop_modal();
    assert!(scene.focused().is_none());
}

#[test]
fn restored_focus_is_generation_checked_and_empty_scopes_trap_input_without_focus() {
    let (mut scene, open, dialog, _, _, _) = scene();
    scene.focus(&open);
    scene.push_modal(&dialog);
    scene
        .update(|ui| {
            ui.container("dialog", |_| {});
        })
        .unwrap();
    assert_eq!(scene.modal_root(), Some(&dialog));
    assert!(scene.focused().is_none());
    assert!(!scene.focus_next(FocusDirection::Forward, |_| true));
    scene
        .update(|ui| {
            assert_eq!(ui.button("open", "New incarnation"), open);
            ui.container("dialog", |_| {});
        })
        .unwrap();
    assert!(!scene.input_allowed(&open));
    scene.pop_modal();
    assert!(scene.focused().is_none());
    // A single button can also be an input scope.
    scene.push_modal(&open);
    assert_eq!(scene.focused(), Some(&open));
    assert!(!scene.focus_next(FocusDirection::Backward, |_| true));
    scene.pop_modal();
    assert!(scene.focused().is_none());
}

#[test]
fn every_successful_scope_transition_cancels_gestures_even_within_the_new_scope() {
    let (mut scene, open, dialog, first, nested, last) = scene();
    let mut input = ActivationState::default();
    scene.focus(&first);
    input.pointer_press(&mut scene, Some(&first));
    scene.push_modal(&dialog);
    assert!(!scene.state(&first).unwrap().pressed());
    assert!(
        input
            .pointer_release(&mut scene, Some(&first))
            .activated
            .is_none()
    );
    input.key(
        &mut scene,
        ActivationKey::Enter,
        true,
        false,
        false,
        Some(&first),
    );
    scene.push_modal(&nested);
    scene.pop_modal();
    assert_eq!(scene.focused(), Some(&first));
    assert!(
        input
            .key(
                &mut scene,
                ActivationKey::Enter,
                false,
                false,
                false,
                Some(&first)
            )
            .activated
            .is_none()
    );
    scene.push_modal(&nested);
    input.pointer_press(&mut scene, Some(&last));
    assert!(!scene.push_modal(&open));
    assert!(scene.state(&last).unwrap().pressed());
    assert!(
        input
            .pointer_release(&mut scene, Some(&last))
            .activated
            .is_some()
    );
    scene.pop_modal();
    scene.pop_modal();
}
