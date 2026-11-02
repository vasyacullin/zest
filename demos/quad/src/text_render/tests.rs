use super::*;
use winit::dpi::PhysicalPosition;

fn editor(text: &str) -> TextDemo {
    let mut editor = TextDemo::new().unwrap();
    editor.focus(true);
    editor
        .edit(|input| {
            input.buffer.select_all()?;
            input.buffer.insert(text)
        })
        .unwrap();
    editor.update_view().unwrap();
    editor
}
fn at(editor: &TextDemo, offset: usize) -> Point<Logical> {
    let caret = editor.rect(editor.paragraph.caret(offset).unwrap());
    Point::new(
        caret.origin.x + 0.1,
        caret.origin.y + caret.size.height * 0.5,
    )
}

#[test]
fn paragraph_rejection_preserves_document_selection_history_and_snapshot() {
    let mut editor = TextDemo::new().unwrap();
    editor.focus(true);
    editor.edit(|input| input.buffer.insert("!")).unwrap();
    let before = editor.input.buffer.text().to_owned();
    let selection = editor.input.buffer.selection();
    let history = editor.input.buffer.history_stats();
    assert!(
        editor
            .edit(|input| input.buffer.insert(&"\n".repeat(5000)))
            .is_err()
    );
    assert_eq!(editor.input.buffer.text(), before);
    assert_eq!(editor.paragraph.text(), before);
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
    editor.edit(|input| input.buffer.undo()).unwrap();
    assert_eq!(editor.input.buffer.text(), SAMPLE);
}

#[test]
fn wheel_preserves_manual_position_selection_history_and_ime_across_redraws() {
    let mut editor = editor(&"one\n".repeat(12));
    let start = editor.scroll_y;
    let selection = editor.input.buffer.selection();
    let history = editor.input.buffer.history_stats();
    assert!(
        editor
            .wheel(
                MouseScrollDelta::LineDelta(0.0, 2.0),
                ScaleFactor::default()
            )
            .unwrap()
    );
    assert!((editor.scroll_y - (start - 72.0)).abs() < 1e-9);
    for _ in 0..3 {
        editor.update_view().unwrap();
    }
    assert!((editor.scroll_y - (start - 72.0)).abs() < 1e-9);
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
    editor.ime(&Ime::Enabled).unwrap();
    editor
        .ime(&Ime::Preedit("draft".into(), Some((5, 5))))
        .unwrap();
    editor.update_view().unwrap();
    let before = editor.scroll_y;
    editor
        .wheel(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 54.0)),
            ScaleFactor::new(1.5).unwrap(),
        )
        .unwrap();
    editor.update_view().unwrap();
    assert!((editor.scroll_y - (before - 36.0)).abs() < 1e-9);
    assert_eq!(editor.input.buffer.preedit().unwrap().text(), "draft");
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
    editor
        .ime(&Ime::Preedit("draft!".into(), Some((6, 6))))
        .unwrap();
    editor.update_view().unwrap();
    assert!(editor.scroll_y > before - 36.0); // a new composition update follows its caret again
    editor.focus(false);
    editor.update_view().unwrap();
    let selection = editor.input.buffer.selection();
    editor
        .wheel(
            MouseScrollDelta::LineDelta(0.0, 1.0),
            ScaleFactor::default(),
        )
        .unwrap();
    assert!(!editor.input.focused);
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
}

#[test]
fn wheel_clamps_both_axes_and_shift_redirects_vertical_lines() {
    let mut editor = editor(&"W".repeat(80));
    assert!(
        editor.scroll_limit().0 > 0.0,
        "fixture needs horizontal overflow"
    );
    editor
        .wheel(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(1e9, 1e9)),
            ScaleFactor::default(),
        )
        .unwrap();
    assert_eq!((editor.scroll_x, editor.scroll_y), (0.0, 0.0));
    editor.modifiers(ModifiersState::SHIFT);
    editor
        .wheel(
            MouseScrollDelta::LineDelta(0.0, -1.0),
            ScaleFactor::default(),
        )
        .unwrap();
    assert_eq!(editor.scroll_x, 36.0);
    assert_eq!(editor.scroll_y, 0.0);
    editor
        .wheel(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(-1e9, 0.0)),
            ScaleFactor::default(),
        )
        .unwrap();
    assert_eq!(editor.scroll_x, editor.scroll_limit().0);
    let before = (editor.scroll_x, editor.scroll_y);
    assert!(
        !editor
            .wheel(
                MouseScrollDelta::LineDelta(f32::NAN, 0.0),
                ScaleFactor::default()
            )
            .unwrap()
    );
    assert!(
        !editor
            .wheel(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, f64::INFINITY)),
                ScaleFactor::default()
            )
            .unwrap()
    );
    assert_eq!((editor.scroll_x, editor.scroll_y), before);
}

#[test]
fn navigation_reveals_a_manually_hidden_caret_even_without_changing_its_offset() {
    let mut editor = editor(&"one\n".repeat(12));
    let selection = editor.input.buffer.selection();
    let history = editor.input.buffer.history_stats();
    editor
        .wheel(
            MouseScrollDelta::LineDelta(0.0, 1e6),
            ScaleFactor::default(),
        )
        .unwrap();
    editor.update_view().unwrap();
    assert_eq!(editor.scroll_y, 0.0);
    assert!(
        !editor
            .key(&Key::Named(NamedKey::End), None, true, true)
            .unwrap()
            .changed
    );
    assert!(
        !editor
            .key(&Key::Named(NamedKey::End), None, false, false)
            .unwrap()
            .changed
    );
    assert_eq!(editor.scroll_y, 0.0);
    assert!(
        editor
            .key(&Key::Named(NamedKey::End), None, true, false)
            .unwrap()
            .changed
    );
    assert!(editor.scroll_y > 0.0);
    let scrolled = editor.scroll_y;
    editor.update_view().unwrap();
    assert_eq!(editor.scroll_y, scrolled);
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
}

#[test]
fn drag_keeps_its_anchor_when_crossing_graphemes_and_shift_extends_it() {
    let mut editor = editor("a e\u{301} Ж");
    editor.refresh(1.5).unwrap();
    let history = editor.input.buffer.history_stats();
    editor.begin_drag(at(&editor, 2), false).unwrap();
    editor
        .drag_to(at(&editor, editor.input.buffer.text().len()))
        .unwrap();
    assert_eq!(editor.input.buffer.selected_text(), "e\u{301} Ж");
    editor.drag_to(at(&editor, 0)).unwrap();
    assert_eq!(editor.input.buffer.selection(), Selection::new(2, 0));
    let before = editor.input.buffer.selection();
    assert!(editor.drag_to(Point::new(f64::NAN, 290.0)).is_err());
    assert_eq!(editor.input.buffer.selection(), before);
    assert_eq!(editor.input.buffer.history_stats(), history);
    editor.end_drag();
    assert!(!editor.drag_to(at(&editor, 2)).unwrap());
    editor
        .begin_drag(at(&editor, editor.input.buffer.text().len()), true)
        .unwrap();
    assert_eq!(editor.input.buffer.selection().anchor, 2);
    editor.drag_to(at(&editor, 0)).unwrap();
    assert_eq!(editor.input.buffer.selection(), Selection::new(2, 0));
    assert_eq!(editor.input.buffer.history_stats(), history);
}

#[test]
fn pointer_drag_selects_rtl_and_soft_wrapped_runs_and_the_rtl_start_caret_is_reachable() {
    for sample in ["العربية".to_owned(), "office Привет ".repeat(3)] {
        let mut editor = editor(&sample);
        editor.begin_drag(at(&editor, 0), false).unwrap();
        editor.drag_to(at(&editor, sample.len())).unwrap();
        assert_eq!(editor.input.buffer.selected_text(), sample);
        assert_eq!(editor.input.buffer.selection().anchor, 0);
        editor.end_drag();
        editor
            .key(
                &Key::Named(winit::keyboard::NamedKey::Home),
                None,
                true,
                false,
            )
            .unwrap();
        editor.update_view().unwrap();
        let caret = editor.rect(editor.paragraph.caret(0).unwrap());
        assert!(caret.origin.x >= VIEW.origin.x);
        assert!(caret.origin.x + caret.size.width <= VIEW.origin.x + VIEW_WIDTH + 1e-9);
        if sample.contains("office") {
            assert!(editor.paragraph.bitmap().metrics.lines > 1);
        }
    }
}

#[test]
fn stationary_outside_drag_scrolls_with_bounded_time_and_stops_at_document_edges() {
    let mut editor = editor(&"one\n".repeat(12));
    editor
        .wheel(
            MouseScrollDelta::LineDelta(0.0, 1e6),
            ScaleFactor::default(),
        )
        .unwrap();
    editor.begin_drag(at(&editor, 0), false).unwrap();
    editor.drag_to(Point::new(330.0, 390.0)).unwrap();
    assert!(editor.autoscroll_active());
    let history = editor.input.buffer.history_stats();
    let head = editor.input.buffer.selection().head;
    assert!(editor.drag_scroll(0.05).unwrap());
    assert!(editor.scroll_y > 0.0);
    assert!(editor.input.buffer.selection().head >= head);
    for _ in 0..100 {
        editor.drag_scroll(0.1).unwrap();
        editor.update_view().unwrap();
    }
    assert!(!editor.autoscroll_active());
    assert_eq!(editor.scroll_y, editor.scroll_limit().1);
    assert_eq!(editor.input.buffer.selection().anchor, 0);
    assert_eq!(
        editor.input.buffer.selection().head,
        editor.input.buffer.text().len()
    );
    assert_eq!(editor.input.buffer.history_stats(), history);
    editor.drag_to(Point::new(310.0, 100.0)).unwrap();
    assert!(editor.autoscroll_active());
    let before = editor.scroll_y;
    editor.drag_scroll(1000.0).unwrap();
    assert!((before - editor.scroll_y) <= 72.0001); // at most 0.1 s * 720 units/s after a stall
    for _ in 0..100 {
        editor.drag_scroll(0.1).unwrap();
    }
    assert_eq!(editor.scroll_y, 0.0);
    assert!(!editor.autoscroll_active());
    assert!(!editor.drag_scroll(f64::NAN).unwrap());
    editor.end_drag();
    assert!(!editor.autoscroll_active());
}

#[test]
fn wheel_during_drag_rehits_the_stationary_pointer_without_resetting_anchor() {
    let mut editor = editor(&"one\n".repeat(12));
    editor
        .wheel(
            MouseScrollDelta::LineDelta(0.0, 1e6),
            ScaleFactor::default(),
        )
        .unwrap();
    editor.begin_drag(at(&editor, 0), false).unwrap();
    editor.drag_to(Point::new(315.0, 320.0)).unwrap();
    let head = editor.input.buffer.selection().head;
    editor
        .wheel(
            MouseScrollDelta::LineDelta(0.0, -2.0),
            ScaleFactor::default(),
        )
        .unwrap();
    assert!(editor.input.buffer.selection().head > head);
    assert_eq!(editor.input.buffer.selection().anchor, 0);
    editor.update_view().unwrap();
    assert_eq!(editor.scroll_y, 72.0);
    editor.drag_to(Point::new(315.0, 340.0)).unwrap();
    assert!(!editor.autoscroll_active());
}

#[test]
fn focus_keyboard_and_composition_cancel_drag_without_committing_preedit() {
    let mut editor = editor(&"one\n".repeat(12));
    editor.begin_drag(Point::new(310.0, 300.0), false).unwrap();
    editor.drag_to(Point::new(310.0, 390.0)).unwrap();
    editor.focus(false);
    assert!(!editor.dragging() && !editor.autoscroll_active());
    let selection = editor.input.buffer.selection();
    editor.drag_to(Point::new(400.0, 200.0)).unwrap();
    assert_eq!(editor.input.buffer.selection(), selection);
    editor.focus(true);
    editor.begin_drag(Point::new(310.0, 300.0), false).unwrap();
    editor.ime(&Ime::Enabled).unwrap();
    editor
        .ime(&Ime::Preedit("draft".into(), Some((5, 5))))
        .unwrap();
    assert!(!editor.dragging());
    let committed = editor.input.buffer.text().to_owned();
    editor.begin_drag(Point::new(310.0, 300.0), false).unwrap();
    assert!(editor.input.buffer.preedit().is_none());
    assert_eq!(editor.input.buffer.text(), committed);
    editor
        .key(
            &Key::Named(winit::keyboard::NamedKey::Home),
            None,
            true,
            false,
        )
        .unwrap();
    assert!(!editor.dragging());
    editor.begin_drag(Point::new(310.0, 300.0), false).unwrap();
    editor.reset();
    assert!(!editor.dragging() && !editor.autoscroll_active());
}
