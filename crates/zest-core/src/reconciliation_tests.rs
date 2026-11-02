use super::*;

fn check_hierarchy(scene: &Scene) {
    let mut visited = HashSet::new();
    let mut pending = vec![None];
    while let Some(parent) = pending.pop() {
        let children = match &parent {
            None => scene.roots(),
            Some(id) => scene.children(id).expect("existing container"),
        };
        for (order, id) in children.iter().enumerate() {
            assert!(visited.insert(id.clone()), "each node occurs exactly once");
            let node = scene.world.get::<Node>(scene.entities[id]).unwrap();
            assert_eq!(node.parent, parent);
            assert_eq!(node.order, order);
            if id.kind() == NodeKind::Container {
                assert!(scene.children.contains_key(&Some(id.clone())));
                pending.push(Some(id.clone()));
            } else {
                assert_eq!(scene.children(id), Some(&[][..]));
            }
        }
    }
    assert_eq!(visited.len(), scene.entities.len());
    for id in scene.children.keys().flatten() {
        assert!(visited.contains(id), "no stale container entries");
        assert_eq!(id.kind(), NodeKind::Container);
    }
}

struct Fixture {
    scene: Scene,
    outer: WidgetId,
    target: WidgetId,
    first: WidgetId,
    nested: WidgetId,
    leaf: WidgetId,
    sibling: WidgetId,
    other: WidgetId,
}

fn fixture() -> Fixture {
    let mut scene = Scene::default();
    let (mut outer, mut target, mut first, mut nested, mut leaf, mut sibling, mut other) =
        (None, None, None, None, None, None, None);
    scene
        .update(|ui| {
            ui.push_id("window", |ui| {
                outer = Some(ui.container("outer", |ui| {
                    ui.push_id("scope", |ui| {
                        target = Some(ui.container("tools", |ui| {
                            first = Some(ui.button("first", "First"));
                            nested = Some(ui.container("nested", |ui| {
                                ui.push_id("items", |ui| {
                                    leaf = Some(ui.button("leaf", "Leaf"));
                                });
                            }));
                        }));
                    });
                    sibling = Some(ui.button("status", "Ready"));
                }));
                other = Some(ui.container("other", |ui| {
                    ui.button("first", "Other branch");
                }));
            });
        })
        .unwrap();
    check_hierarchy(&scene);
    Fixture {
        scene,
        outer: outer.unwrap(),
        target: target.unwrap(),
        first: first.unwrap(),
        nested: nested.unwrap(),
        leaf: leaf.unwrap(),
        sibling: sibling.unwrap(),
        other: other.unwrap(),
    }
}

#[test]
fn scoped_update_preserves_ancestors_siblings_ids_and_interaction() {
    let mut f = fixture();
    let before = f.scene.entities.clone();
    let roots = f.scene.roots().to_vec();
    let outer_children = f.scene.children(&f.outer).unwrap().to_vec();
    let first_entity = before[&f.first];
    f.scene.focus(&f.first);
    f.scene.set_hovered(&f.sibling, true);
    let stats = f
        .scene
        .update_children(&f.target, |ui| {
            ui.container("nested", |ui| {
                ui.push_id("items", |ui| {
                    assert_eq!(ui.button("leaf", "Leaf"), f.leaf);
                });
            });
            assert_eq!(ui.button("first", "Renamed"), f.first);
        })
        .unwrap();
    assert_eq!(
        stats,
        UpdateStats {
            updated: 2,
            ..UpdateStats::default()
        }
    );
    assert_eq!(f.scene.entities, before);
    assert_eq!(f.scene.entities[&f.first], first_entity);
    assert_eq!(f.scene.focused(), Some(&f.first));
    assert!(f.scene.state(&f.first).unwrap().focused());
    assert!(f.scene.state(&f.sibling).unwrap().hovered());
    assert_eq!(f.scene.label(&f.sibling), Some("Ready"));
    assert_eq!(f.scene.label(&f.first), Some("Renamed"));
    assert_eq!(f.scene.roots(), roots);
    assert_eq!(f.scene.children(&f.outer).unwrap(), outer_children);
    assert_eq!(f.scene.children(&f.target).unwrap(), [f.nested, f.first]);
    check_hierarchy(&f.scene);
}

#[test]
fn empty_partial_update_removes_descendants_and_clears_their_focus() {
    let mut f = fixture();
    let entity = f.scene.entities[&f.leaf];
    f.scene.focus(&f.leaf);
    f.scene.set_hovered(&f.leaf, true);
    let before = f.scene.stats();
    assert_eq!(
        f.scene.update_children(&f.target, |_| {}).unwrap().removed,
        3
    );
    assert_eq!(f.scene.stats().nodes, before.nodes - 3);
    assert_eq!(f.scene.focused(), None);
    assert_eq!(f.scene.state(&f.leaf), None);
    assert_eq!(f.scene.children(&f.nested), None);
    assert_eq!(f.scene.children(&f.target), Some(&[][..]));
    check_hierarchy(&f.scene);

    let stats = f
        .scene
        .update_children(&f.target, |ui| {
            ui.container("nested", |ui| {
                ui.push_id("items", |ui| {
                    assert_eq!(ui.button("leaf", "New leaf"), f.leaf);
                });
            });
        })
        .unwrap();
    assert_eq!(stats.created, 2);
    assert_ne!(f.scene.entities[&f.leaf], entity);
    assert!(!f.scene.state(&f.leaf).unwrap().focused());
    assert!(!f.scene.state(&f.leaf).unwrap().hovered());
    check_hierarchy(&f.scene);
}

#[test]
fn omitted_nested_container_is_pruned_and_retained_empty_container_is_reset() {
    let mut f = fixture();
    f.scene.focus(&f.target);
    assert_eq!(
        f.scene
            .update_children(&f.target, |ui| {
                ui.button("first", "First");
                ui.container("nested", |_| {});
            })
            .unwrap(),
        UpdateStats {
            removed: 1,
            ..UpdateStats::default()
        }
    );
    assert_eq!(f.scene.children(&f.nested), Some(&[][..]));
    assert_eq!(f.scene.focused(), Some(&f.target));
    check_hierarchy(&f.scene);

    assert_eq!(
        f.scene
            .update_children(&f.target, |ui| {
                ui.button("first", "First");
            })
            .unwrap()
            .removed,
        1
    );
    assert_eq!(f.scene.children(&f.nested), None);
    assert_eq!(f.scene.focused(), Some(&f.target));
    check_hierarchy(&f.scene);
}

#[test]
fn duplicate_partial_declaration_is_atomic_including_hierarchy_and_focus() {
    let mut f = fixture();
    f.scene.focus(&f.leaf);
    let entities = f.scene.entities.clone();
    let hierarchy = f.scene.children.clone();
    let states: HashMap<_, _> = entities
        .keys()
        .map(|id| (id.clone(), f.scene.state(id)))
        .collect();
    let error = f.scene.update_children(&f.target, |ui| {
        ui.button("first", "Changed");
        ui.container("duplicate", |_| {});
        ui.container("duplicate", |_| {});
    });
    assert!(matches!(error, Err(ReconcileError::DuplicateId(_))));
    assert_eq!(f.scene.entities, entities);
    assert_eq!(f.scene.children, hierarchy);
    assert_eq!(f.scene.label(&f.first), Some("First"));
    assert_eq!(f.scene.focused(), Some(&f.leaf));
    for (id, state) in states {
        assert_eq!(f.scene.state(&id), state);
    }
    check_hierarchy(&f.scene);
}

#[test]
fn invalid_targets_are_rejected_before_the_builder_runs() {
    let mut f = fixture();
    let mut ran = false;
    assert_eq!(
        f.scene.update_children(&f.first, |_| ran = true),
        Err(ReconcileError::NotContainer(f.first.clone()))
    );
    assert!(!ran);
    let mut unknown = None;
    Scene::default()
        .update(|ui| unknown = Some(ui.container("missing", |_| {})))
        .unwrap();
    let unknown = unknown.unwrap();
    assert_eq!(
        f.scene.update_children(&unknown, |_| ran = true),
        Err(ReconcileError::UnknownWidget(unknown))
    );
    assert!(!ran);
    f.scene.update_children(&f.target, |_| {}).unwrap();
    assert_eq!(
        f.scene.update_children(&f.nested, |_| ran = true),
        Err(ReconcileError::UnknownWidget(f.nested))
    );
    assert!(!ran);
    check_hierarchy(&f.scene);
}

#[test]
fn full_and_partial_updates_can_be_interleaved_without_stale_hierarchy() {
    let mut f = fixture();
    assert_eq!(
        f.scene
            .update_children(&f.target, |ui| {
                ui.button("first", "First");
                ui.container("nested", |ui| {
                    ui.push_id("items", |ui| {
                        ui.button("leaf", "Leaf");
                    });
                });
            })
            .unwrap(),
        UpdateStats::default()
    );

    let mut replacement = None;
    f.scene
        .update(|ui| {
            replacement = Some(ui.container("replacement", |ui| {
                ui.button("child", "Child");
            }));
        })
        .unwrap();
    assert_eq!(f.scene.children(&f.outer), None);
    check_hierarchy(&f.scene);
    let replacement = replacement.unwrap();
    f.scene.update_children(&replacement, |_| {}).unwrap();
    assert_eq!(f.scene.roots(), [replacement]);
    assert_eq!(f.scene.stats().nodes, 1);
    check_hierarchy(&f.scene);
    f.scene.update(|_| {}).unwrap();
    assert!(f.scene.roots().is_empty());
    check_hierarchy(&f.scene);
}

#[test]
fn independent_branch_updates_preserve_external_focus_and_archetypes() {
    let mut f = fixture();
    f.scene.focus(&f.sibling);
    let archetypes = f.scene.stats().archetypes;
    let sibling_entity = f.scene.entities[&f.sibling];
    for index in 0..20 {
        f.scene
            .update_children(&f.target, |ui| {
                if index % 2 == 0 {
                    ui.container("branch", |ui| {
                        ui.button("item", "Item");
                    });
                }
            })
            .unwrap();
        f.scene
            .update_children(&f.other, |ui| {
                ui.button("first", format!("Other {index}"));
            })
            .unwrap();
        assert_eq!(f.scene.focused(), Some(&f.sibling));
        assert_eq!(f.scene.entities[&f.sibling], sibling_entity);
        assert_eq!(f.scene.stats().archetypes, archetypes);
        check_hierarchy(&f.scene);
    }
}

#[test]
fn hierarchy_changes_dirty_the_parent_without_archetype_migration() {
    let mut f = fixture();
    let entity = f.scene.entities[&f.target];
    let archetype = f.scene.world.entity(entity).archetype().id();
    f.scene
        .world
        .get_mut::<WidgetState>(entity)
        .unwrap()
        .set(WidgetState::PAINT_DIRTY, false);
    f.scene
        .update_children(&f.target, |ui| {
            ui.button("first", "First");
            ui.container("nested", |ui| {
                ui.push_id("items", |ui| {
                    ui.button("leaf", "Leaf");
                });
            });
        })
        .unwrap();
    assert!(!f.scene.state(&f.target).unwrap().paint_dirty());
    f.scene.update_children(&f.target, |_| {}).unwrap();
    assert!(f.scene.state(&f.target).unwrap().paint_dirty());
    assert_eq!(f.scene.world.entity(entity).archetype().id(), archetype);
    check_hierarchy(&f.scene);
}

#[test]
fn panicking_partial_builder_leaves_the_scene_unchanged() {
    let mut f = fixture();
    let entities = f.scene.entities.clone();
    let hierarchy = f.scene.children.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = f.scene.update_children(&f.target, |ui| {
            ui.button("first", "Changed");
            panic!("application builder failed");
        });
    }));
    assert!(result.is_err());
    assert_eq!(f.scene.entities, entities);
    assert_eq!(f.scene.children, hierarchy);
    assert_eq!(f.scene.label(&f.first), Some("First"));
    check_hierarchy(&f.scene);
}

#[test]
fn replacing_the_builder_cannot_escape_the_updated_subtree() {
    let mut f = fixture();
    f.scene.focus(&f.leaf);
    let entities = f.scene.entities.clone();
    let hierarchy = f.scene.children.clone();
    let mut escaped = None;
    let result = f.scene.update_children(&f.target, |ui| {
        *ui = Ui::default();
        escaped = Some(ui.button("outside", "Outside"));
    });
    assert_eq!(result, Err(ReconcileError::OutsideScope(escaped.unwrap())));
    assert_eq!(f.scene.entities, entities);
    assert_eq!(f.scene.children, hierarchy);
    assert_eq!(f.scene.focused(), Some(&f.leaf));
    check_hierarchy(&f.scene);
}
