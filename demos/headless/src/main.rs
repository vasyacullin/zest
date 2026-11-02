use zest::Scene;

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
    println!("Headless demo: run zest-quad for the experimental window/GPU slice.");
    Ok(())
}
