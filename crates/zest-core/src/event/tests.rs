use super::*;
struct Fixture {
    scene: Scene,
    root: WidgetId,
    panel: WidgetId,
    target: WidgetId,
    outside: WidgetId,
}
fn fixture() -> Fixture {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.push_id("window", |ui| {
                ui.container("root", |ui| {
                    ui.push_id("component", |ui| {
                        ui.container("panel", |ui| {
                            ui.button("target", "Target");
                        });
                    });
                    ui.button("outside", "Outside");
                });
            });
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let panel = scene.children(&root).unwrap()[0].clone();
    let outside = scene.children(&root).unwrap()[1].clone();
    let target = scene.children(&panel).unwrap()[0].clone();
    Fixture {
        scene,
        root,
        panel,
        target,
        outside,
    }
}

#[test]
fn capture_target_bubble_order_omits_identity_scopes_and_preserves_scene_state() {
    let mut f = fixture();
    f.scene.focus(&f.target);
    f.scene.set_hovered(&f.target, true);
    let state = f.scene.state(&f.target);
    let archetypes = f.scene.stats().archetypes;
    let mut trace = Vec::new();
    let result = f
        .scene
        .dispatch_event(&f.target, &mut trace, |ctx, trace| {
            assert_eq!(ctx.target, &f.target);
            assert!(f.scene.label(ctx.current_target).is_some());
            trace.push((ctx.current_target.clone(), ctx.phase));
            EventControl::Continue
        })
        .unwrap();
    assert_eq!(
        trace,
        vec![
            (f.root.clone(), EventPhase::Capture),
            (f.panel.clone(), EventPhase::Capture),
            (f.target.clone(), EventPhase::Target),
            (f.panel.clone(), EventPhase::Bubble),
            (f.root.clone(), EventPhase::Bubble)
        ]
    );
    assert_eq!(
        result,
        DispatchResult {
            visits: 5,
            target_reached: true,
            ..Default::default()
        }
    );
    assert_eq!(f.scene.state(&f.target), state);
    assert_eq!(f.scene.focused(), Some(&f.target));
    assert_eq!(f.scene.stats().archetypes, archetypes);
}

#[test]
fn propagation_can_stop_in_each_phase_without_implicitly_preventing_default() {
    let f = fixture();
    for (phase, expected, reached) in [
        (EventPhase::Capture, 1, false),
        (EventPhase::Target, 3, true),
        (EventPhase::Bubble, 4, true),
    ] {
        let mut visits = 0;
        let result = f
            .scene
            .dispatch_event(&f.target, &mut visits, |ctx, visits| {
                *visits += 1;
                if ctx.phase == phase {
                    EventControl::StopPropagation
                } else {
                    EventControl::Continue
                }
            })
            .unwrap();
        assert_eq!(visits, expected);
        assert_eq!(result.visits, expected);
        assert_eq!(result.target_reached, reached);
        assert!(result.propagation_stopped && !result.default_prevented);
    }
}

#[test]
fn prevention_is_monotonic_and_consuming_capture_prevents_target_delivery_and_default() {
    let f = fixture();
    let mut seen = Vec::new();
    let result = f
        .scene
        .dispatch_event(&f.target, &mut seen, |ctx, seen| {
            seen.push(ctx.default_prevented);
            if ctx.current_target == &f.root && ctx.phase == EventPhase::Capture {
                EventControl::PreventDefault
            } else {
                EventControl::Continue
            }
        })
        .unwrap();
    assert_eq!(seen, [false, true, true, true, true]);
    assert!(result.default_prevented && result.target_reached && !result.propagation_stopped);
    let consumed = f
        .scene
        .dispatch_event(&f.target, &mut (), |_, _| EventControl::Consume)
        .unwrap();
    assert_eq!(
        consumed,
        DispatchResult {
            visits: 1,
            target_reached: false,
            propagation_stopped: true,
            default_prevented: true
        }
    );
}

#[test]
fn focused_and_single_root_dispatch_use_the_current_target_once() {
    let mut f = fixture();
    assert!(
        f.scene
            .dispatch_focused_event(&mut (), |_, _| panic!("no focus"))
            .is_none()
    );
    f.scene.focus(&f.target);
    let result = f
        .scene
        .dispatch_focused_event(&mut (), |ctx, _| {
            assert_eq!(ctx.target, &f.target);
            EventControl::Continue
        })
        .unwrap();
    assert!(result.target_reached);
    let root = f
        .scene
        .dispatch_event(&f.root, &mut (), |ctx, _| {
            assert_eq!(ctx.phase, EventPhase::Target);
            EventControl::Continue
        })
        .unwrap();
    assert_eq!(root.visits, 1);
}

#[test]
fn modal_root_is_an_inclusive_boundary_and_blocked_targets_have_no_delivery() {
    let mut f = fixture();
    f.scene.push_modal(&f.panel);
    let mut trace = Vec::new();
    let result = f
        .scene
        .dispatch_event(&f.target, &mut trace, |ctx, trace| {
            trace.push((ctx.current_target.clone(), ctx.phase));
            EventControl::Continue
        })
        .unwrap();
    assert_eq!(result.visits, 3);
    assert!(trace.iter().all(|(id, _)| id != &f.root));
    assert!(
        f.scene
            .dispatch_event(&f.outside, &mut (), |_, _| panic!("blocked"))
            .is_none()
    );
    f.scene.push_modal(&f.target);
    let result = f
        .scene
        .dispatch_event(&f.target, &mut (), |ctx, _| {
            assert_eq!(ctx.phase, EventPhase::Target);
            EventControl::Continue
        })
        .unwrap();
    assert_eq!(result.visits, 1);
}

#[test]
fn disabled_and_unknown_targets_leave_payloads_and_scene_unchanged() {
    let mut f = fixture();
    f.scene.set_enabled(&f.panel, false);
    let mut payload = 0;
    assert!(
        f.scene
            .dispatch_event(&f.target, &mut payload, |_, value| {
                *value += 1;
                EventControl::Continue
            })
            .is_none()
    );
    let mut other = Scene::default();
    other
        .update(|ui| {
            ui.button("missing", "Missing");
        })
        .unwrap();
    assert!(
        f.scene
            .dispatch_event(&other.roots()[0], &mut payload, |_, value| {
                *value += 1;
                EventControl::Continue
            })
            .is_none()
    );
    assert_eq!(payload, 0);
}

#[test]
fn dispatch_uses_current_reconciliation_and_does_not_cache_removed_or_recreated_routes() {
    let mut f = fixture();
    let old_entity = f.scene.entities[&f.target];
    f.scene
        .update_children(&f.panel, |ui| {
            ui.button("new", "New sibling");
            ui.button("target", "Renamed");
        })
        .unwrap();
    f.scene
        .dispatch_event(&f.target, &mut (), |ctx, _| {
            if ctx.phase == EventPhase::Target {
                assert_eq!(f.scene.label(ctx.current_target), Some("Renamed"));
            }
            EventControl::Continue
        })
        .unwrap();
    f.scene.update_children(&f.panel, |_| {}).unwrap();
    assert!(
        f.scene
            .dispatch_event(&f.target, &mut (), |_, _| panic!("removed"))
            .is_none()
    );
    f.scene
        .update_children(&f.panel, |ui| {
            ui.button("target", "Recreated");
        })
        .unwrap();
    assert_ne!(old_entity, f.scene.entities[&f.target]);
    assert!(
        f.scene
            .dispatch_event(&f.target, &mut (), |_, _| EventControl::Continue)
            .unwrap()
            .target_reached
    );
}

#[test]
fn nested_dispatches_keep_default_and_propagation_control_local() {
    let f = fixture();
    let mut nested = false;
    let result = f
        .scene
        .dispatch_event(&f.target, &mut (), |ctx, _| {
            if !nested && ctx.phase == EventPhase::Capture {
                nested = true;
                let inner = f
                    .scene
                    .dispatch_event(&f.outside, &mut (), |_, _| EventControl::Consume)
                    .unwrap();
                assert!(inner.default_prevented && inner.propagation_stopped);
            }
            EventControl::Continue
        })
        .unwrap();
    assert!(
        nested && result.target_reached && !result.default_prevented && !result.propagation_stopped
    );
}
