use super::*;
use crate::{
    Ui,
    commands::SceneCommands,
    event::{EventControl, EventHandlers, EventPhase},
    input::ActivationState,
};

fn declare(ui: &mut Ui, name: &str) -> WidgetId {
    let id = ui.button("save", "💾");
    assert!(
        ui.set_semantics(
            &id,
            SemanticProperties::default()
                .named(name)
                .described("Writes a document")
                .test_id("save")
        )
    );
    id
}

#[test]
fn declared_names_descriptions_and_test_ids_are_independent_of_painted_labels() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            let root = ui.container("tools", |ui| {
                declare(ui, "Save document");
            });
            assert!(
                ui.set_semantics(
                    &root,
                    SemanticProperties::default()
                        .named("Document tools")
                        .test_id("toolbar")
                )
            );
        })
        .unwrap();
    let snapshot = scene.semantic_snapshot();
    let root = snapshot
        .find(SemanticQuery::new().test_id("toolbar"))
        .unwrap();
    assert_eq!(root.role, SemanticRole::Group);
    assert_eq!(root.name, "Document tools");
    let save = snapshot
        .find(
            SemanticQuery::new()
                .role(SemanticRole::Button)
                .test_id("save")
                .named("Save document"),
        )
        .unwrap();
    assert_eq!(save.description.as_deref(), Some("Writes a document"));
    assert_eq!(scene.label(&save.id), Some("💾"));
    assert_eq!(save.parent.as_ref(), Some(&root.id));
}

#[test]
fn metadata_only_updates_preserve_entity_focus_capture_handlers_commands_and_paint() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            declare(ui, "Save");
        })
        .unwrap();
    let id = scene.roots()[0].clone();
    let entity = scene.entities[&id];
    let archetypes = scene.stats().archetypes;
    scene.focus(&id);
    scene.set_hovered(&id, true);
    let mut input = ActivationState::default();
    input.pointer_press(&mut scene, Some(&id));
    let mut handlers = EventHandlers::new(&scene);
    handlers.set(
        &scene,
        &id,
        EventPhase::Target,
        |_, _: &mut (), calls: &mut usize| {
            *calls += 1;
            EventControl::Continue
        },
    );
    let mut commands = SceneCommands::new(&scene);
    commands.focus(&scene, &id);
    scene.take_paint_invalidation();
    let old = scene.semantic_snapshot();
    let stats = scene
        .update(|ui| {
            assert_eq!(declare(ui, "Сохранить документ"), id);
        })
        .unwrap();
    assert_eq!(
        stats,
        crate::UpdateStats {
            created: 0,
            updated: 1,
            removed: 0
        }
    );
    assert!(!scene.needs_paint());
    assert_eq!(scene.entities[&id], entity);
    assert_eq!(scene.stats().archetypes, archetypes);
    assert!(
        scene.state(&id).unwrap().focused()
            && scene.state(&id).unwrap().hovered()
            && scene.state(&id).unwrap().pressed()
    );
    assert_eq!(old.node(&id).unwrap().name, "Save");
    assert_eq!(
        scene.semantic_snapshot().node(&id).unwrap().name,
        "Сохранить документ"
    );
    let mut calls = 0;
    handlers.dispatch(&scene, &id, &mut (), &mut calls).unwrap();
    assert_eq!(calls, 1);
    assert_eq!(commands.apply(&mut scene).unwrap().applied, 1);
    assert!(
        input
            .pointer_release(&mut scene, Some(&id))
            .activated
            .is_some()
    );
    scene.take_paint_invalidation();
    assert_eq!(
        scene
            .update(|ui| {
                declare(ui, "Сохранить документ");
            })
            .unwrap(),
        crate::UpdateStats::default()
    );
    assert!(!scene.needs_paint());
}

#[test]
fn explicit_empty_values_differ_from_fallback_and_metadata_resets_when_omitted() {
    for explicit_default in [false, true] {
        let mut scene = Scene::default();
        scene
            .update(|ui| {
                let id = ui.button("save", "Visible");
                ui.set_semantics(
                    &id,
                    SemanticProperties::default()
                        .named("")
                        .described("")
                        .test_id(""),
                );
            })
            .unwrap();
        let old = scene.semantic_snapshot();
        let node = old
            .find(SemanticQuery::new().test_id("").named(""))
            .unwrap();
        let id = node.id.clone();
        assert_eq!(node.description.as_deref(), Some(""));
        scene.take_paint_invalidation();
        scene
            .update(|ui| {
                let id = ui.button("save", "Visible");
                if explicit_default {
                    ui.set_semantics(&id, SemanticProperties::default());
                }
            })
            .unwrap();
        assert!(!scene.needs_paint());
        let new = scene.semantic_snapshot();
        assert_eq!(new.node(&id).unwrap().name, "Visible");
        assert!(
            new.node(&id).unwrap().description.is_none()
                && new.node(&id).unwrap().test_id.is_none()
        );
        assert_eq!(
            new.find(SemanticQuery::new().test_id("")),
            Err(FindError::NotFound)
        );
        assert!(old.node(&id).unwrap().name.is_empty());
    }
}

#[test]
fn duplicate_test_ids_are_valid_and_scope_role_and_name_filters_disambiguate() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            for key in ["left", "right"] {
                ui.container(key, |ui| {
                    declare(ui, "Save");
                });
            }
        })
        .unwrap();
    let snapshot = scene.semantic_snapshot();
    let ids: Vec<_> = snapshot
        .find_all(SemanticQuery::new().test_id("save"))
        .iter()
        .map(|node| node.id.clone())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(
        snapshot.find(SemanticQuery::new().test_id("save")),
        Err(FindError::Ambiguous(ids.clone()))
    );
    let query = SemanticQuery::new()
        .test_id("save")
        .within(&snapshot.roots()[0])
        .role(SemanticRole::Button)
        .named("Save")
        .enabled(true)
        .input_allowed(true);
    assert_eq!(snapshot.find(query).unwrap().id, ids[0]);
    assert_eq!(snapshot.find(query.id(&ids[1])), Err(FindError::NotFound));
    assert_eq!(snapshot.find(query.named("💾")), Err(FindError::NotFound));
    assert_eq!(
        snapshot.find(SemanticQuery::new().test_id("Save")),
        Err(FindError::NotFound)
    );
}

#[test]
fn partial_updates_preserve_root_and_sibling_metadata_and_reset_redeclared_descendants() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            let root = ui.container("root", |ui| {
                declare(ui, "Save");
            });
            ui.set_semantics(
                &root,
                SemanticProperties::default()
                    .named("Tools")
                    .test_id("tools"),
            );
            let other = ui.button("other", "Other");
            ui.set_semantics(&other, SemanticProperties::default().test_id("other"));
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let other = scene.roots()[1].clone();
    scene.take_paint_invalidation();
    scene
        .update_children(&root, |ui| {
            assert!(!ui.set_semantics(&root, SemanticProperties::default().named("Invalid")));
            ui.button("save", "💾");
        })
        .unwrap();
    assert!(!scene.needs_paint());
    let snapshot = scene.semantic_snapshot();
    assert_eq!(snapshot.node(&root).unwrap().name, "Tools");
    assert_eq!(
        snapshot
            .find(SemanticQuery::new().test_id("other"))
            .unwrap()
            .id,
        other
    );
    assert_eq!(
        snapshot.find(SemanticQuery::new().test_id("save")),
        Err(FindError::NotFound)
    );
}

#[test]
fn invalid_or_panicking_declarations_do_not_publish_metadata_changes() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            declare(ui, "Save");
        })
        .unwrap();
    scene.take_paint_invalidation();
    let old = scene.semantic_snapshot();
    assert!(
        scene
            .update(|ui| {
                declare(ui, "Changed");
                declare(ui, "Duplicate");
            })
            .is_err()
    );
    assert_eq!(scene.semantic_snapshot().nodes(), old.nodes());
    assert!(!scene.needs_paint());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = scene.update(|ui| {
            declare(ui, "Changed");
            panic!("builder");
        });
    }));
    assert!(result.is_err());
    assert_eq!(scene.semantic_snapshot().nodes(), old.nodes());
    assert!(!scene.needs_paint());
}

#[test]
fn setter_requires_a_declaration_and_repeated_assignment_replaces_all_properties() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            declare(ui, "Save");
        })
        .unwrap();
    let id = scene.roots()[0].clone();
    scene
        .update(|ui| {
            assert!(!ui.set_semantics(
                &id,
                SemanticProperties::default().named("Before declaration")
            ));
            assert_eq!(ui.button("save", "💾"), id);
            ui.set_semantics(
                &id,
                SemanticProperties::default()
                    .test_id("first")
                    .described("Old"),
            );
            ui.set_semantics(&id, SemanticProperties::default().test_id("second"));
        })
        .unwrap();
    let snapshot = scene.semantic_snapshot();
    let node = snapshot
        .find(SemanticQuery::new().test_id("second"))
        .unwrap();
    assert_eq!(node.name, "💾");
    assert!(node.description.is_none());
    assert_eq!(
        snapshot.find(SemanticQuery::new().test_id("first")),
        Err(FindError::NotFound)
    );
}

#[test]
fn metadata_does_not_follow_a_removed_node_into_a_new_generation() {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            declare(ui, "Save");
        })
        .unwrap();
    let id = scene.roots()[0].clone();
    let old_entity = scene.entities[&id];
    let old = scene.semantic_snapshot();
    scene.update(|_| {}).unwrap();
    scene
        .update(|ui| {
            ui.button("save", "New");
        })
        .unwrap();
    assert_ne!(scene.entities[&id], old_entity);
    let new = scene.semantic_snapshot();
    assert_eq!(new.node(&id).unwrap().name, "New");
    assert!(new.node(&id).unwrap().test_id.is_none());
    assert_eq!(old.node(&id).unwrap().test_id.as_deref(), Some("save"));
}
