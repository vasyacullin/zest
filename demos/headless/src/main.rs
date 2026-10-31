use zest::Scene;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::default();
    for keys in [["one", "two"], ["two", "one"]] {
        let stats = scene.update(|ui| {
            ui.container("toolbar", |ui| {
                for key in keys {
                    ui.push_id(key, |ui| {
                        ui.button("action", format!("Action {key}"));
                    });
                }
            });
        })?;
        println!("Reconciled: {stats:?}; scene: {:?}", scene.stats());
    }
    println!("Headless foundation only: no window or GPU renderer yet.");
    Ok(())
}
