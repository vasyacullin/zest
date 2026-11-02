use zest::Scene;
use zest::commands::SceneCommands;
use zest::event::{EventControl, EventHandlers, EventPhase};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::default();
    let (mut toolbar, mut retained, mut status) = (None, None, None);
    for keys in [["one", "two"], ["two", "one"]] {
        let stats = scene.update(|ui| {
            toolbar = Some(ui.container("toolbar", |ui| {
                for key in keys {
                    ui.push_id(key, |ui| {
                        let id = ui.button("action", format!("Action {key}"));
                        if key == "one" {
                            retained = Some(id);
                        }
                    });
                }
            }));
            status = Some(ui.button("status", "Ready"));
        })?;
        println!("Reconciled: {stats:?}; scene: {:?}", scene.stats());
    }
    let retained = retained.unwrap();
    scene.focus(&retained);
    scene.set_hovered(&retained, true);
    let stats = scene.update_children(&toolbar.unwrap(), |ui| {
        ui.push_id("one", |ui| {
            assert_eq!(ui.button("action", "Updated action"), retained);
        });
    })?;
    assert_eq!(scene.focused(), Some(&retained));
    assert!(scene.state(&retained).unwrap().hovered());
    assert_eq!(scene.label(&status.unwrap()), Some("Ready"));
    println!("Partial update: {stats:?}; scene: {:?}", scene.stats());
    println!("Retained button keeps focus/hover; status sibling remains Ready.");
    let paint = scene.take_paint_invalidation();
    assert!(paint.structure_changed && !paint.is_empty());
    assert!(!scene.needs_paint());
    let mut phases = Vec::new();
    let dispatch = scene
        .dispatch_focused_event(&mut phases, |ctx, phases| {
            phases.push(ctx.phase);
            EventControl::Continue
        })
        .unwrap();
    assert_eq!(
        phases,
        [EventPhase::Capture, EventPhase::Target, EventPhase::Bubble]
    );
    assert!(dispatch.target_reached && !dispatch.default_prevented);
    println!("Focused event: {dispatch:?}; identity scopes do not receive events.");
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &retained,
        EventPhase::Target,
        |_, amount: &mut u32, total: &mut u32| {
            *total += *amount;
            EventControl::PreventDefault
        },
    );
    let mut total = 0;
    let dispatch = handlers
        .dispatch_focused(&scene, &mut 2, &mut total)
        .unwrap();
    assert_eq!(total, 2);
    assert!(dispatch.default_prevented);
    scene.invalidate_paint(&retained);
    assert_eq!(
        scene.take_paint_invalidation().widgets,
        std::slice::from_ref(&retained)
    );
    assert!(!scene.needs_paint());
    println!("Registered button handler updated the application total to {total}.");
    println!(
        "Application visual changes invalidate paint explicitly; draining leaves the scene clean."
    );
    let mut commands = SceneCommands::new(&scene);
    scene
        .dispatch_event(&retained, &mut (), |ctx, _| {
            if ctx.phase == EventPhase::Target {
                commands.set_enabled(ctx.scene(), ctx.target, false);
            }
            EventControl::Continue
        })
        .unwrap();
    assert!(scene.state(&retained).unwrap().enabled());
    let stats = commands.apply(&mut scene).unwrap();
    assert_eq!(stats.applied, 1);
    assert!(!scene.state(&retained).unwrap().enabled());
    println!("Deferred scene command applied after dispatch: {stats:?}.");
    println!("Headless demo: run zest-quad for the experimental window/GPU slice.");
    Ok(())
}
