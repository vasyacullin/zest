use super::*;
use winit::{
    event::MouseScrollDelta,
    keyboard::{Key, ModifiersState, NamedKey},
};
use zest::layout::ScrollbarPart;

fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    demo.key(&Key::Named(NamedKey::F4), true, false);
    demo
}
fn center(bounds: Rect<Logical>) -> Point<Logical> {
    Point::new(
        bounds.origin.x + bounds.size.width * 0.5,
        bounds.origin.y + bounds.size.height * 0.5,
    )
}
fn start(demo: &mut LayoutDemo) -> Point<Logical> {
    let point = center(demo.scrollbar().unwrap().thumb());
    demo.pointer_moved(point);
    assert!(demo.pointer_press());
    assert!(demo.scrollbar_dragging());
    point
}

#[test]
fn thumb_drag_keeps_its_grab_location_clamps_outside_and_never_activates_underlying_nodes() {
    let mut demo = demo();
    let old = demo.snapshot.as_ref().unwrap().clone();
    let point = start(&mut demo);
    assert_eq!(demo.scroll_offset.y, 0.0);
    assert!(!demo.pointer_press());
    assert!(!demo.key_event(&Key::Named(NamedKey::F4), true, true, false));
    assert!(!demo.key(&Key::Named(NamedKey::Escape), true, true));
    assert!(demo.scrollbar_dragging());
    assert!(!demo.pointer_moved(point));
    assert!(old.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert!(demo.pointer_moved(Point::new(point.x, 1000.0)));
    assert_eq!(demo.scroll_offset.y, demo.scrollbar().unwrap().max_offset());
    assert!(demo.scrollbar_dragging() && demo.hovered.is_none());
    demo.pointer_left();
    assert!(demo.scrollbar_dragging());
    assert!(demo.pointer_moved(Point::new(point.x, -1000.0)));
    assert_eq!(demo.scroll_offset.y, 0.0);
    assert!(demo.pointer_release());
    assert!(!demo.scrollbar_dragging() && !demo.activation.pointer_down());
    assert!(demo.selected.is_none());
    let title = demo.labels[0].id.clone();
    demo.pointer_moved(center(
        demo.snapshot.as_ref().unwrap().bounds(&title).unwrap(),
    ));
    demo.pointer_press();
    demo.pointer_release();
    assert_eq!(demo.selected.as_ref(), Some(&title));
}

#[test]
fn track_press_pages_once_and_does_not_turn_into_a_drag_until_the_next_gesture() {
    let mut demo = demo();
    let bar = demo.scrollbar().unwrap();
    let point = Point::new(
        center(bar.track()).x,
        bar.track().origin.y + bar.track().size.height - 1.0,
    );
    assert_eq!(bar.hit_test(point), Some(ScrollbarPart::AfterThumb));
    demo.pointer_moved(point);
    assert!(demo.pointer_press());
    assert_eq!(
        demo.scroll_offset.y,
        bar.page_offset(ScrollbarPart::AfterThumb)
    );
    assert!(!demo.scrollbar_dragging());
    assert!(!demo.pointer_press());
    assert!(!demo.pointer_release());
    assert!(demo.selected.is_none());
    let bar = demo.scrollbar().unwrap();
    let before = Point::new(center(bar.track()).x, bar.track().origin.y + 0.1);
    assert_eq!(bar.hit_test(before), Some(ScrollbarPart::BeforeThumb));
    demo.pointer_moved(before);
    demo.pointer_press();
    demo.pointer_release();
    assert_eq!(demo.scroll_offset.y, 0.0);
}

#[test]
fn geometry_focus_modal_keyboard_wheel_and_lifecycle_changes_cancel_scrollbar_capture() {
    for reason in 0..10 {
        let mut demo = demo();
        start(&mut demo);
        match reason {
            0 => {
                demo.update_geometry(Size::new(300.0, 450.0), 1.25).unwrap();
            }
            1 => {
                demo.update_geometry(Size::new(240.0, 450.0), 1.5).unwrap();
            }
            2 => {
                demo.focus_window(false);
                demo.focus_window(true);
            }
            3 => {
                demo.key(&Key::Named(NamedKey::F3), true, false);
            }
            4 => {
                demo.set_enabled(&demo.root.clone(), false);
            }
            5 => {
                demo.key(&Key::Named(NamedKey::F4), true, false);
            }
            6 => {
                assert!(demo.key(&Key::Named(NamedKey::Escape), true, false));
            }
            7 => {
                demo.cancel_pointer();
            }
            8 => {
                demo.modifiers(ModifiersState::CONTROL);
            }
            _ => {
                demo.wheel(
                    MouseScrollDelta::LineDelta(0.0, -1.0),
                    ScaleFactor::new(1.25).unwrap(),
                )
                .unwrap();
            }
        }
        assert!(!demo.scrollbar_dragging(), "cancellation reason {reason}");
        assert!(!demo.pointer_release());
        assert!(demo.selected.is_none());
        assert!(!demo.activation.pointer_down());
    }
}

#[test]
fn unchanged_geometry_retains_capture_while_reconciliation_cancels_it() {
    let mut demo = demo();
    start(&mut demo);
    assert!(!demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap());
    assert!(demo.scrollbar_dragging());
    demo.scene
        .update_children(&demo.root, |ui| {
            ui.button("footer", "Layout follows text");
            ui.button("body", demo.labels[1].source);
            ui.button("title", demo.labels[0].source);
        })
        .unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    assert!(!demo.scrollbar_dragging());
    assert!(!demo.pointer_release());
}

#[test]
fn nonoverflowing_disabled_modal_and_tiny_columns_hide_the_overlay() {
    let mut demo = demo();
    assert!(demo.scrollbar().is_some());
    demo.key(&Key::Named(NamedKey::F3), true, false);
    assert!(demo.scrollbar().is_none());
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    assert!(demo.scrollbar().is_some());
    demo.set_enabled(&demo.root.clone(), false);
    assert!(demo.scrollbar().is_none());
    demo.set_enabled(&demo.root.clone(), true);
    demo.scene
        .update_children(&demo.root, |ui| {
            ui.button("footer", "Footer");
        })
        .unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    assert!(demo.scrollbar().is_none());
    demo.update_geometry(Size::new(40.0, 450.0), 1.25).unwrap();
    assert!(demo.scrollbar().is_none());
    demo.update_geometry(Size::new(0.0, 0.0), 1.25).unwrap();
    assert!(demo.scrollbar().is_none());
}

#[test]
fn overlay_paint_and_hit_geometry_share_idle_hover_and_pressed_states() {
    let mut demo = demo();
    let bar = demo.scrollbar().unwrap();
    let thumb = center(bar.thumb());
    assert!(demo.hit(thumb).is_none());
    assert_eq!(
        demo.paint().unwrap().quads().last().unwrap().color,
        [0.2, 0.35, 0.5, 1.0]
    );
    demo.pointer_moved(thumb);
    assert_eq!(
        demo.paint().unwrap().quads().last().unwrap().color,
        [0.3, 0.5, 0.7, 1.0]
    );
    demo.pointer_press();
    assert_eq!(
        demo.paint().unwrap().quads().last().unwrap().color,
        [0.2, 0.6, 1.0, 1.0]
    );
    demo.pointer_release();
    assert_eq!(
        demo.paint().unwrap().quads().last().unwrap().color,
        [0.3, 0.5, 0.7, 1.0]
    );
}
