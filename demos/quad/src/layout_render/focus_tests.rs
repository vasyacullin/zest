use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};
fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    demo.key(&Key::Named(NamedKey::F4), true, false);
    demo
}
fn tab(demo: &mut LayoutDemo) -> bool {
    demo.key(&Key::Named(NamedKey::Tab), true, false)
}

#[test]
fn tab_and_shift_tab_reveal_the_complete_order_without_replacing_shapes_or_archetypes() {
    let mut demo = demo();
    let ids = [
        demo.labels[0].id.clone(),
        demo.labels[1].id.clone(),
        demo.footer.clone(),
    ];
    let old = demo.snapshot.as_ref().unwrap().clone();
    let shapes: Vec<_> = demo
        .labels
        .iter()
        .map(|label| label.content.clone())
        .collect();
    let archetypes = demo.scene.stats().archetypes;
    assert!(old.visible_bounds(&ids[2]).is_none());
    for expected in [&ids[0], &ids[1], &ids[2], &ids[0]] {
        assert!(tab(&mut demo));
        assert_eq!(demo.scene.focused(), Some(expected));
        assert!(demo.visible(expected));
    }
    demo.modifiers(ModifiersState::SHIFT);
    for expected in [&ids[2], &ids[1], &ids[0]] {
        assert!(tab(&mut demo));
        assert_eq!(demo.scene.focused(), Some(expected));
        assert!(demo.visible(expected));
    }
    assert!(old.visible_bounds(&ids[2]).is_none());
    assert_eq!(
        shapes,
        demo.labels
            .iter()
            .map(|label| label.content.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(archetypes, demo.scene.stats().archetypes);
}

#[test]
fn disabled_nodes_are_skipped_and_modal_tab_reveals_without_changing_the_focused_id() {
    let mut demo = demo();
    let body = demo.labels[1].id.clone();
    let title = demo.labels[0].id.clone();
    let footer = demo.footer.clone();
    demo.set_enabled(&body, false);
    tab(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&title));
    tab(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&footer));
    assert!(demo.visible(&footer));
    demo.set_enabled(&body, true);
    // Return to the top, then focus the partially visible body via its modal.
    demo.key(&Key::Named(NamedKey::F4), true, false);
    demo.key(&Key::Named(NamedKey::F4), true, false);
    demo.key(&Key::Named(NamedKey::F3), true, false);
    assert_eq!(demo.scene.focused(), Some(&body));
    let before = demo.scroll_offset;
    let enter = Key::Named(NamedKey::Enter);
    demo.key(&enter, true, false);
    assert!(tab(&mut demo));
    assert_ne!(demo.scroll_offset, before);
    assert_eq!(demo.scene.focused(), Some(&body));
    assert!(!demo.scene.state(&body).unwrap().pressed());
    assert!(!demo.key(&enter, false, false));
    assert!(demo.selected.is_none());
    let stable = demo.snapshot.as_ref().unwrap().clone();
    assert!(!tab(&mut demo));
    assert!(stable.same_snapshot(demo.snapshot.as_ref().unwrap()));
}

#[test]
fn revealing_a_keyboard_target_rehits_pointer_and_cancels_old_mouse_capture() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let footer = demo.footer.clone();
    let old_footer = demo.snapshot.as_ref().unwrap().bounds(&footer).unwrap();
    demo.pointer_moved(Point::new(34.0, 40.0));
    demo.pointer_press();
    assert!(demo.scene.state(&title).unwrap().pressed());
    demo.modifiers(ModifiersState::SHIFT);
    tab(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&footer));
    assert!(demo.visible(&footer));
    assert!(!demo.pointer_release());
    assert!(demo.selected.is_none());
    assert_eq!(demo.hovered.as_ref(), Some(&demo.labels[1].id));
    assert!(
        demo.hit(Point::new(34.0, old_footer.origin.y + 6.0))
            .is_none()
    );
    let bounds = demo.snapshot.as_ref().unwrap().bounds(&footer).unwrap();
    let point = Point::new(bounds.origin.x + 10.0, bounds.origin.y + 6.0);
    assert_eq!(demo.hit(point), Some(footer.clone()));
    demo.pointer_moved(point);
    demo.pointer_press();
    demo.pointer_release();
    assert_eq!(demo.selected.as_ref(), Some(&footer));
}

#[test]
fn unreachable_clips_and_ignored_keys_cannot_scroll_or_steal_focus() {
    let mut demo = demo();
    let tab_key = Key::Named(NamedKey::Tab);
    let old = demo.snapshot.as_ref().unwrap().clone();
    assert!(!demo.key(&tab_key, true, true));
    assert!(!demo.key(&tab_key, false, false));
    demo.modifiers(ModifiersState::CONTROL);
    assert!(!tab(&mut demo));
    assert!(old.same_snapshot(demo.snapshot.as_ref().unwrap()));
    demo.modifiers(ModifiersState::empty());
    demo.focus_window(false);
    assert!(!tab(&mut demo));
    demo.focus_window(true);
    demo.update_geometry(Size::new(0.0, 0.0), 1.25).unwrap();
    assert!(!tab(&mut demo));
    assert!(demo.scene.focused().is_none());
}
