use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};

fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    demo
}
fn key(demo: &mut LayoutDemo, named: NamedKey) -> bool {
    demo.key(&Key::Named(named), true, false)
}
fn activate(demo: &mut LayoutDemo, named: NamedKey) {
    assert!(key(demo, named));
    assert!(demo.key(&Key::Named(named), false, false));
}
fn center(bounds: Rect<Logical>) -> Point<Logical> {
    Point::new(
        bounds.origin.x + bounds.size.width * 0.5,
        bounds.origin.y + bounds.size.height * 0.5,
    )
}

#[test]
fn parent_consumes_pointer_and_keyboard_actions_without_disabling_focus_or_gestures() {
    let mut demo = demo();
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let archetypes = demo.scene.stats().archetypes;
    let point = center(snapshot.bounds(&demo.footer).unwrap());
    assert!(key(&mut demo, NamedKey::F6));
    demo.pointer_moved(point);
    assert!(demo.pointer_press());
    assert!(demo.scene.state(&demo.footer).unwrap().pressed());
    assert!(demo.pointer_release());
    assert!(demo.selected.is_none());
    assert_eq!(demo.scene.focused(), Some(&demo.footer));
    assert!(demo.scene.state(&demo.footer).unwrap().enabled());
    for named in [NamedKey::Enter, NamedKey::Space] {
        activate(&mut demo, named);
        assert!(demo.selected.is_none());
    }
    assert!(key(&mut demo, NamedKey::F6));
    activate(&mut demo, NamedKey::Enter);
    assert_eq!(demo.selected.as_ref(), Some(&demo.footer));
    activate(&mut demo, NamedKey::Space);
    assert!(demo.selected.is_none());
    assert!(!demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap());
    assert!(snapshot.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert_eq!(demo.scene.stats().archetypes, archetypes);
}

#[test]
fn modal_boundary_excludes_the_outer_parent_interceptor_then_restores_it() {
    let mut demo = demo();
    key(&mut demo, NamedKey::F6);
    key(&mut demo, NamedKey::F3);
    let body = demo.labels[1].id.clone();
    assert_eq!(demo.scene.modal_root(), Some(&body));
    activate(&mut demo, NamedKey::Enter);
    assert_eq!(demo.selected.as_ref(), Some(&body));
    activate(&mut demo, NamedKey::Space);
    assert!(demo.selected.is_none());
    key(&mut demo, NamedKey::Escape);
    demo.pointer_moved(center(
        demo.snapshot.as_ref().unwrap().bounds(&body).unwrap(),
    ));
    demo.pointer_press();
    demo.pointer_release();
    assert_eq!(demo.scene.focused(), Some(&body));
    assert!(demo.selected.is_none());
}

#[test]
fn interception_shortcut_ignores_releases_repeats_synthetic_modifiers_and_blur() {
    let mut demo = demo();
    let f6 = Key::Named(NamedKey::F6);
    assert!(!demo.key(&f6, false, false));
    assert!(!demo.key(&f6, true, true));
    assert!(!demo.key_event(&f6, true, true, false));
    for modifiers in [
        ModifiersState::CONTROL,
        ModifiersState::ALT,
        ModifiersState::SUPER,
    ] {
        demo.modifiers(modifiers);
        assert!(!demo.key(&f6, true, false));
    }
    demo.modifiers(ModifiersState::empty());
    demo.focus_window(false);
    assert!(!demo.key(&f6, true, false));
    assert!(!demo.intercept_activation);
    demo.focus_window(true);
    assert!(demo.key(&f6, true, false));
    assert!(demo.intercept_activation);
}

#[test]
fn only_a_real_single_interception_press_cancels_a_scrollbar_drag() {
    let mut demo = demo();
    key(&mut demo, NamedKey::F4);
    demo.pointer_moved(center(demo.scrollbar().unwrap().thumb()));
    demo.pointer_press();
    assert!(demo.scrollbar_dragging());
    let f6 = Key::Named(NamedKey::F6);
    assert!(!demo.key(&f6, false, false));
    assert!(!demo.key(&f6, true, true));
    assert!(!demo.key_event(&f6, true, true, false));
    assert!(demo.scrollbar_dragging());
    assert!(demo.key(&f6, true, false));
    assert!(!demo.scrollbar_dragging());
    assert!(demo.intercept_activation && demo.selected.is_none());
}
