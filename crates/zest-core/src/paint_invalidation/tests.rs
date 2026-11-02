use super::*;
use crate::{
    event::{EventControl, EventHandlers, EventPhase},
    input::ActivationState,
};

fn fixture() -> (Scene, WidgetId, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.push_id("window", |ui| {
                ui.container("panel", |ui| {
                    ui.button("one", "One");
                    ui.button("two", "Two");
                });
            });
        })
        .unwrap();
    let panel = scene.roots()[0].clone();
    let one = scene.children(&panel).unwrap()[0].clone();
    let two = scene.children(&panel).unwrap()[1].clone();
    (scene, panel, one, two)
}

#[test]
fn new_and_unchanged_empty_scenes_have_no_work() {
    let mut scene = Scene::default();
    assert!(!scene.needs_paint());
    scene.update(|_| {}).unwrap();
    assert!(scene.take_paint_invalidation().is_empty());
    assert_eq!(scene.paint_invalidation(), PaintInvalidation::default());
}

#[test]
fn reads_are_non_consuming_and_drains_preserve_order_state_and_archetypes() {
    let (mut scene, panel, one, two) = fixture();
    let work = scene.paint_invalidation();
    assert_eq!(work.widgets, [panel, one.clone(), two]);
    assert!(work.structure_changed && !work.is_empty());
    assert_eq!(scene.paint_invalidation(), work);
    scene.focus(&one);
    scene.set_hovered(&one, true);
    let mut activation = ActivationState::default();
    activation.pointer_press(&mut scene, Some(&one));
    let archetypes = scene.stats().archetypes;
    assert_eq!(scene.take_paint_invalidation(), work);
    let state = scene.state(&one).unwrap();
    assert!(state.focused() && state.hovered() && state.pressed() && !state.paint_dirty());
    assert!(!scene.needs_paint() && scene.take_paint_invalidation().is_empty());
    assert!(
        activation
            .pointer_release(&mut scene, Some(&one))
            .activated
            .is_some()
    );
    assert_eq!(scene.take_paint_invalidation().widgets, [one]);
    assert_eq!(scene.stats().archetypes, archetypes);
}

#[test]
fn repeated_interaction_noops_stay_clean_and_changes_coalesce() {
    let (mut scene, _, one, two) = fixture();
    scene.focus(&one);
    scene.set_hovered(&one, true);
    scene.take_paint_invalidation();
    scene.focus(&one);
    scene.set_hovered(&one, true);
    scene.set_enabled(&one, true);
    assert!(!scene.needs_paint());
    scene.focus(&two);
    scene.set_hovered(&one, false);
    scene.set_hovered(&one, true);
    let work = scene.take_paint_invalidation();
    assert_eq!(work.widgets, [one, two]);
    assert!(!work.structure_changed);
}

#[test]
fn removal_of_the_last_root_retains_work_without_a_surviving_dirty_node() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.button("only", "Only");
        })
        .unwrap();
    let removed = scene.roots()[0].clone();
    scene.take_paint_invalidation();
    scene.update(|_| {}).unwrap();
    assert!(scene.needs_paint());
    let work = scene.take_paint_invalidation();
    assert_eq!(
        work,
        PaintInvalidation {
            widgets: vec![],
            structure_changed: true
        }
    );
    assert!(!work.is_empty() && !scene.needs_paint());
    assert!(!scene.invalidate_paint(&removed));
}

#[test]
fn partial_reorders_and_deletion_mark_structure_but_labels_do_not() {
    let (mut scene, panel, one, two) = fixture();
    scene.take_paint_invalidation();
    scene
        .update_children(&panel, |ui| {
            ui.button("one", "One");
            ui.button("two", "Two");
        })
        .unwrap();
    assert!(!scene.needs_paint());
    scene
        .update_children(&panel, |ui| {
            ui.button("one", "Renamed");
            ui.button("two", "Two");
        })
        .unwrap();
    let work = scene.take_paint_invalidation();
    assert_eq!(work.widgets, std::slice::from_ref(&one));
    assert!(!work.structure_changed);
    scene
        .update_children(&panel, |ui| {
            ui.button("two", "Two");
            ui.button("one", "Renamed");
        })
        .unwrap();
    let work = scene.take_paint_invalidation();
    assert_eq!(work.widgets, [panel.clone(), two, one]);
    assert!(work.structure_changed);
    scene.update_children(&panel, |_| {}).unwrap();
    assert_eq!(
        scene.take_paint_invalidation(),
        PaintInvalidation {
            widgets: vec![panel],
            structure_changed: true
        }
    );
}

#[test]
fn invalid_and_panicking_declarations_preserve_existing_work() {
    let (mut scene, panel, _, _) = fixture();
    scene.take_paint_invalidation();
    for dirty in [false, true] {
        if dirty {
            scene.invalidate_paint(&panel);
        }
        let before = scene.paint_invalidation();
        assert!(
            scene
                .update_children(&panel, |ui| {
                    ui.button("bad", "A");
                    ui.button("bad", "B");
                })
                .is_err()
        );
        assert_eq!(scene.paint_invalidation(), before);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = scene.update(|_| panic!("builder"));
        }));
        assert!(result.is_err());
        assert_eq!(scene.paint_invalidation(), before);
    }
}

#[test]
fn availability_and_modal_transitions_include_blocked_nodes() {
    let (mut scene, panel, one, two) = fixture();
    scene.take_paint_invalidation();
    scene.set_enabled(&panel, false);
    let work = scene.take_paint_invalidation();
    assert_eq!(work.widgets, [panel.clone(), one.clone(), two.clone()]);
    assert!(!work.structure_changed);
    assert!(scene.invalidate_paint(&one));
    assert_eq!(
        scene.take_paint_invalidation().widgets,
        std::slice::from_ref(&one)
    );
    scene.set_enabled(&panel, true);
    scene.take_paint_invalidation();
    scene.push_modal(&one);
    assert!(!scene.input_allowed(&two));
    assert_eq!(
        scene.take_paint_invalidation().widgets,
        [panel.clone(), one.clone(), two.clone()]
    );
    assert!(scene.invalidate_paint(&two));
    assert_eq!(
        scene.take_paint_invalidation().widgets,
        std::slice::from_ref(&two)
    );
    scene.pop_modal();
    assert_eq!(scene.take_paint_invalidation().widgets, [panel, one, two]);
}

#[test]
fn retained_batches_do_not_consume_later_changes_and_event_models_can_invalidate_explicitly() {
    let (mut scene, _, one, two) = fixture();
    let retained_work = scene.take_paint_invalidation();
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &one,
        EventPhase::Target,
        |_, _: &mut (), count: &mut usize| {
            *count += 1;
            EventControl::PreventDefault
        },
    );
    let mut count = 0;
    handlers
        .dispatch(&scene, &one, &mut (), &mut count)
        .unwrap();
    assert_eq!(count, 1);
    assert!(!scene.needs_paint());
    scene.invalidate_paint(&one);
    scene.invalidate_paint(&one);
    assert_eq!(scene.take_paint_invalidation().widgets, [one]);
    scene.invalidate_paint(&two);
    drop(retained_work); // Old CPU work is not an acknowledgement token.
    assert!(scene.needs_paint());
    assert_eq!(scene.take_paint_invalidation().widgets, [two]);
}
