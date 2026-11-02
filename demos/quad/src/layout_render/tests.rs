use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};
fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    demo
}
fn center(demo: &LayoutDemo, id: &WidgetId) -> Point<Logical> {
    let bounds = demo.snapshot.as_ref().unwrap().bounds(id).unwrap();
    Point::new(
        bounds.origin.x + bounds.size.width * 0.5,
        bounds.origin.y + bounds.size.height * 0.5,
    )
}
fn tab(demo: &mut LayoutDemo) -> bool {
    demo.key(&Key::Named(NamedKey::Tab), true, false)
}

#[test]
fn empty_viewports_and_empty_footer_bounds_emit_only_valid_geometry() {
    let mut demo = LayoutDemo::new().unwrap();
    demo.tree.sync(&demo.scene, |_| Style::default()).unwrap();
    demo.snapshot = Some(demo.tree.compute(Size::new(0.0, 0.0)).unwrap().snapshot);
    assert!(demo.paint().unwrap().is_empty());
    demo.snapshot = Some(demo.tree.compute(Size::new(40.0, 40.0)).unwrap().snapshot);
    assert_eq!(demo.paint().unwrap().len(), 1);
}

#[test]
fn hover_click_and_blank_space_update_only_scene_interaction_flags() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let body = demo.labels[1].id.clone();
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let archetypes = demo.scene.stats().archetypes;
    assert!(demo.pointer_moved(center(&demo, &title)));
    assert!(demo.scene.state(&title).unwrap().hovered());
    assert!(!demo.pointer_moved(center(&demo, &title)));
    assert!(demo.pointer_press());
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(!demo.pointer_press());
    assert!(demo.pointer_moved(center(&demo, &body)));
    assert!(!demo.scene.state(&title).unwrap().hovered());
    assert!(demo.scene.state(&body).unwrap().hovered());
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(!demo.pointer_moved(Point::new(f64::NAN, 0.0)));
    assert!(!demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap());
    assert!(snapshot.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert!(demo.pointer_moved(Point::new(5.0, 5.0)));
    assert!(demo.pointer_press());
    assert!(demo.scene.focused().is_none() && demo.hovered.is_none());
    assert_eq!(demo.scene.stats().archetypes, archetypes);
}

#[test]
fn tab_order_wraps_in_both_directions_and_ignores_unsupported_or_unfocused_input() {
    let mut demo = demo();
    let ids = [
        demo.labels[0].id.clone(),
        demo.labels[1].id.clone(),
        demo.footer.clone(),
    ];
    for expected in [&ids[0], &ids[1], &ids[2], &ids[0]] {
        assert!(tab(&mut demo));
        assert_eq!(demo.scene.focused(), Some(expected));
    }
    demo.modifiers(ModifiersState::SHIFT);
    assert!(tab(&mut demo));
    assert_eq!(demo.scene.focused(), Some(&ids[2]));
    for modifiers in [
        ModifiersState::CONTROL,
        ModifiersState::ALT,
        ModifiersState::SUPER,
        ModifiersState::CONTROL | ModifiersState::ALT,
    ] {
        demo.modifiers(modifiers);
        assert!(!tab(&mut demo));
    }
    demo.modifiers(ModifiersState::empty());
    assert!(!demo.key(&Key::Named(NamedKey::Tab), false, false));
    assert!(!demo.key(&Key::Named(NamedKey::Tab), true, true));
    assert!(demo.focus_window(false));
    assert!(demo.scene.focused().is_none());
    assert!(!demo.pointer_moved(center(&demo, &ids[0])));
    assert!(!demo.pointer_press() && !tab(&mut demo));
    demo.modifiers(ModifiersState::CONTROL);
    demo.focus_window(true);
    assert!(tab(&mut demo));
    assert_eq!(demo.scene.focused(), Some(&ids[0]));
}

#[test]
fn resize_rehits_a_stationary_pointer_before_click_and_clears_clipped_focus() {
    let mut demo = demo();
    let footer = demo.footer.clone();
    let body = demo.labels[1].id.clone();
    let point = Point::new(34.0, center(&demo, &footer).y);
    demo.pointer_moved(point);
    demo.pointer_press();
    assert_eq!(demo.scene.focused(), Some(&footer));
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    assert_eq!(demo.hovered, Some(body.clone()));
    assert!(!demo.scene.state(&footer).unwrap().hovered());
    demo.pointer_press();
    assert_eq!(demo.scene.focused(), Some(&body));
    demo.update_geometry(Size::new(240.0, 60.0), 1.25).unwrap();
    assert!(demo.scene.focused().is_none() && demo.hovered.is_none());
    assert!(tab(&mut demo));
    assert_eq!(demo.scene.focused(), Some(&demo.labels[0].id));
    assert!(!tab(&mut demo));
    demo.update_geometry(Size::new(0.0, 0.0), 1.25).unwrap();
    assert!(demo.scene.focused().is_none() && !tab(&mut demo));
}

#[test]
fn keyed_reorder_preserves_focus_and_tab_follows_the_new_snapshot_order() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let body = demo.labels[1].id.clone();
    demo.pointer_moved(center(&demo, &body));
    demo.pointer_press();
    demo.scene
        .update_children(&demo.root, |ui| {
            ui.button("footer", "Layout follows text");
            ui.button("body", demo.labels[1].source);
            ui.button("title", demo.labels[0].source);
        })
        .unwrap();
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    assert_eq!(demo.scene.focused(), Some(&body));
    assert!(tab(&mut demo));
    assert_eq!(demo.scene.focused(), Some(&title));
    demo.scene
        .update_children(&demo.root, |ui| {
            ui.button("footer", "Layout follows text");
        })
        .unwrap();
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    assert!(demo.scene.focused().is_none());
    assert!(tab(&mut demo));
    assert_eq!(demo.scene.focused(), Some(&demo.footer));
}

#[test]
fn dpi_changes_drop_stale_pointer_coordinates_while_retaining_visible_keyed_focus() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    demo.pointer_moved(center(&demo, &title));
    demo.pointer_press();
    demo.update_geometry(Size::new(480.0, 450.0), 1.5).unwrap();
    assert!(demo.pointer.is_none() && demo.hovered.is_none());
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(!demo.pointer_left());
}
