use super::*;
use std::{cell::Cell, rc::Rc};
use winit::keyboard::{Key, NamedKey};
use zest::semantics::SemanticQuery;

fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    demo
}
fn target(demo: &LayoutDemo, id: &str) -> SemanticTarget {
    demo.scene
        .semantic_snapshot()
        .find_target(SemanticQuery::new().test_id(id))
        .unwrap()
}
fn point(demo: &LayoutDemo, id: &WidgetId) -> Point<Logical> {
    let bounds = demo.snapshot.as_ref().unwrap().bounds(id).unwrap();
    Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0)
}
fn key(demo: &mut LayoutDemo, key: NamedKey) {
    demo.key(&Key::Named(key), true, false);
}

#[test]
fn logical_activation_uses_shared_handlers_prevention_modal_scope_and_deferred_focus() {
    let mut demo = demo();
    let title = target(&demo, "layout.title");
    let body = target(&demo, "layout.body");
    let geometry = demo.snapshot.as_ref().unwrap().clone();
    let archetypes = demo.scene.stats().archetypes;
    let calls = Rc::new(Cell::new(0));
    let handler_calls = calls.clone();
    demo.activation_handlers.set(
        &demo.scene,
        title.id(),
        EventPhase::Target,
        move |_, source, _| {
            assert_eq!(*source, ActivationSource::Semantic);
            handler_calls.set(handler_calls.get() + 1);
            EventControl::Continue
        },
    );
    assert!(demo.semantic_activate(&title));
    assert_eq!(demo.selected.as_ref(), Some(title.id()));
    assert_eq!(demo.scene.focused(), Some(title.id()));
    assert!(!demo.scene.state(title.id()).unwrap().pressed());
    assert!(!demo.activation.pointer_down());
    assert!(demo.semantic_activate(&title));
    assert!(demo.selected.is_none());
    key(&mut demo, NamedKey::F6);
    demo.semantic_activate(&title);
    assert!(demo.selected.is_none());
    assert_eq!(calls.get(), 2);
    key(&mut demo, NamedKey::F6);
    key(&mut demo, NamedKey::F7);
    demo.semantic_activate(&title);
    assert_eq!(demo.selected.as_ref(), Some(title.id()));
    assert_eq!(demo.scene.focused(), Some(&demo.footer));
    assert_eq!(calls.get(), 3);
    key(&mut demo, NamedKey::F3);
    demo.semantic_activate(&body);
    assert_eq!(demo.selected.as_ref(), Some(body.id()));
    assert_eq!(demo.scene.focused(), Some(body.id()));
    assert!(geometry.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert_eq!(demo.scene.stats().archetypes, archetypes);
}

#[test]
fn valid_semantic_activation_cancels_pending_pointer_release_but_invalid_actions_do_not() {
    let mut demo = demo();
    let title = target(&demo, "layout.title");
    let footer = target(&demo, "layout.footer");
    demo.pointer_moved(point(&demo, title.id()));
    demo.pointer_press();
    let mut other = Scene::default();
    other
        .update(|ui| {
            ui.button("foreign", "Foreign");
        })
        .unwrap();
    let foreign = other.semantic_snapshot().target(&other.roots()[0]).unwrap();
    assert!(!demo.semantic_activate(&foreign));
    assert!(demo.scene.state(title.id()).unwrap().pressed() && demo.activation.pointer_down());
    assert!(demo.semantic_activate(&footer));
    assert!(!demo.scene.state(title.id()).unwrap().pressed());
    assert!(demo.activation.pointer_down()); // Retain held-input latch until release.
    assert_eq!(demo.selected.as_ref(), Some(footer.id()));
    demo.pointer_release();
    assert!(!demo.activation.pointer_down());
    assert_eq!(demo.selected.as_ref(), Some(footer.id()));
}

#[test]
fn stale_disabled_blocked_hidden_group_and_unfocused_targets_are_rejected() {
    let mut demo = demo();
    let stale = target(&demo, "layout.footer");
    metadata_checks::without_footer(&mut demo).unwrap();
    metadata_checks::redeclare(&mut demo, "Layout status", "layout.footer").unwrap();
    assert!(!demo.semantic_activate(&stale));
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    let footer = target(&demo, "layout.footer");
    let root = target(&demo, "layout.root");
    assert!(!demo.semantic_activate(&root));
    demo.set_enabled(footer.id(), false);
    assert!(!demo.semantic_activate(&footer));
    demo.set_enabled(footer.id(), true);
    key(&mut demo, NamedKey::F3);
    assert!(!demo.semantic_activate(&footer));
    key(&mut demo, NamedKey::Escape);
    key(&mut demo, NamedKey::F4);
    assert!(!demo.semantic_activate(&footer));
    key(&mut demo, NamedKey::F4);
    demo.focus_window(false);
    assert!(!demo.semantic_activate(&footer));
    demo.focus_window(true);
    assert!(demo.semantic_activate(&footer));
    assert_eq!(demo.selected.as_ref(), Some(footer.id()));
}
