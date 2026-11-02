use super::*;
use winit::{
    dpi::PhysicalPosition,
    event::MouseScrollDelta,
    keyboard::{Key, ModifiersState, NamedKey},
};
fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    demo.key(&Key::Named(NamedKey::F4), true, false);
    demo.pointer_moved(Point::new(34.0, 40.0));
    demo
}
fn wheel(demo: &mut LayoutDemo, delta: MouseScrollDelta) -> bool {
    demo.wheel(delta, ScaleFactor::new(1.25).unwrap()).unwrap()
}

#[test]
fn line_and_pixel_wheels_use_logical_offsets_and_boundary_noops_preserve_the_snapshot() {
    let mut demo = demo();
    assert!(wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, -1.0)));
    assert_eq!(demo.scroll_offset, Point::new(0.0, 36.0));
    assert!(wheel(
        &mut demo,
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -45.0))
    ));
    assert_eq!(demo.scroll_offset, Point::new(0.0, 72.0));
    assert!(wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, -1e6)));
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    assert_eq!(
        demo.scroll_offset,
        snapshot.scroll(&demo.root).unwrap().max_offset
    );
    assert!(!wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, -1e6)));
    assert!(snapshot.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert!(wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, 1e6)));
    assert_eq!(demo.scroll_offset, Point::new(0.0, 0.0));
    assert!(!wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, 1.0)));
}

#[test]
fn wheel_rejects_inactive_outside_modified_nonfinite_and_unclipped_requests() {
    let mut demo = demo();
    let delta = MouseScrollDelta::LineDelta(0.0, -1.0);
    assert!(!wheel(
        &mut demo,
        MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, f64::NAN))
    ));
    assert!(!wheel(
        &mut demo,
        MouseScrollDelta::LineDelta(0.0, f32::INFINITY)
    ));
    demo.modifiers(ModifiersState::CONTROL);
    assert!(!wheel(&mut demo, delta));
    demo.modifiers(ModifiersState::SHIFT);
    assert!(!wheel(&mut demo, delta)); // No horizontal overflow in this preview.
    demo.modifiers(ModifiersState::empty());
    demo.pointer_moved(Point::new(20.0, CLIP_HEIGHT));
    assert!(!wheel(&mut demo, delta));
    demo.pointer_left();
    assert!(!wheel(&mut demo, delta));
    demo.focus_window(false);
    assert!(!wheel(&mut demo, delta));
    demo.focus_window(true);
    demo.pointer_moved(Point::new(34.0, 40.0));
    demo.key(&Key::Named(NamedKey::F4), true, false);
    assert!(!wheel(&mut demo, delta));
    demo.key(&Key::Named(NamedKey::F4), true, false);
    let root = demo.root.clone();
    demo.set_enabled(&root, false);
    assert!(!wheel(&mut demo, delta));
}

#[test]
fn scrolling_cancels_gestures_rehits_a_stationary_pointer_and_filters_tab_by_projected_bounds() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let body = demo.labels[1].id.clone();
    let footer = demo.footer.clone();
    demo.pointer_press();
    assert!(demo.scene.state(&title).unwrap().pressed());
    let content = demo.labels[1].content.clone();
    assert!(wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, -1e6)));
    assert!(!demo.pointer_release());
    assert!(!demo.scene.state(&title).unwrap().pressed());
    assert!(demo.scene.focused().is_none());
    assert_eq!(demo.hovered.as_ref(), Some(&body));
    assert_eq!(demo.labels[1].content, content);
    let tab = Key::Named(NamedKey::Tab);
    demo.key(&tab, true, false);
    assert_eq!(demo.scene.focused(), Some(&body));
    demo.key(&tab, true, false);
    assert_eq!(demo.scene.focused(), Some(&footer));
    let bounds = demo.snapshot.as_ref().unwrap().bounds(&footer).unwrap();
    let point = Point::new(bounds.origin.x + 10.0, bounds.origin.y + 6.0);
    assert_eq!(demo.hit(point), Some(footer.clone()));
    demo.pointer_moved(point);
    demo.pointer_press();
    demo.pointer_release();
    assert_eq!(demo.selected.as_ref(), Some(&footer));
}

#[test]
fn resize_and_partial_removal_clamp_scroll_and_disabling_clipping_resets_the_offset() {
    let mut demo = demo();
    wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, -1e6));
    let old = demo.scroll_offset;
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    assert!(demo.scroll_offset.y < old.y);
    assert_eq!(
        demo.scroll_offset,
        demo.snapshot
            .as_ref()
            .unwrap()
            .scroll(&demo.root)
            .unwrap()
            .offset
    );
    demo.scene
        .update_children(&demo.root, |ui| {
            ui.button("footer", "Footer");
        })
        .unwrap();
    demo.update_geometry(Size::new(480.0, 450.0), 1.25).unwrap();
    assert_eq!(demo.scroll_offset, Point::new(0.0, 0.0));
    demo.key(&Key::Named(NamedKey::F4), true, false);
    assert!(!demo.clipped);
    assert_eq!(demo.scroll_offset, Point::new(0.0, 0.0));
}

#[test]
fn modal_wheel_routing_stays_on_eligible_content_and_ignores_blocked_padding() {
    let mut demo = demo();
    let body = demo.labels[1].id.clone();
    demo.key(&Key::Named(NamedKey::F3), true, false);
    assert!(demo.modal_active());
    assert!(!wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, -1.0))); // Pointer on blocked title.
    demo.pointer_moved(Point::new(34.0, 90.0));
    assert_eq!(demo.hit(Point::new(34.0, 90.0)), Some(body));
    assert!(wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, -1.0)));
    assert!(demo.modal_active());
    demo.pointer_moved(Point::new(10.0, 10.0));
    assert!(!wheel(&mut demo, MouseScrollDelta::LineDelta(0.0, -1.0)));
}
