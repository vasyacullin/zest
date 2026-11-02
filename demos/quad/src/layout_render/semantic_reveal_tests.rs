use super::*;
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
fn key(demo: &mut LayoutDemo, key: NamedKey) {
    demo.key(&Key::Named(key), true, false);
}

#[test]
fn hidden_target_is_revealed_before_activation_and_visible_reuse_keeps_offset() {
    let mut demo = demo();
    key(&mut demo, NamedKey::F4);
    let footer = target(&demo, "layout.footer");
    let old = demo.snapshot.as_ref().unwrap().clone();
    let plan = old
        .semantic_view(&demo.scene.semantic_snapshot())
        .reveal_target(&footer)
        .unwrap();
    assert!(!demo.semantic_activate(&footer));
    assert!(demo.semantic_reveal_and_activate(&footer).unwrap());
    assert_eq!(demo.scroll_offset, plan.adjustments()[0].offset());
    assert_eq!(demo.scene.focused(), Some(footer.id()));
    assert_eq!(demo.selected.as_ref(), Some(footer.id()));
    assert!(!demo.scene.state(footer.id()).unwrap().pressed());
    assert!(!demo.activation.pointer_down());
    assert!(demo.visible(footer.id()));
    assert!(old.visible_bounds(footer.id()).is_none());
    assert!(!plan.can_apply(&demo.scene, demo.snapshot.as_ref().unwrap()));
    let offset = demo.scroll_offset;
    assert!(demo.semantic_reveal_and_activate(&footer).unwrap());
    assert!(demo.selected.is_none());
    assert_eq!(demo.scroll_offset, offset);
}

#[test]
fn rejected_requests_preserve_pending_gesture_focus_and_scroll() {
    let mut demo = demo();
    let stale = target(&demo, "layout.footer");
    metadata_checks::without_footer(&mut demo).unwrap();
    metadata_checks::redeclare(&mut demo, "Layout status", "layout.footer").unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    key(&mut demo, NamedKey::F4);
    let footer = target(&demo, "layout.footer");
    let root = target(&demo, "layout.root");
    let foreign = target(&self::demo(), "layout.footer");
    demo.set_enabled(footer.id(), false);
    let title = demo.labels[0].id.clone();
    let bounds = demo.snapshot.as_ref().unwrap().bounds(&title).unwrap();
    demo.pointer_moved(Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0));
    demo.pointer_press();
    for rejected in [&stale, &foreign, &root, &footer] {
        assert!(!demo.semantic_reveal_and_activate(rejected).unwrap());
        assert!(demo.scene.state(&title).unwrap().pressed());
        assert!(demo.activation.pointer_down());
        assert_eq!(demo.scene.focused(), Some(&title));
        assert_eq!(demo.scroll_offset, Point::new(0.0, 0.0));
    }
    demo.pointer_release();
    demo.set_enabled(footer.id(), true);
    demo.focus_window(false);
    assert!(!demo.semantic_reveal_and_activate(&footer).unwrap());
    demo.focus_window(true);
    demo.update_geometry(Size::new(0.0, 0.0), 1.25).unwrap();
    assert!(!demo.semantic_reveal_and_activate(&footer).unwrap());
    assert_eq!(demo.scroll_offset, Point::new(0.0, 0.0));
}

#[test]
fn modal_owner_policy_precedes_reveal_and_shared_event_prevention_follows_it() {
    let mut demo = demo();
    let footer = target(&demo, "layout.footer");
    key(&mut demo, NamedKey::F4);
    demo.scene.push_modal(footer.id());
    let focus_before = demo.scene.focused().cloned();
    assert!(!demo.semantic_reveal_and_activate(&footer).unwrap());
    assert_eq!(demo.scroll_offset, Point::new(0.0, 0.0));
    assert!(demo.selected.is_none());
    assert_eq!(demo.scene.focused(), focus_before.as_ref());
    demo.scene.pop_modal();
    key(&mut demo, NamedKey::F6);
    demo.scene.push_modal(&demo.root);
    assert!(demo.semantic_reveal_and_activate(&footer).unwrap());
    assert!(demo.visible(footer.id()));
    assert!(demo.selected.is_none()); // Capture handler still prevents default.
    assert_eq!(demo.scene.focused(), Some(footer.id()));
    demo.scene.pop_modal();
    key(&mut demo, NamedKey::F6);
    demo.scene.push_modal(footer.id());
    let offset = demo.scroll_offset;
    assert!(demo.semantic_reveal_and_activate(&footer).unwrap());
    assert_eq!(demo.selected.as_ref(), Some(footer.id()));
    assert_eq!(demo.scroll_offset, offset); // Visible leaf needs no outside owner edit.
}
