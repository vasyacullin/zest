use super::*;
use winit::keyboard::{Key, NamedKey};

fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    demo
}
fn center(bounds: Rect<Logical>) -> Point<Logical> {
    Point::new(
        bounds.origin.x + bounds.size.width * 0.5,
        bounds.origin.y + bounds.size.height * 0.5,
    )
}

#[test]
fn repeated_retry_and_completed_frames_reuse_commands_without_consuming_input_work() {
    let mut demo = demo();
    let first = demo.cached_paint().unwrap();
    assert_eq!(*first, demo.paint().unwrap());
    assert!(demo.scene.needs_paint());
    for status in [
        RenderStatus::Skipped,
        RenderStatus::Suspended,
        RenderStatus::NeedsResize,
    ] {
        demo.finish_frame(status, true);
        assert!(Arc::ptr_eq(&first, &demo.cached_paint().unwrap()));
        assert!(demo.scene.needs_paint());
    }
    demo.finish_frame(RenderStatus::Presented, true);
    assert!(!demo.scene.needs_paint());
    demo.scene.invalidate_paint(&demo.footer);
    assert!(Arc::ptr_eq(&first, &demo.cached_paint().unwrap()));
}

#[test]
fn hover_press_selection_modal_tint_geometry_and_reset_change_cache_inputs() {
    let mut demo = demo();
    let mut previous = demo.cached_paint().unwrap();
    for action in 0..8 {
        match action {
            0 => {
                demo.pointer_moved(center(
                    demo.snapshot
                        .as_ref()
                        .unwrap()
                        .bounds(&demo.footer)
                        .unwrap(),
                ));
            }
            1 => {
                demo.pointer_press();
            }
            2 => {
                demo.pointer_release();
            }
            3 => {
                demo.key(&Key::Named(NamedKey::F6), true, false);
            }
            4 => {
                demo.key(&Key::Named(NamedKey::F3), true, false);
            }
            5 => {
                demo.tint = [0.5, 0.8, 1.0, 1.0];
            }
            6 => {
                demo.update_geometry(Size::new(300.0, 450.0), 1.25).unwrap();
            }
            _ => {
                demo.reset_gpu();
            }
        }
        let next = demo.cached_paint().unwrap();
        assert!(!Arc::ptr_eq(&previous, &next), "action {action}");
        assert_eq!(*next, demo.paint().unwrap());
        assert!(Arc::ptr_eq(&next, &demo.cached_paint().unwrap()));
        previous = next;
    }
}

#[test]
fn scrollbar_hover_capture_release_and_pointer_motion_have_correct_cache_identity() {
    let mut demo = demo();
    demo.key(&Key::Named(NamedKey::F4), true, false);
    let idle = demo.cached_paint().unwrap();
    let point = center(demo.scrollbar().unwrap().thumb());
    demo.pointer_moved(point);
    let hover = demo.cached_paint().unwrap();
    assert!(!Arc::ptr_eq(&idle, &hover));
    demo.pointer_moved(Point::new(point.x, point.y + 1.0));
    assert!(Arc::ptr_eq(&hover, &demo.cached_paint().unwrap()));
    demo.pointer_press();
    let pressed = demo.cached_paint().unwrap();
    assert!(!Arc::ptr_eq(&hover, &pressed));
    demo.pointer_left(); // Logical drag remains active; visual input stays equal.
    assert!(Arc::ptr_eq(&pressed, &demo.cached_paint().unwrap()));
    demo.pointer_release();
    let released = demo.cached_paint().unwrap();
    assert!(!Arc::ptr_eq(&pressed, &released));
    assert_eq!(*released, demo.paint().unwrap());
}
