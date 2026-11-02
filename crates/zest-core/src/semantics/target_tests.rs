use super::*;
use crate::{
    commands::{CommandStats, SceneCommands},
    event::{EventHandlers, EventPhase},
    input::ActivationSource,
    semantics::SemanticRole,
};

fn fixture() -> (Scene, WidgetId, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("panel", |ui| {
                ui.button("save", "Save");
            });
            ui.button("other", "Other");
        })
        .unwrap();
    let panel = scene.roots()[0].clone();
    let save = scene.children(&panel).unwrap()[0].clone();
    let other = scene.roots()[1].clone();
    (scene, panel, save, other)
}

#[test]
fn unique_targets_focus_and_route_typed_events_without_emulating_gestures() {
    let (mut scene, panel, save, _) = fixture();
    let snapshot = scene.semantic_snapshot();
    let target = snapshot
        .find_target(SemanticQuery::new().named("Save"))
        .unwrap();
    assert_eq!(target.id(), &save);
    assert!(target.is_live(&scene) && target.input_allowed(&scene));
    assert!(target.focus(&mut scene));
    let mut event = (ActivationSource::Semantic, Vec::new());
    let result = target
        .dispatch_event(&scene, &mut event, |ctx, event| {
            assert_eq!(event.0, ActivationSource::Semantic);
            event.1.push((ctx.current_target.clone(), ctx.phase));
            EventControl::Continue
        })
        .unwrap();
    assert_eq!(
        event.1,
        [
            (panel.clone(), EventPhase::Capture),
            (save.clone(), EventPhase::Target),
            (panel, EventPhase::Bubble)
        ]
    );
    assert!(result.target_reached && !result.default_prevented);
    assert!(!scene.state(&save).unwrap().pressed());
}

#[test]
fn stale_snapshot_bindings_cannot_focus_or_invoke_even_a_new_instances_handler() {
    let (mut scene, panel, save, other) = fixture();
    let old = scene.semantic_snapshot();
    scene.update_children(&panel, |_| {}).unwrap();
    scene
        .update_children(&panel, |ui| {
            ui.button("save", "Save");
        })
        .unwrap();
    scene.focus(&other);
    let stale = old.find_target(SemanticQuery::new().named("Save")).unwrap();
    assert!(!stale.is_live(&scene) && !stale.input_allowed(&scene));
    assert!(!stale.focus(&mut scene));
    let mut payload = 0;
    assert!(
        stale
            .dispatch_event(&scene, &mut payload, |_, value| {
                *value += 1;
                EventControl::Continue
            })
            .is_none()
    );
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &save,
        EventPhase::Target,
        |_, event: &mut usize, calls: &mut usize| {
            *event += 1;
            *calls += 1;
            EventControl::Continue
        },
    );
    let mut calls = 0;
    assert!(
        handlers
            .dispatch_target(&scene, &stale, &mut payload, &mut calls)
            .is_none()
    );
    assert_eq!((payload, calls), (0, 0));
    assert_eq!(scene.focused(), Some(&other));
    let fresh = scene.semantic_snapshot().target(&save).unwrap();
    assert_ne!(fresh, stale);
    assert!(
        handlers
            .dispatch_target(&scene, &fresh, &mut payload, &mut calls)
            .unwrap()
            .target_reached
    );
    assert_eq!((payload, calls), (1, 1));
}

#[test]
fn current_eligibility_is_checked_instead_of_old_snapshot_state() {
    let (mut scene, _, save, other) = fixture();
    let target = scene.semantic_snapshot().target(&save).unwrap();
    scene.focus(&other);
    scene.set_enabled(&save, false);
    assert!(target.is_live(&scene) && !target.input_allowed(&scene));
    assert!(!target.focus(&mut scene));
    assert!(
        target
            .dispatch_event(&scene, &mut (), |_, _| panic!("disabled"))
            .is_none()
    );
    assert_eq!(scene.focused(), Some(&other));
    scene.set_enabled(&save, true);
    scene.push_modal(&other);
    assert!(!target.input_allowed(&scene));
    assert!(!target.focus(&mut scene));
    assert!(
        target
            .dispatch_event(&scene, &mut (), |_, _| panic!("blocked"))
            .is_none()
    );
    scene.pop_modal();
    assert!(target.input_allowed(&scene) && target.focus(&mut scene));
}

#[test]
fn registered_dispatch_keeps_prevention_and_modal_boundaries() {
    let (mut scene, panel, save, _) = fixture();
    let target = scene.semantic_snapshot().target(&save).unwrap();
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &panel,
        EventPhase::Capture,
        |_, _: &mut (), calls: &mut usize| {
            *calls += 1;
            EventControl::Consume
        },
    );
    handlers.set(&scene, &save, EventPhase::Target, |_, _, calls| {
        *calls += 10;
        EventControl::Continue
    });
    let mut calls = 0;
    let consumed = handlers
        .dispatch_target(&scene, &target, &mut (), &mut calls)
        .unwrap();
    assert!(consumed.default_prevented && !consumed.target_reached);
    assert_eq!(calls, 1);
    scene.push_modal(&save);
    let scoped = handlers
        .dispatch_target(&scene, &target, &mut (), &mut calls)
        .unwrap();
    assert!(scoped.target_reached && !scoped.default_prevented);
    assert_eq!(scoped.visits, 1);
    assert_eq!(calls, 11);
}

#[test]
fn retained_targets_survive_reorder_rename_and_failed_reconciliation_without_rerunning_queries() {
    let (mut scene, panel, save, _) = fixture();
    let target = scene
        .semantic_snapshot()
        .find_target(SemanticQuery::new().named("Save"))
        .unwrap();
    scene
        .update_children(&panel, |ui| {
            ui.button("new", "New");
            ui.button("save", "Renamed");
        })
        .unwrap();
    assert!(
        scene
            .update_children(&panel, |ui| {
                ui.button("save", "A");
                ui.button("save", "B");
            })
            .is_err()
    );
    assert!(target.is_live(&scene));
    assert_eq!(target, scene.semantic_snapshot().target(&save).unwrap());
    assert_eq!(
        scene
            .semantic_snapshot()
            .find_target(SemanticQuery::new().named("Save")),
        Err(FindError::NotFound)
    );
    let result = target
        .dispatch_event(&scene, &mut (), |ctx, _| {
            if ctx.phase == EventPhase::Target {
                assert_eq!(ctx.scene().label(ctx.target), Some("Renamed"));
            }
            EventControl::Continue
        })
        .unwrap();
    assert!(result.target_reached && target.focus(&mut scene));
}

#[test]
fn foreign_scenes_with_identical_paths_cannot_use_bindings_or_append_focus_commands() {
    let (scene, _, save, _) = fixture();
    let (mut other, _, same_id, other_focus) = fixture();
    let target = scene.semantic_snapshot().target(&save).unwrap();
    assert_eq!(&save, &same_id);
    other.focus(&other_focus);
    assert!(!target.is_live(&other) && !target.focus(&mut other));
    assert!(
        target
            .dispatch_event(&other, &mut (), |_, _| panic!("foreign"))
            .is_none()
    );
    let mut queue = SceneCommands::new(&other);
    assert!(!queue.focus_target(&other, &target));
    assert!(!queue.focus_target(&scene, &target));
    assert!(queue.is_empty());
    let mut handlers = EventHandlers::<(), ()>::new(&other);
    assert!(
        handlers
            .dispatch_target(&other, &target, &mut (), &mut ())
            .is_none()
    );
    assert_eq!(other.focused(), Some(&other_focus));
}

#[test]
fn queued_focus_validates_the_observed_instance_at_binding_and_application() {
    let (mut scene, panel, save, other) = fixture();
    scene.set_enabled(&save, false);
    let target = scene.semantic_snapshot().target(&save).unwrap();
    let mut queue = SceneCommands::new(&scene);
    queue.set_enabled(&scene, &save, true);
    assert!(queue.focus_target(&scene, &target));
    assert_eq!(queue.apply(&mut scene).unwrap().applied, 2);
    assert_eq!(scene.focused(), Some(&save));
    assert!(queue.focus_target(&scene, &target));
    scene.update_children(&panel, |_| {}).unwrap();
    scene
        .update_children(&panel, |ui| {
            ui.button("save", "Save");
        })
        .unwrap();
    scene.focus(&other);
    assert!(!queue.focus_target(&scene, &target));
    assert_eq!(queue.len(), 1);
    assert_eq!(
        queue.apply(&mut scene),
        Some(CommandStats {
            applied: 0,
            skipped: 1,
            changed: false
        })
    );
    assert_eq!(scene.focused(), Some(&other));
}

#[test]
fn binding_lookup_diagnoses_ambiguity_and_targets_expose_no_native_stamps() {
    let (scene, _, save, other) = fixture();
    let snapshot = scene.semantic_snapshot();
    assert_eq!(
        snapshot.find_target(SemanticQuery::new().role(SemanticRole::Button)),
        Err(FindError::Ambiguous(vec![save.clone(), other]))
    );
    let mut unknown = Scene::default();
    unknown
        .update(|ui| {
            ui.button("missing", "Missing");
        })
        .unwrap();
    assert!(snapshot.target(&unknown.roots()[0]).is_none());
    let target = snapshot.target(&save).unwrap();
    let mut targets = std::collections::HashSet::new();
    targets.insert(target.clone());
    targets.insert(snapshot.target(&save).unwrap());
    assert_eq!(targets.len(), 1);
    let debug = format!("{target:?}");
    assert!(!debug.contains("Entity(") && !debug.contains("WorldId"));
}
