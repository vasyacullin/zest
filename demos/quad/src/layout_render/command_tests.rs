use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};

fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    demo
}
fn key(demo: &mut LayoutDemo, named: NamedKey) {
    demo.key(&Key::Named(named), true, false);
}
fn activate(demo: &mut LayoutDemo) {
    key(demo, NamedKey::Enter);
    demo.key(&Key::Named(NamedKey::Enter), false, false);
}
fn point(demo: &LayoutDemo, id: &WidgetId) -> Point<Logical> {
    let bounds = demo.snapshot.as_ref().unwrap().bounds(id).unwrap();
    Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0)
}

#[test]
fn deferred_focus_runs_after_the_original_targets_default_action() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let archetypes = demo.scene.stats().archetypes;
    key(&mut demo, NamedKey::F7);
    demo.pointer_moved(point(&demo, &title));
    demo.pointer_press();
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(demo.selected.is_none());
    demo.pointer_release();
    assert_eq!(demo.selected.as_ref(), Some(&title));
    assert_eq!(demo.scene.focused(), Some(&demo.footer));
    assert!(!demo.scene.state(&title).unwrap().pressed());
    activate(&mut demo);
    assert_eq!(demo.selected.as_ref(), Some(&demo.footer));
    key(&mut demo, NamedKey::F7);
    demo.pointer_moved(point(&demo, &title));
    demo.pointer_press();
    demo.pointer_release();
    assert_eq!(demo.selected.as_ref(), Some(&title));
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(snapshot.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert_eq!(demo.scene.stats().archetypes, archetypes);
}

#[test]
fn prevented_defaults_and_modal_boundaries_do_not_queue_outer_focus_changes() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let body = demo.labels[1].id.clone();
    key(&mut demo, NamedKey::F7);
    key(&mut demo, NamedKey::F6);
    demo.pointer_moved(point(&demo, &title));
    demo.pointer_press();
    demo.pointer_release();
    assert!(demo.selected.is_none());
    assert_eq!(demo.scene.focused(), Some(&title));
    key(&mut demo, NamedKey::F6);
    demo.activation_handlers
        .set(&demo.scene, &title, EventPhase::Target, |_, _, _| {
            EventControl::PreventDefault
        });
    activate(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(demo.selected.is_none());
    demo.activation_handlers.remove(&title, EventPhase::Target);
    key(&mut demo, NamedKey::F3);
    activate(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&body));
    assert_eq!(demo.selected.as_ref(), Some(&body));
    key(&mut demo, NamedKey::Escape);
    activate(&mut demo);
    assert_eq!(demo.selected.as_ref(), Some(&title));
    assert_eq!(demo.scene.focused(), Some(&demo.footer));
}

#[test]
fn f7_guards_and_scrollbar_cancellation_match_other_preview_commands() {
    let mut demo = demo();
    let f7 = Key::Named(NamedKey::F7);
    assert!(!demo.key(&f7, false, false));
    assert!(!demo.key(&f7, true, true));
    assert!(!demo.key_event(&f7, true, true, false));
    demo.modifiers(ModifiersState::CONTROL);
    assert!(!demo.key(&f7, true, false));
    demo.modifiers(ModifiersState::empty());
    demo.focus_window(false);
    assert!(!demo.key(&f7, true, false));
    demo.focus_window(true);
    assert!(!demo.focus_footer);
    key(&mut demo, NamedKey::F4);
    let thumb = demo.scrollbar().unwrap().thumb();
    demo.pointer_moved(Point::new(thumb.origin.x + 2.0, thumb.origin.y + 2.0));
    demo.pointer_press();
    assert!(demo.scrollbar_dragging());
    assert!(!demo.key_event(&f7, true, true, false));
    assert!(demo.scrollbar_dragging());
    assert!(demo.key(&f7, true, false));
    assert!(demo.focus_footer && !demo.scrollbar_dragging());
}
