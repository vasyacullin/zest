use super::*;
use std::{cell::Cell, rc::Rc};

fn fixture() -> (Scene, WidgetId, WidgetId) {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("panel", |ui| {
                ui.button("save", "Save");
            });
        })
        .unwrap();
    let panel = scene.roots()[0].clone();
    let save = scene.children(&panel).unwrap()[0].clone();
    (scene, panel, save)
}

#[test]
fn registered_phases_mutate_typed_state_and_payload_in_route_order() {
    let (scene, panel, save) = fixture();
    let mut handlers = EventHandlers::new(&scene);
    for (id, phase) in [
        (&panel, EventPhase::Capture),
        (&save, EventPhase::Target),
        (&panel, EventPhase::Bubble),
    ] {
        assert!(handlers.set(
            &scene,
            id,
            phase,
            move |ctx, amount: &mut u32, trace: &mut Vec<(EventPhase, u32)>| {
                assert_eq!(ctx.phase, phase);
                *amount += 1;
                trace.push((ctx.phase, *amount));
                EventControl::Continue
            }
        ));
    }
    let mut amount = 0;
    let mut trace = Vec::new();
    let result = handlers
        .dispatch(&scene, &save, &mut amount, &mut trace)
        .unwrap();
    assert_eq!(
        trace,
        [
            (EventPhase::Capture, 1),
            (EventPhase::Target, 2),
            (EventPhase::Bubble, 3)
        ]
    );
    assert_eq!(result.visits, 3);
    assert!(result.target_reached && !result.default_prevented);
    assert_eq!(handlers.len(), 3);
}

#[test]
fn unhandled_phases_preserve_prevention_and_stopping_is_independent() {
    let (scene, panel, save) = fixture();
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &panel,
        EventPhase::Capture,
        |_, _: &mut (), _: &mut ()| EventControl::PreventDefault,
    );
    handlers.set(&scene, &panel, EventPhase::Bubble, |ctx, _, _| {
        assert!(ctx.default_prevented);
        EventControl::Continue
    });
    let result = handlers.dispatch(&scene, &save, &mut (), &mut ()).unwrap();
    assert!(result.target_reached && result.default_prevented && !result.propagation_stopped);
    for (control, prevented) in [
        (EventControl::StopPropagation, false),
        (EventControl::Consume, true),
    ] {
        handlers.set(&scene, &panel, EventPhase::Capture, move |_, _, _| control);
        let result = handlers.dispatch(&scene, &save, &mut (), &mut ()).unwrap();
        assert_eq!(result.visits, 1);
        assert!(!result.target_reached && result.propagation_stopped);
        assert_eq!(result.default_prevented, prevented);
    }
}

#[test]
fn keyed_reorders_and_failed_reconciliation_retain_bindings_but_recreation_does_not() {
    let (mut scene, panel, save) = fixture();
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &save,
        EventPhase::Target,
        |_, _: &mut (), calls: &mut usize| {
            *calls += 1;
            EventControl::Continue
        },
    );
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
    let mut calls = 0;
    handlers
        .dispatch(&scene, &save, &mut (), &mut calls)
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(handlers.prune(&scene), 0);
    scene.update_children(&panel, |_| {}).unwrap();
    assert!(
        handlers
            .dispatch(&scene, &save, &mut (), &mut calls)
            .is_none()
    );
    scene
        .update_children(&panel, |ui| {
            ui.button("save", "Recreated");
        })
        .unwrap();
    assert!(
        handlers
            .dispatch(&scene, &save, &mut (), &mut calls)
            .unwrap()
            .target_reached
    );
    assert_eq!(calls, 1);
    assert_eq!(handlers.prune(&scene), 1);
    assert!(handlers.is_empty());
    handlers.set(&scene, &save, EventPhase::Target, |_, _, calls| {
        *calls += 10;
        EventControl::Continue
    });
    handlers
        .dispatch(&scene, &save, &mut (), &mut calls)
        .unwrap();
    assert_eq!(calls, 11);
}

#[test]
fn foreign_scenes_cannot_bind_dispatch_or_prune_even_with_identical_ids() {
    let (scene, panel, save) = fixture();
    let (mut other, other_panel, other_save) = fixture();
    assert_eq!(save, other_save);
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &panel,
        EventPhase::Capture,
        |_, _: &mut (), calls: &mut usize| {
            *calls += 1;
            EventControl::Continue
        },
    );
    assert!(
        !handlers.set(&other, &other_panel, EventPhase::Capture, |_, _, _| panic!(
            "foreign"
        ))
    );
    let mut calls = 0;
    assert!(
        handlers
            .dispatch(&other, &other_save, &mut (), &mut calls)
            .is_none()
    );
    other.focus(&other_save);
    assert!(
        handlers
            .dispatch_focused(&other, &mut (), &mut calls)
            .is_none()
    );
    assert_eq!(handlers.prune(&other), 0);
    handlers
        .dispatch(&scene, &save, &mut (), &mut calls)
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(handlers.len(), 1);
}

#[test]
fn modal_and_disabled_nodes_retain_bindings_but_reject_blocked_delivery() {
    let (mut scene, panel, save) = fixture();
    let mut handlers = EventHandlers::new(&scene);
    scene.set_enabled(&panel, false);
    handlers.set(
        &scene,
        &panel,
        EventPhase::Capture,
        |_, _: &mut (), trace: &mut Vec<&str>| {
            trace.push("parent");
            EventControl::Consume
        },
    );
    handlers.set(&scene, &save, EventPhase::Target, |_, _, trace| {
        trace.push("save");
        EventControl::Continue
    });
    let mut trace = Vec::new();
    assert!(
        handlers
            .dispatch(&scene, &save, &mut (), &mut trace)
            .is_none()
    );
    assert!(trace.is_empty());
    assert_eq!(handlers.prune(&scene), 0);
    scene.set_enabled(&panel, true);
    scene.push_modal(&save);
    assert!(
        handlers
            .dispatch(&scene, &panel, &mut (), &mut trace)
            .is_none()
    );
    handlers
        .dispatch_focused(&scene, &mut (), &mut trace)
        .unwrap();
    assert_eq!(trace, ["save"]);
    scene.pop_modal();
    assert!(
        handlers
            .dispatch(&scene, &save, &mut (), &mut trace)
            .unwrap()
            .default_prevented
    );
    assert_eq!(trace, ["save", "parent"]);
}

#[test]
fn empty_unknown_and_unfocused_dispatch_do_not_call_handlers() {
    let (mut scene, panel, save) = fixture();
    let mut handlers = EventHandlers::<(), ()>::new(&scene);
    let result = handlers.dispatch(&scene, &save, &mut (), &mut ()).unwrap();
    assert_eq!(result.visits, 3);
    assert!(result.target_reached && !result.default_prevented);
    assert!(
        handlers
            .dispatch_focused(&scene, &mut (), &mut ())
            .is_none()
    );
    scene.update_children(&panel, |_| {}).unwrap();
    assert!(
        !handlers.set(&scene, &save, EventPhase::Target, |_, _, _| panic!(
            "removed"
        ))
    );
    assert!(handlers.is_empty());
}

#[test]
fn replacing_removing_clearing_and_pruning_drop_owned_captures() {
    struct Dropped(Rc<Cell<usize>>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let (mut scene, panel, save) = fixture();
    let mut handlers = EventHandlers::new(&scene);
    let drops = Rc::new(Cell::new(0));
    let callback = || {
        let owned = Dropped(drops.clone());
        move |_: EventContext<'_>, _: &mut (), _: &mut ()| {
            let _keep_alive = &owned;
            EventControl::Continue
        }
    };
    handlers.set(&scene, &save, EventPhase::Target, callback());
    handlers.set(&scene, &save, EventPhase::Target, callback());
    assert_eq!(handlers.len(), 1);
    assert_eq!(drops.get(), 1);
    assert!(handlers.remove(&save, EventPhase::Target));
    assert!(!handlers.remove(&save, EventPhase::Target));
    assert_eq!(drops.get(), 2);
    handlers.set(&scene, &save, EventPhase::Target, callback());
    scene.update_children(&panel, |_| {}).unwrap();
    assert_eq!(handlers.prune(&scene), 1);
    assert_eq!(drops.get(), 3);
    handlers.set(&scene, &panel, EventPhase::Target, callback());
    handlers.clear();
    assert!(handlers.is_empty());
    assert_eq!(drops.get(), 4);
}

#[test]
fn callbacks_and_registries_keep_their_own_mutable_state() {
    let (scene, _, save) = fixture();
    let mut first = EventHandlers::new(&scene);
    let mut second = EventHandlers::new(&scene);
    let mut owned_count = 0;
    first.set(
        &scene,
        &save,
        EventPhase::Target,
        move |_, _: &mut (), state: &mut usize| {
            owned_count += 1;
            *state = owned_count;
            EventControl::Continue
        },
    );
    second.set(
        &scene,
        &save,
        EventPhase::Target,
        |_, amount: &mut String, state: &mut String| {
            state.push_str(amount);
            EventControl::Continue
        },
    );
    let mut count = 0;
    first.dispatch(&scene, &save, &mut (), &mut count).unwrap();
    first.dispatch(&scene, &save, &mut (), &mut count).unwrap();
    assert_eq!(count, 2);
    let mut message = String::new();
    second
        .dispatch(&scene, &save, &mut "saved".into(), &mut message)
        .unwrap();
    assert_eq!(message, "saved");
}
