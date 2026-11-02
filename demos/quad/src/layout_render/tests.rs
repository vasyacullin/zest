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
    assert!(!demo.pointer_release());
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
    demo.pointer_release();
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

#[test]
fn release_toggles_selection_only_after_a_complete_same_target_gesture() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let footer = demo.footer.clone();
    demo.pointer_moved(center(&demo, &title));
    demo.pointer_press();
    assert!(demo.scene.state(&title).unwrap().pressed());
    assert!(demo.selected.is_none());
    assert_eq!(demo.activation.captured(), Some(&title));
    demo.pointer_left();
    assert!(!demo.scene.state(&title).unwrap().pressed());
    assert_eq!(demo.activation.captured(), Some(&title));
    demo.pointer_moved(center(&demo, &title));
    assert!(demo.scene.state(&title).unwrap().pressed());
    assert!(demo.pointer_release());
    assert_eq!(demo.selected.as_ref(), Some(&title));
    assert!(!demo.pointer_release());
    demo.pointer_press();
    demo.pointer_moved(center(&demo, &footer));
    demo.pointer_release();
    assert_eq!(demo.selected.as_ref(), Some(&title));
    demo.pointer_moved(center(&demo, &title));
    demo.pointer_press();
    demo.pointer_release();
    assert!(demo.selected.is_none());
    demo.pointer_moved(Point::new(5.0, 5.0));
    demo.pointer_press();
    demo.pointer_moved(center(&demo, &title));
    assert!(!demo.pointer_release());
    assert!(demo.selected.is_none());
}

#[test]
fn resize_dpi_occlusion_and_focus_loss_cancel_held_gestures() {
    for cancel in 0..4 {
        let mut demo = demo();
        let title = demo.labels[0].id.clone();
        demo.pointer_moved(center(&demo, &title));
        demo.pointer_press();
        match cancel {
            0 => {
                demo.update_geometry(Size::new(400.0, 450.0), 1.25).unwrap();
            }
            1 => {
                demo.update_geometry(Size::new(480.0, 450.0), 1.5).unwrap();
            }
            2 => {
                demo.cancel_pointer();
            }
            _ => {
                demo.focus_window(false);
                demo.focus_window(true);
            }
        }
        assert!(!demo.scene.state(&title).unwrap().pressed());
        demo.pointer_moved(center(&demo, &title));
        assert!(!demo.pointer_release());
        assert!(demo.selected.is_none());
        demo.pointer_press();
        assert!(demo.pointer_release());
        assert_eq!(demo.selected.as_ref(), Some(&title));
    }
}

#[test]
fn keyboard_activation_pairs_releases_and_cancels_on_tab_modifiers_and_resize() {
    let mut demo = demo();
    tab(&mut demo);
    let title = demo.labels[0].id.clone();
    for key in [
        Key::Named(NamedKey::Enter),
        Key::Named(NamedKey::Space),
        Key::Character(" ".into()),
    ] {
        assert!(!demo.key(&key, false, false));
        assert!(demo.key(&key, true, false));
        assert!(!demo.key_event(&key, true, true, false));
        assert!(!demo.key(&key, false, true));
        assert!(demo.selected.is_none());
        assert!(demo.scene.state(&title).unwrap().pressed());
        assert!(demo.key(&key, false, false));
        assert_eq!(demo.selected.as_ref(), Some(&title));
        demo.key(&key, true, false);
        demo.key(&key, false, false);
        assert!(demo.selected.is_none());
    }
    let key = Key::Named(NamedKey::Enter);
    demo.key(&key, true, false);
    tab(&mut demo);
    assert!(!demo.scene.state(&title).unwrap().pressed());
    assert!(!demo.key(&key, false, false));
    demo.key(&key, true, false);
    assert!(demo.modifiers(ModifiersState::CONTROL));
    demo.modifiers(ModifiersState::empty());
    assert!(!demo.key(&key, true, false));
    assert!(!demo.key(&key, false, false));
    demo.key(&key, true, false);
    demo.update_geometry(Size::new(470.0, 450.0), 1.25).unwrap();
    assert!(!demo.key(&key, false, false));
    assert!(demo.selected.is_none());
}

#[test]
fn selection_survives_keyed_reorder_and_is_cleared_by_removal() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    tab(&mut demo);
    let enter = Key::Named(NamedKey::Enter);
    demo.key(&enter, true, false);
    demo.key(&enter, false, false);
    demo.scene
        .update_children(&demo.root, |ui| {
            ui.button("footer", "Layout follows text");
            ui.button("title", demo.labels[0].source);
        })
        .unwrap();
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    assert_eq!(demo.selected.as_ref(), Some(&title));
    demo.scene
        .update_children(&demo.root, |ui| {
            ui.button("footer", "Layout follows text");
        })
        .unwrap();
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    assert!(demo.selected.is_none());
}

#[test]
fn f2_disables_body_clears_its_interaction_and_tab_skips_it_without_reflow() {
    let mut demo = demo();
    let body = demo.labels[1].id.clone();
    let title = demo.labels[0].id.clone();
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let archetypes = demo.scene.stats().archetypes;
    let f2 = Key::Named(NamedKey::F2);
    demo.pointer_moved(center(&demo, &body));
    demo.pointer_press();
    assert!(demo.scene.state(&body).unwrap().pressed());
    assert!(demo.key(&f2, true, false));
    assert!(demo.scene.focused().is_none() && demo.hovered.is_none());
    assert!(!demo.scene.state(&body).unwrap().enabled());
    assert!(!demo.pointer_release());
    assert!(!demo.key(&f2, false, false));
    assert!(!demo.key_event(&f2, true, true, false));
    assert!(!demo.key(&f2, true, true));
    tab(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&title));
    tab(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&demo.footer));
    demo.modifiers(ModifiersState::SHIFT);
    tab(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&title));
    demo.modifiers(ModifiersState::CONTROL);
    assert!(!demo.key(&f2, true, false));
    demo.modifiers(ModifiersState::empty());
    assert!(demo.key(&f2, true, false));
    assert!(demo.scene.state(&body).unwrap().enabled());
    assert_eq!(demo.hovered.as_ref(), Some(&body));
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(demo.selected.is_none());
    assert!(!demo.update_geometry(snapshot.viewport(), 1.25).unwrap());
    assert!(snapshot.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert_eq!(demo.scene.stats().archetypes, archetypes);
}

#[test]
fn disabling_a_selected_node_retains_selection_but_cancels_keyboard_activation() {
    let mut demo = demo();
    let body = demo.labels[1].id.clone();
    demo.pointer_moved(center(&demo, &body));
    demo.pointer_press();
    demo.pointer_release();
    assert_eq!(demo.selected.as_ref(), Some(&body));
    let enter = Key::Named(NamedKey::Enter);
    demo.key(&enter, true, false);
    demo.set_enabled(&body, false);
    demo.set_enabled(&body, true);
    demo.set_focus(Some(body.clone()));
    assert!(!demo.key(&enter, false, false));
    assert_eq!(demo.selected.as_ref(), Some(&body));
    demo.key(&enter, true, false);
    demo.key(&enter, false, false);
    assert!(demo.selected.is_none());
    demo.set_enabled(&demo.root.clone(), false);
    assert!(!tab(&mut demo));
    assert!(demo.scene.focused().is_none());
    assert!(!demo.pointer_press());
    assert!(!demo.pointer_release());
}
