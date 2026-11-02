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

#[test]
fn modal_preview_traps_tab_and_pointer_input_then_restores_the_previous_focus() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let body = demo.labels[1].id.clone();
    let footer = demo.footer.clone();
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let archetypes = demo.scene.stats().archetypes;
    demo.set_focus(Some(footer.clone()));
    assert!(demo.key(&Key::Named(NamedKey::F3), true, false));
    assert_eq!(demo.scene.modal_root(), Some(&body));
    assert_eq!(demo.scene.focused(), Some(&body));
    demo.pointer_moved(center(&demo, &title));
    assert!(demo.hovered.is_none());
    assert!(!demo.pointer_press() && !demo.pointer_release());
    assert_eq!(demo.scene.focused(), Some(&body));
    assert!(!tab(&mut demo));
    demo.modifiers(ModifiersState::SHIFT);
    assert!(!tab(&mut demo));
    demo.modifiers(ModifiersState::empty());
    let enter = Key::Named(NamedKey::Enter);
    demo.key(&enter, true, false);
    demo.key(&enter, false, false);
    assert_eq!(demo.selected.as_ref(), Some(&body));
    assert!(demo.key(&Key::Named(NamedKey::Escape), true, false));
    assert!(!demo.modal_active());
    assert_eq!(demo.scene.focused(), Some(&footer));
    assert_eq!(demo.hovered.as_ref(), Some(&title));
    assert_eq!(demo.selected.as_ref(), Some(&body));
    assert!(!demo.update_geometry(snapshot.viewport(), 1.25).unwrap());
    assert!(snapshot.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert_eq!(demo.scene.stats().archetypes, archetypes);
}

#[test]
fn modal_boundaries_cancel_presses_and_disabling_the_root_closes_the_scope() {
    let mut demo = demo();
    let body = demo.labels[1].id.clone();
    let footer = demo.footer.clone();
    let f3 = Key::Named(NamedKey::F3);
    let enter = Key::Named(NamedKey::Enter);
    demo.pointer_moved(center(&demo, &body));
    demo.pointer_press();
    assert!(demo.key(&f3, true, false));
    assert!(!demo.scene.state(&body).unwrap().pressed());
    assert!(!demo.pointer_release());
    demo.key(&enter, true, false);
    demo.key(&f3, true, false);
    assert!(!demo.key(&enter, false, false));
    assert!(demo.selected.is_none());
    demo.set_focus(Some(footer.clone()));
    demo.key(&f3, true, false);
    demo.key(&Key::Named(NamedKey::F2), true, false);
    assert!(!demo.modal_active());
    assert_eq!(demo.scene.focused(), Some(&footer));
    assert!(!demo.key(&f3, true, false));
    demo.key(&Key::Named(NamedKey::F2), true, false);
    assert!(!demo.modal_active());
}

#[test]
fn modal_shortcuts_ignore_synthetic_repeat_modified_and_unfocused_input() {
    let mut demo = demo();
    let f3 = Key::Named(NamedKey::F3);
    let escape = Key::Named(NamedKey::Escape);
    assert!(!demo.key(&f3, false, false));
    assert!(!demo.key(&f3, true, true));
    assert!(!demo.key_event(&f3, true, true, false));
    demo.modifiers(ModifiersState::CONTROL);
    assert!(!demo.key(&f3, true, false));
    demo.modifiers(ModifiersState::empty());
    demo.focus_window(false);
    assert!(!demo.key(&f3, true, false));
    demo.focus_window(true);
    assert!(demo.key(&f3, true, false));
    assert!(!demo.key(&escape, false, false));
    assert!(!demo.key(&escape, true, true));
    assert!(!demo.key_event(&escape, true, true, false));
    demo.focus_window(false);
    assert!(demo.modal_active() && demo.scene.focused().is_none());
    assert!(!demo.key(&escape, true, false));
    demo.focus_window(true);
    tab(&mut demo);
    assert_eq!(demo.scene.focused(), Some(&demo.labels[1].id));
    assert!(demo.key(&escape, true, false));
}

#[test]
fn clipped_modal_focus_stays_bounded_and_removing_its_root_restores_visible_focus() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let f3 = Key::Named(NamedKey::F3);
    demo.set_focus(Some(title.clone()));
    demo.key(&f3, true, false);
    demo.update_geometry(Size::new(480.0, 50.0), 1.25).unwrap();
    assert!(demo.modal_active() && demo.scene.focused().is_none());
    assert!(!tab(&mut demo));
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(!demo.key(&f3, true, false));
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    demo.key(&f3, true, false);
    demo.scene
        .update_children(&demo.root, |ui| {
            ui.button("title", demo.labels[0].source);
            ui.button("footer", "Layout follows text");
        })
        .unwrap();
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    assert!(!demo.modal_active());
    assert_eq!(demo.scene.focused(), Some(&title));
}

#[test]
fn ancestor_clip_cancels_hidden_pointer_focus_and_preserves_raw_bounds_and_shapes() {
    let mut demo = demo();
    let footer = demo.footer.clone();
    let before = demo.snapshot.as_ref().unwrap().clone();
    let content = demo.labels[1].content.clone();
    demo.pointer_moved(center(&demo, &footer));
    demo.pointer_press();
    assert!(demo.key(&Key::Named(NamedKey::F4), true, false));
    assert!(demo.clipped && demo.hovered.is_none() && demo.scene.focused().is_none());
    assert!(!demo.pointer_release());
    assert!(!demo.visible(&footer));
    let after = demo.snapshot.as_ref().unwrap();
    assert!(!before.same_snapshot(after));
    for node in before.boxes() {
        assert_eq!(after.bounds(&node.id), Some(node.bounds));
    }
    assert_eq!(demo.labels[1].content, content);
    assert!(demo.visible(&demo.labels[1].id));
    assert_eq!(
        demo.hit(Point::new(34.0, CLIP_HEIGHT - 0.001)),
        Some(demo.labels[1].id.clone())
    );
    assert!(demo.hit(Point::new(34.0, CLIP_HEIGHT)).is_none());
    for expected in [
        demo.labels[0].id.clone(),
        demo.labels[1].id.clone(),
        demo.footer.clone(),
    ] {
        tab(&mut demo);
        assert_eq!(demo.scene.focused(), Some(&expected));
    }
    demo.key(&Key::Named(NamedKey::F4), true, false);
    assert!(demo.visible(&footer));
    assert_eq!(demo.hovered.as_ref(), Some(&footer));
    assert!(demo.selected.is_none());
}

#[test]
fn clip_shortcut_ignores_unfocused_modified_repeated_and_synthetic_events() {
    let mut demo = demo();
    let f4 = Key::Named(NamedKey::F4);
    assert!(!demo.key(&f4, false, false));
    assert!(!demo.key(&f4, true, true));
    assert!(!demo.key_event(&f4, true, true, false));
    demo.modifiers(ModifiersState::CONTROL);
    assert!(!demo.key(&f4, true, false));
    demo.focus_window(false);
    assert!(!demo.key(&f4, true, false));
    demo.focus_window(true);
    assert!(demo.key(&f4, true, false));
    assert!(demo.clipped);
    assert!(!demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap());
}

#[test]
fn closing_a_modal_cannot_restore_focus_to_an_ancestor_clipped_widget() {
    let mut demo = demo();
    let footer = demo.footer.clone();
    demo.set_focus(Some(footer));
    demo.key(&Key::Named(NamedKey::F3), true, false);
    demo.key(&Key::Named(NamedKey::F4), true, false);
    assert!(demo.modal_active());
    assert_eq!(demo.scene.focused(), Some(&demo.labels[1].id));
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    assert!(demo.scene.focused().is_none() && !demo.modal_active());
    demo.key(&Key::Named(NamedKey::F4), true, false);
    assert!(demo.scene.focused().is_none());
}

#[test]
fn subpixel_visible_slivers_reject_pointer_input_until_tab_reveals_the_target() {
    let mut demo = demo();
    let old = demo.snapshot.as_ref().unwrap().clone();
    let title = demo.labels[0].id.clone();
    let bounds = old.bounds(&title).unwrap();
    demo.clipped = true;
    demo.scroll_offset.y = bounds.origin.y + bounds.size.height - 0.25;
    demo.update_geometry(old.viewport(), 1.25).unwrap();
    assert!(
        demo.snapshot
            .as_ref()
            .unwrap()
            .visible_bounds(&title)
            .is_some()
    );
    assert!(!demo.visible(&title));
    demo.pointer_moved(Point::new(34.0, 0.1));
    assert!(demo.hit(Point::new(34.0, 0.1)).is_none());
    assert!(tab(&mut demo));
    assert_eq!(demo.scene.focused(), Some(&title));
    assert!(demo.visible(&title));
    assert_eq!(demo.hovered.as_ref(), Some(&title));
}
