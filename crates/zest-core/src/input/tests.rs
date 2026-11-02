use super::*;

fn scene() -> (Scene, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.button("a", "A");
            ui.button("b", "B");
        })
        .unwrap();
    let ids = scene.roots().to_vec();
    (scene, ids[0].clone(), ids[1].clone())
}
fn key(
    input: &mut ActivationState,
    scene: &mut Scene,
    key: ActivationKey,
    down: bool,
) -> InputResult {
    let focused = scene.focused().cloned();
    input.key(scene, key, down, false, false, focused.as_ref())
}

#[test]
fn pointer_capture_rearms_only_the_original_widget_and_emits_once() {
    let (mut scene, a, b) = scene();
    let mut input = ActivationState::default();
    let archetypes = scene.stats().archetypes;
    let entity = scene.entities[&a];
    scene
        .world
        .get_mut::<WidgetState>(entity)
        .unwrap()
        .set(WidgetState::PAINT_DIRTY, false);
    assert!(input.pointer_press(&mut scene, Some(&a)).changed);
    assert!(scene.state(&a).unwrap().pressed() && scene.state(&a).unwrap().paint_dirty());
    assert_eq!(input.captured(), Some(&a));
    assert!(!input.pointer_press(&mut scene, Some(&b)).changed);
    assert!(input.pointer_move(&mut scene, Some(&b)));
    assert!(!scene.state(&a).unwrap().pressed() && !scene.state(&b).unwrap().pressed());
    assert_eq!(input.captured(), Some(&a));
    assert!(input.pointer_move(&mut scene, Some(&a)));
    let action = input
        .pointer_release(&mut scene, Some(&a))
        .activated
        .unwrap();
    assert_eq!(
        action,
        Activation {
            widget: a.clone(),
            source: ActivationSource::Pointer
        }
    );
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_none()
    );
    assert!(!scene.state(&a).unwrap().pressed());
    assert_eq!(scene.stats().archetypes, archetypes);
}

#[test]
fn outside_or_sibling_release_and_pressing_blank_space_do_not_activate() {
    let (mut scene, a, b) = scene();
    let mut input = ActivationState::default();
    for release in [None, Some(&b)] {
        input.pointer_press(&mut scene, Some(&a));
        assert!(
            input
                .pointer_release(&mut scene, release)
                .activated
                .is_none()
        );
        assert!(!scene.state(&a).unwrap().pressed());
    }
    input.pointer_press(&mut scene, None);
    input.pointer_move(&mut scene, Some(&a));
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_none()
    );
}

#[test]
fn cancel_keeps_latches_until_release_but_focus_reset_forgets_them() {
    let (mut scene, a, _) = scene();
    let mut input = ActivationState::default();
    input.pointer_press(&mut scene, Some(&a));
    assert!(input.cancel(&mut scene));
    assert!(input.pointer_down() && input.captured().is_none());
    input.pointer_press(&mut scene, Some(&a));
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_none()
    );
    scene.focus(&a);
    key(&mut input, &mut scene, ActivationKey::Space, true);
    assert!(input.cancel(&mut scene));
    assert!(!key(&mut input, &mut scene, ActivationKey::Space, true).changed);
    assert!(
        key(&mut input, &mut scene, ActivationKey::Space, false)
            .activated
            .is_none()
    );
    key(&mut input, &mut scene, ActivationKey::Enter, true);
    assert!(input.reset(&mut scene));
    assert!(key(&mut input, &mut scene, ActivationKey::Enter, true).changed);
    assert!(
        key(&mut input, &mut scene, ActivationKey::Enter, false)
            .activated
            .is_some()
    );
}

#[test]
fn keyboard_requires_a_real_pair_on_the_original_focus_and_ignores_repeats() {
    let (mut scene, a, b) = scene();
    let mut input = ActivationState::default();
    scene.focus(&a);
    for activation_key in [ActivationKey::Enter, ActivationKey::Space] {
        assert!(
            key(&mut input, &mut scene, activation_key, false)
                .activated
                .is_none()
        );
        assert!(
            !input
                .key(&mut scene, activation_key, true, false, true, Some(&a))
                .changed
        );
        assert!(key(&mut input, &mut scene, activation_key, true).changed);
        assert!(
            !input
                .key(&mut scene, activation_key, true, true, false, Some(&a))
                .changed
        );
        assert!(
            !input
                .key(&mut scene, activation_key, false, false, true, Some(&a))
                .changed
        );
        assert!(scene.state(&a).unwrap().pressed());
        assert_eq!(
            key(&mut input, &mut scene, activation_key, false)
                .activated
                .unwrap()
                .source,
            ActivationSource::Keyboard(activation_key)
        );
    }
    key(&mut input, &mut scene, ActivationKey::Enter, true);
    scene.focus(&b);
    assert!(input.reconcile(&mut scene));
    scene.focus(&a);
    assert!(
        key(&mut input, &mut scene, ActivationKey::Enter, false)
            .activated
            .is_none()
    );
    // A repeat received after reset cannot invent the missing initial press.
    input.reset(&mut scene);
    input.key(
        &mut scene,
        ActivationKey::Space,
        true,
        true,
        false,
        Some(&a),
    );
    assert!(
        key(&mut input, &mut scene, ActivationKey::Space, false)
            .activated
            .is_none()
    );
}

#[test]
fn concurrent_keys_and_pointer_do_not_create_overlapping_activations() {
    let (mut scene, a, _) = scene();
    let mut input = ActivationState::default();
    scene.focus(&a);
    key(&mut input, &mut scene, ActivationKey::Enter, true);
    key(&mut input, &mut scene, ActivationKey::Space, true);
    assert!(
        key(&mut input, &mut scene, ActivationKey::Enter, false)
            .activated
            .is_some()
    );
    assert!(
        key(&mut input, &mut scene, ActivationKey::Space, false)
            .activated
            .is_none()
    );
    key(&mut input, &mut scene, ActivationKey::Enter, true);
    input.pointer_press(&mut scene, Some(&a));
    assert!(
        key(&mut input, &mut scene, ActivationKey::Enter, false)
            .activated
            .is_none()
    );
    key(&mut input, &mut scene, ActivationKey::Space, true);
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_some()
    );
    assert!(
        key(&mut input, &mut scene, ActivationKey::Space, false)
            .activated
            .is_none()
    );
}

#[test]
fn retained_keyed_reorder_keeps_capture_but_replacement_does_not() {
    let (mut scene, a, _) = scene();
    let mut input = ActivationState::default();
    input.pointer_press(&mut scene, Some(&a));
    scene
        .update(|ui| {
            ui.button("b", "B");
            ui.button("a", "Renamed");
        })
        .unwrap();
    assert!(!input.reconcile(&mut scene));
    assert!(scene.state(&a).unwrap().pressed());
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_some()
    );
    input.pointer_press(&mut scene, Some(&a));
    scene.update(|_| {}).unwrap();
    scene
        .update(|ui| {
            ui.button("a", "New incarnation");
        })
        .unwrap();
    assert!(
        input
            .pointer_release(&mut scene, Some(&a))
            .activated
            .is_none()
    );
    assert!(!scene.state(&a).unwrap().pressed());
    scene.focus(&a);
    key(&mut input, &mut scene, ActivationKey::Enter, true);
    scene.update(|_| {}).unwrap();
    scene
        .update(|ui| {
            ui.button("a", "Another incarnation");
        })
        .unwrap();
    scene.focus(&a);
    assert!(
        key(&mut input, &mut scene, ActivationKey::Enter, false)
            .activated
            .is_none()
    );
}

#[test]
fn foreign_scene_cannot_receive_capture_even_with_matching_ids_and_entities() {
    let (mut original, a, _) = scene();
    let (mut foreign, same_id, _) = scene();
    assert_eq!(a, same_id);
    assert_eq!(original.entities[&a], foreign.entities[&same_id]);
    let mut input = ActivationState::default();
    input.pointer_press(&mut original, Some(&a));
    assert!(
        input
            .pointer_release(&mut foreign, Some(&same_id))
            .activated
            .is_none()
    );
    assert!(!foreign.state(&same_id).unwrap().pressed());
    // The caller must reset using the owning scene before switching/dropping it.
    assert!(original.state(&a).unwrap().pressed());
}
