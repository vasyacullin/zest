use super::*;
use crate::{
    event::{EventControl, EventHandlers, EventPhase},
    input::ActivationState,
};

fn fixture() -> (Scene, WidgetId, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("panel", |ui| {
                ui.button("one", "One");
            });
            ui.button("outside", "Outside");
        })
        .unwrap();
    let panel = scene.roots()[0].clone();
    let one = scene.children(&panel).unwrap()[0].clone();
    let outside = scene.roots()[1].clone();
    (scene, panel, one, outside)
}

#[test]
fn ordered_availability_and_focus_commands_check_eligibility_when_applied() {
    let (mut scene, _, one, _) = fixture();
    let mut commands = SceneCommands::new(&scene);
    commands.focus(&scene, &one);
    commands.set_enabled(&scene, &one, false);
    commands.focus(&scene, &one);
    commands.set_enabled(&scene, &one, true);
    commands.focus(&scene, &one);
    assert!(scene.focused().is_none() && scene.state(&one).unwrap().enabled());
    assert_eq!(commands.len(), 5);
    assert_eq!(
        commands.apply(&mut scene),
        Some(CommandStats {
            applied: 4,
            skipped: 1,
            changed: true
        })
    );
    assert_eq!(scene.focused(), Some(&one));
    assert!(commands.is_empty());
    assert_eq!(commands.apply(&mut scene), Some(CommandStats::default()));
}

#[test]
fn handlers_read_the_routed_scene_and_queue_edits_without_changing_the_route() {
    let (mut scene, panel, one, _) = fixture();
    let mut commands = SceneCommands::new(&scene);
    let mut phases = Vec::new();
    let result = scene
        .dispatch_event(&one, &mut phases, |ctx, phases| {
            assert!(ctx.scene().state(&one).unwrap().enabled());
            assert!(ctx.scene().focused().is_none());
            phases.push(ctx.phase);
            if ctx.phase == EventPhase::Target {
                commands.focus(ctx.scene(), &panel);
                commands.set_enabled(ctx.scene(), ctx.target, false);
            }
            EventControl::Continue
        })
        .unwrap();
    assert_eq!(
        phases,
        [EventPhase::Capture, EventPhase::Target, EventPhase::Bubble]
    );
    assert!(result.target_reached && !result.default_prevented);
    assert!(commands.apply(&mut scene).unwrap().changed);
    assert_eq!(scene.focused(), Some(&panel));
    assert!(!scene.state(&one).unwrap().enabled());
}

#[test]
fn all_targeted_commands_skip_removed_and_recreated_generations() {
    let (mut scene, panel, one, _) = fixture();
    let mut commands = SceneCommands::new(&scene);
    commands.focus(&scene, &one);
    commands.set_enabled(&scene, &one, false);
    commands.push_modal(&scene, &one);
    commands.invalidate_paint(&scene, &one);
    scene.update_children(&panel, |_| {}).unwrap();
    scene
        .update_children(&panel, |ui| {
            ui.button("one", "Recreated");
        })
        .unwrap();
    scene.take_paint_invalidation();
    assert_eq!(
        commands.apply(&mut scene),
        Some(CommandStats {
            applied: 0,
            skipped: 4,
            changed: false
        })
    );
    assert!(scene.focused().is_none() && scene.modal_root().is_none() && !scene.needs_paint());
    assert!(scene.state(&one).unwrap().enabled());
    assert!(commands.is_empty());
}

#[test]
fn foreign_scenes_and_unknown_targets_do_not_consume_or_append_commands() {
    let (mut scene, panel, one, _) = fixture();
    let (mut other, _, other_one, _) = fixture();
    let mut commands = SceneCommands::new(&scene);
    commands.focus(&scene, &one);
    assert!(!commands.focus(&other, &other_one));
    assert!(!commands.set_enabled(&other, &one, false));
    assert!(!commands.push_modal(&other, &one));
    assert!(!commands.invalidate_paint(&other, &one));
    assert!(commands.apply(&mut other).is_none());
    assert_eq!(commands.len(), 1);
    assert_eq!(commands.apply(&mut scene).unwrap().applied, 1);
    scene.update_children(&panel, |_| {}).unwrap();
    assert!(!commands.focus(&scene, &one));
    assert!(!commands.set_enabled(&scene, &one, false));
    assert!(!commands.push_modal(&scene, &one));
    assert!(!commands.invalidate_paint(&scene, &one));
    assert!(commands.is_empty());
}

#[test]
fn modal_push_pop_and_focus_use_the_current_stack_in_command_order() {
    let (mut scene, panel, one, outside) = fixture();
    scene.focus(&outside);
    let mut commands = SceneCommands::new(&scene);
    commands.push_modal(&scene, &panel);
    commands.focus(&scene, &outside);
    commands.push_modal(&scene, &one);
    commands.push_modal(&scene, &outside);
    commands.pop_modal();
    commands.pop_modal();
    commands.pop_modal();
    assert_eq!(
        commands.apply(&mut scene),
        Some(CommandStats {
            applied: 4,
            skipped: 3,
            changed: true
        })
    );
    assert_eq!(scene.focused(), Some(&outside));
    assert!(scene.modal_root().is_none());
}

#[test]
fn accepted_noops_paint_requests_clear_and_discard_report_precise_results() {
    let (mut scene, _, one, _) = fixture();
    scene.take_paint_invalidation();
    let mut commands = SceneCommands::new(&scene);
    commands.clear_focus();
    commands.invalidate_paint(&scene, &one);
    commands.invalidate_paint(&scene, &one);
    assert_eq!(
        commands.apply(&mut scene),
        Some(CommandStats {
            applied: 3,
            skipped: 0,
            changed: true
        })
    );
    commands.clear_focus();
    commands.set_enabled(&scene, &one, true);
    commands.invalidate_paint(&scene, &one);
    assert_eq!(
        commands.apply(&mut scene),
        Some(CommandStats {
            applied: 3,
            skipped: 0,
            changed: false
        })
    );
    assert_eq!(
        scene.take_paint_invalidation().widgets,
        std::slice::from_ref(&one)
    );
    commands.set_enabled(&scene, &one, false);
    commands.clear();
    assert_eq!(commands.apply(&mut scene), Some(CommandStats::default()));
    assert!(scene.state(&one).unwrap().enabled());
}

#[test]
fn queued_availability_transitions_revoke_a_gesture_even_after_reenable() {
    let (mut scene, _, one, _) = fixture();
    let archetypes = scene.stats().archetypes;
    let mut input = ActivationState::default();
    input.pointer_press(&mut scene, Some(&one));
    let mut commands = SceneCommands::new(&scene);
    commands.set_enabled(&scene, &one, false);
    commands.set_enabled(&scene, &one, true);
    commands.apply(&mut scene).unwrap();
    input.reconcile(&mut scene);
    assert!(
        input
            .pointer_release(&mut scene, Some(&one))
            .activated
            .is_none()
    );
    assert!(!scene.state(&one).unwrap().pressed());
    assert_eq!(scene.stats().archetypes, archetypes);
}

#[test]
fn registered_handlers_can_own_a_queue_and_retained_targets_survive_reorders() {
    let (mut scene, panel, one, outside) = fixture();
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &one,
        EventPhase::Target,
        |ctx, _: &mut (), commands: &mut SceneCommands| {
            assert_eq!(ctx.scene().label(ctx.target), Some("Renamed"));
            commands.focus(ctx.scene(), ctx.target);
            commands.invalidate_paint(ctx.scene(), ctx.current_target);
            EventControl::PreventDefault
        },
    );
    let mut commands = SceneCommands::new(&scene);
    commands.focus(&scene, &outside);
    scene
        .update_children(&panel, |ui| {
            ui.button("new", "New");
            ui.button("one", "Renamed");
        })
        .unwrap();
    assert!(
        scene
            .update_children(&panel, |ui| {
                ui.button("one", "A");
                ui.button("one", "B");
            })
            .is_err()
    );
    scene.take_paint_invalidation();
    let result = handlers
        .dispatch(&scene, &one, &mut (), &mut commands)
        .unwrap();
    assert!(result.default_prevented && scene.focused().is_none());
    assert_eq!(commands.apply(&mut scene).unwrap().applied, 3);
    assert_eq!(scene.focused(), Some(&one));
}
