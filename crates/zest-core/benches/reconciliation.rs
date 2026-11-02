//! Dependency-free baseline for complete scene and partial subtree declarations.
//! Includes builder allocation, ID construction, validation and ECS mutation.
use std::{
    hint::black_box,
    time::{Duration, Instant},
};
use zest_core::{Scene, Ui, UpdateStats, WidgetId};

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Unchanged,
    Reorder,
    OneLabel,
    InsertRemove,
}

fn declare(ui: &mut Ui, keys: &[String], scenario: Scenario, alternate: bool) -> WidgetId {
    let mut retained = None;
    ui.container("list", |ui| {
        for index in 0..keys.len() {
            let index = if matches!(scenario, Scenario::Reorder) && alternate {
                keys.len() - 1 - index
            } else {
                index
            };
            let label = if matches!(scenario, Scenario::OneLabel) && alternate && index == 0 {
                "Changed"
            } else {
                "Item"
            };
            let id = ui.button(keys[index].as_str(), label);
            if index == 0 {
                retained = Some(id);
            }
        }
        if matches!(scenario, Scenario::InsertRemove) && alternate {
            ui.button("extra", "Extra");
        }
    });
    retained.expect("nonempty benchmark scene")
}

fn expected(scenario: Scenario, alternate: bool, nodes: usize) -> UpdateStats {
    match scenario {
        Scenario::Unchanged => UpdateStats::default(),
        Scenario::Reorder => UpdateStats {
            updated: nodes - nodes % 2,
            ..UpdateStats::default()
        },
        Scenario::OneLabel => UpdateStats {
            updated: 1,
            ..UpdateStats::default()
        },
        Scenario::InsertRemove => UpdateStats {
            created: usize::from(alternate),
            removed: usize::from(!alternate),
            updated: 0,
        },
    }
}

fn run(scenario: Scenario, nodes: usize, warmup: usize, samples: usize) {
    let keys: Vec<_> = (0..nodes).map(|index| format!("item-{index}")).collect();
    let mut scene = Scene::default();
    let mut retained = None;
    scene
        .update(|ui| retained = Some(declare(ui, &keys, scenario, false)))
        .unwrap();
    let retained = retained.unwrap();
    assert!(scene.focus(&retained));
    assert!(scene.set_hovered(&retained, true));
    let archetypes = scene.stats().archetypes;
    let mut timings = Vec::with_capacity(samples);
    for iteration in 0..warmup + samples {
        let alternate = iteration % 2 == 0;
        let start = Instant::now();
        let stats = scene
            .update(|ui| {
                black_box(declare(ui, &keys, scenario, alternate));
            })
            .unwrap();
        let elapsed = start.elapsed();
        // Validate outside the timed section. A speedup must preserve semantics.
        assert_eq!(stats, expected(scenario, alternate, nodes));
        assert_eq!(scene.focused(), Some(&retained));
        assert!(scene.state(&retained).unwrap().hovered());
        assert_eq!(scene.stats().archetypes, archetypes);
        let extra = usize::from(matches!(scenario, Scenario::InsertRemove) && alternate);
        assert_eq!(scene.stats().nodes, nodes + 1 + extra);
        black_box(&scene);
        if iteration >= warmup {
            timings.push(elapsed);
        }
    }
    report(&format!("{scenario:?}"), nodes, timings);
}

fn sidebar_scene(ui: &mut Ui, keys: &[String], label: &str) -> (WidgetId, WidgetId) {
    let retained = declare(ui, keys, Scenario::Unchanged, false);
    let sidebar = ui.container("sidebar", |ui| {
        ui.button("value", label);
    });
    (retained, sidebar)
}

fn run_sidebar(partial: bool, nodes: usize, warmup: usize, samples: usize) {
    let keys: Vec<_> = (0..nodes).map(|index| format!("item-{index}")).collect();
    let mut scene = Scene::default();
    let mut ids = None;
    scene
        .update(|ui| ids = Some(sidebar_scene(ui, &keys, "Original")))
        .unwrap();
    let (retained, sidebar) = ids.unwrap();
    assert!(scene.focus(&retained));
    assert!(scene.set_hovered(&retained, true));
    let roots = scene.roots().to_vec();
    let value = scene.children(&sidebar).unwrap()[0].clone();
    let archetypes = scene.stats().archetypes;
    let mut timings = Vec::with_capacity(samples);
    for iteration in 0..warmup + samples {
        let label = if iteration % 2 == 0 {
            "Changed"
        } else {
            "Original"
        };
        let start = Instant::now();
        let stats = if partial {
            scene.update_children(&sidebar, |ui| {
                black_box(ui.button("value", label));
            })
        } else {
            scene.update(|ui| {
                black_box(sidebar_scene(ui, &keys, label));
            })
        }
        .unwrap();
        let elapsed = start.elapsed();
        assert_eq!(
            stats,
            UpdateStats {
                updated: 1,
                ..UpdateStats::default()
            }
        );
        assert_eq!(scene.focused(), Some(&retained));
        assert!(scene.state(&retained).unwrap().hovered());
        assert_eq!(scene.stats().nodes, nodes + 3);
        assert_eq!(scene.stats().archetypes, archetypes);
        assert_eq!(scene.roots(), roots);
        assert_eq!(
            scene.children(&sidebar).unwrap(),
            std::slice::from_ref(&value)
        );
        assert_eq!(scene.label(&value), Some(label));
        black_box(&scene);
        if iteration >= warmup {
            timings.push(elapsed);
        }
    }
    report(
        if partial {
            "PartialSidebar"
        } else {
            "FullSidebar"
        },
        nodes,
        timings,
    );
}

fn report(name: &str, nodes: usize, mut timings: Vec<Duration>) {
    let samples = timings.len();
    timings.sort_unstable();
    // Nearest-rank percentiles. These are observations, not confidence intervals.
    let percentile = |rank: usize| {
        let index = (rank * samples).div_ceil(100) - 1;
        timings[index].as_secs_f64() * 1_000.0
    };
    println!(
        "{name}, list_buttons={nodes}, samples={samples}, p50={:.3}ms, p95={:.3}ms, p99={:.3}ms",
        percentile(50),
        percentile(95),
        percentile(99),
    );
}

fn main() {
    let mut smoke = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--smoke" | "--test" => smoke = true,
            "--bench" => {}
            _ => panic!("unknown argument {argument:?}; supported: --smoke"),
        }
    }
    let (sizes, warmup, samples): (&[usize], usize, usize) = if smoke {
        (&[100], 2, 4)
    } else {
        (&[1_000, 10_000], 10, 100)
    };
    println!(
        "Reconciliation; target={}-{}; warmup={warmup}; smoke={smoke}",
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    for &nodes in sizes {
        for scenario in [
            Scenario::Unchanged,
            Scenario::Reorder,
            Scenario::OneLabel,
            Scenario::InsertRemove,
        ] {
            run(scenario, nodes, warmup, samples);
        }
        run_sidebar(false, nodes, warmup, samples);
        run_sidebar(true, nodes, warmup, samples);
    }
}
