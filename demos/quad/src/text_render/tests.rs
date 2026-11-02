use super::*;
use crate::clipboard::MemoryClipboard;
use winit::dpi::PhysicalPosition;

#[test]
fn paging_scrolls_with_the_caret_and_preserves_selection_history_and_uploaded_layout() {
    let mut editor = editor(&"abcdefghij\n".repeat(16));
    editor
        .edit(|input| input.buffer.set_selection(Selection::caret(8)))
        .unwrap();
    editor.refresh(1.25).unwrap();
    editor.update_view().unwrap();
    editor.uploaded = true;
    let history = editor.input.buffer.history_stats();
    let snapshot = editor
        .paragraph
        .layout_selection(Selection::caret(8))
        .unwrap();
    let before = editor.rect(editor.current_caret().unwrap());
    assert!(
        editor
            .key(
                &Key::Named(NamedKey::PageDown),
                Some("incidental"),
                true,
                false
            )
            .unwrap()
            .changed
    );
    let after = editor.rect(editor.current_caret().unwrap());
    assert!((after.origin.y - before.origin.y).abs() < 1e-4);
    assert!((editor.scroll_y - VIEW_HEIGHT).abs() <= after.size.height * 0.5 + 1e-4);
    let anchor = editor.input.buffer.selection().head;
    editor.modifiers(ModifiersState::SHIFT);
    editor
        .key(&Key::Named(NamedKey::PageDown), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection().anchor, anchor);
    assert!(editor.input.buffer.selection().head > anchor);
    editor
        .key(&Key::Named(NamedKey::PageUp), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection(), Selection::caret(anchor));
    editor.modifiers(ModifiersState::empty());
    editor
        .key(&Key::Named(NamedKey::PageUp), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection(), Selection::caret(8));
    assert_eq!(editor.scroll_y, 0.0);
    assert!(editor.uploaded);
    assert_eq!(editor.input.buffer.history_stats(), history);
    assert!(editor.paragraph.selection_caret(&snapshot).is_ok());
}

#[test]
fn page_boundary_noop_reveals_a_manually_hidden_caret_without_changing_history() {
    let mut editor = editor(&"one\n".repeat(12));
    let selection = editor.input.buffer.selection();
    let history = editor.input.buffer.history_stats();
    editor
        .wheel(
            MouseScrollDelta::LineDelta(0.0, 1e6),
            ScaleFactor::default(),
        )
        .unwrap();
    assert_eq!(editor.scroll_y, 0.0);
    for (pressed, synthetic) in [(false, false), (true, true)] {
        assert!(
            !editor
                .key(
                    &Key::Named(NamedKey::PageDown),
                    Some("incidental"),
                    pressed,
                    synthetic
                )
                .unwrap()
                .changed
        );
        assert_eq!(editor.scroll_y, 0.0);
    }
    assert!(
        editor
            .key(&Key::Named(NamedKey::PageDown), None, true, false)
            .unwrap()
            .changed
    );
    assert!(editor.scroll_y > 0.0);
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
    assert!(
        !editor
            .key(&Key::Named(NamedKey::PageDown), None, true, false)
            .unwrap()
            .changed
    );
}

fn word_modifier() -> ModifiersState {
    if cfg!(target_os = "macos") {
        ModifiersState::ALT
    } else {
        ModifiersState::CONTROL
    }
}

#[test]
fn word_selection_keeps_the_paragraph_and_history_and_resets_vertical_goal() {
    let mut editor = editor("one two\nabcdefghij\nx\nabcdefghij");
    editor
        .edit(|input| input.buffer.set_selection(Selection::caret(0)))
        .unwrap();
    let snapshot = editor
        .paragraph
        .layout_selection(Selection::caret(0))
        .unwrap();
    let history = editor.input.buffer.history_stats();
    editor.uploaded = true;
    editor.modifiers(word_modifier() | ModifiersState::SHIFT);
    editor
        .key(&Key::Named(NamedKey::ArrowRight), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection(), Selection::new(0, 3));
    editor
        .key(&Key::Named(NamedKey::ArrowRight), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection(), Selection::new(0, 7));
    editor.modifiers(word_modifier());
    editor
        .key(&Key::Named(NamedKey::ArrowLeft), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection(), Selection::caret(0));
    assert_eq!(editor.input.buffer.history_stats(), history);
    assert!(editor.uploaded);
    assert!(editor.paragraph.selection_caret(&snapshot).is_ok());

    editor
        .edit(|input| input.buffer.set_selection(Selection::caret(16)))
        .unwrap();
    editor.modifiers(ModifiersState::empty());
    editor
        .key(&Key::Named(NamedKey::ArrowDown), None, true, false)
        .unwrap();
    editor.modifiers(word_modifier());
    editor
        .key(&Key::Named(NamedKey::ArrowLeft), None, true, false)
        .unwrap();
    let x = editor.current_caret().unwrap().x;
    editor.modifiers(ModifiersState::empty());
    editor
        .key(&Key::Named(NamedKey::ArrowDown), None, true, false)
        .unwrap();
    assert!((editor.current_caret().unwrap().x - x).abs() < 1e-4);
}

#[test]
fn no_op_word_deletion_reveals_the_caret_but_ignored_events_leave_manual_scroll() {
    let mut editor = editor(&"one\n".repeat(12));
    let selection = editor.input.buffer.selection();
    let history = editor.input.buffer.history_stats();
    editor
        .wheel(
            MouseScrollDelta::LineDelta(0.0, 1e6),
            ScaleFactor::default(),
        )
        .unwrap();
    assert_eq!(editor.scroll_y, 0.0);
    editor.modifiers(word_modifier());
    for (pressed, synthetic) in [(false, false), (true, true)] {
        let result = editor
            .key(&Key::Named(NamedKey::Delete), None, pressed, synthetic)
            .unwrap();
        assert!(!result.changed && !result.reveal_caret);
        assert_eq!(editor.scroll_y, 0.0);
    }
    assert!(
        editor
            .key(&Key::Named(NamedKey::Delete), None, true, false)
            .unwrap()
            .changed
    );
    assert!(editor.scroll_y > 0.0);
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
}

#[test]
fn word_deletion_layout_failure_retains_the_document_history_and_snapshot() {
    let text = format!("{}\n{}", "W".repeat(140), "W".repeat(140));
    let mut editor = editor(&text);
    let selection = Selection::new(141, 140);
    editor
        .edit(|input| input.buffer.set_selection(selection))
        .unwrap();
    let snapshot = editor.paragraph.layout_selection(selection).unwrap();
    let history = editor.input.buffer.history_stats();
    editor.modifiers(word_modifier());
    for key in [NamedKey::Backspace, NamedKey::Delete] {
        assert!(editor.key(&Key::Named(key), None, true, false).is_err());
        assert_eq!(editor.input.buffer.text(), text);
        assert_eq!(editor.input.buffer.selection(), selection);
        assert_eq!(editor.input.buffer.history_stats(), history);
        assert!(editor.paragraph.selection_caret(&snapshot).is_ok());
    }
}

#[test]
fn copying_a_reversed_unicode_selection_is_observational_and_empty_selection_does_not_write() {
    let mut editor = editor("a e\u{301} 👩🏽‍💻 z");
    let end = editor.input.buffer.text().len() - 2;
    editor
        .edit(|input| input.buffer.set_selection(Selection::new(end, 2)))
        .unwrap();
    editor.navigation = Some(
        editor
            .paragraph
            .layout_selection(editor.input.buffer.selection())
            .unwrap(),
    );
    let nav = editor.current_navigation().unwrap().clone();
    let selection = editor.input.buffer.selection();
    let history = editor.input.buffer.history_stats();
    let scroll = (editor.scroll_x, editor.scroll_y);
    let mut clipboard = MemoryClipboard::default();
    assert!(
        !editor
            .clipboard(ClipboardAction::Copy, &mut clipboard)
            .unwrap()
    );
    assert_eq!(clipboard.text.as_deref(), Some("e\u{301} 👩🏽‍💻"));
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
    assert_eq!((editor.scroll_x, editor.scroll_y), scroll);
    assert!(editor.current_navigation().unwrap().same_position(&nav));
    editor
        .edit(|input| input.buffer.set_selection(Selection::caret(0)))
        .unwrap();
    for action in [ClipboardAction::Copy, ClipboardAction::Cut] {
        assert!(!editor.clipboard(action, &mut clipboard).unwrap());
    }
    assert_eq!(clipboard.writes, 1);
    assert_eq!(clipboard.reads, 0);
}

#[test]
fn cut_commits_only_after_a_successful_write_and_undo_restores_direction() {
    let mut editor = editor("a Ж👩🏽‍💻 z");
    let selection = Selection::new(editor.input.buffer.text().len() - 2, 2);
    editor
        .edit(|input| input.buffer.set_selection(selection))
        .unwrap();
    let before = editor.input.buffer.text().to_owned();
    let history = editor.input.buffer.history_stats();
    let guard = editor.paragraph.layout_selection(selection).unwrap();
    let mut clipboard = MemoryClipboard {
        text: Some("keep".into()),
        fail_write: true,
        ..Default::default()
    };
    assert!(
        editor
            .clipboard(ClipboardAction::Cut, &mut clipboard)
            .is_err()
    );
    assert_eq!(editor.input.buffer.text(), before);
    assert_eq!(editor.input.buffer.selection(), selection);
    assert_eq!(editor.input.buffer.history_stats(), history);
    assert!(editor.paragraph.selection_caret(&guard).is_ok());
    assert_eq!(clipboard.text.as_deref(), Some("keep"));
    clipboard.fail_write = false;
    assert!(
        editor
            .clipboard(ClipboardAction::Cut, &mut clipboard)
            .unwrap()
    );
    assert_eq!(clipboard.text.as_deref(), Some("Ж👩🏽‍💻"));
    assert_eq!(editor.input.buffer.text(), "a  z");
    assert_eq!(
        editor.input.buffer.history_stats().undo_edits,
        history.undo_edits + 1
    );
    editor.edit(|input| input.buffer.undo()).unwrap();
    assert_eq!(editor.input.buffer.text(), before);
    assert_eq!(editor.input.buffer.selection(), selection);
    editor.edit(|input| input.buffer.redo()).unwrap();
    assert_eq!(editor.input.buffer.text(), "a  z");
}

#[test]
fn multiline_paste_normalizes_crlf_as_one_edit_and_restores_replaced_selection() {
    let mut editor = editor("a OLD z");
    let selection = Selection::new(5, 2);
    editor
        .edit(|input| input.buffer.set_selection(selection))
        .unwrap();
    let history = editor.input.buffer.history_stats();
    let mut clipboard = MemoryClipboard {
        text: Some("Привет\r\nالعربية\re\u{301}\t👩🏽‍💻".into()),
        ..Default::default()
    };
    editor
        .clipboard(ClipboardAction::Paste, &mut clipboard)
        .unwrap();
    let expected = "a Привет\nالعربية\ne\u{301}\t👩🏽‍💻 z";
    assert_eq!(editor.input.buffer.text(), expected);
    assert_eq!(
        editor.input.buffer.history_stats().undo_edits,
        history.undo_edits + 1
    );
    editor.edit(|input| input.buffer.undo()).unwrap();
    assert_eq!(editor.input.buffer.text(), "a OLD z");
    assert_eq!(editor.input.buffer.selection(), selection);
    editor.edit(|input| input.buffer.redo()).unwrap();
    assert_eq!(editor.input.buffer.text(), expected);
    assert_eq!(clipboard.reads, 1);
    assert_eq!(clipboard.writes, 0);
}

#[test]
fn failed_empty_invalid_and_over_budget_pastes_leave_the_document_and_snapshot_intact() {
    let mut editor = editor("keep");
    editor.edit(|input| input.buffer.select_all()).unwrap();
    let history = editor.input.buffer.history_stats();
    let selection = editor.input.buffer.selection();
    let guard = editor.paragraph.layout_selection(selection).unwrap();
    for text in [
        None,
        Some("a\0b".to_owned()),
        Some("x".repeat(MAX_PASTE_BYTES + 1)),
        Some("\n".repeat(5000)),
    ] {
        let mut clipboard = MemoryClipboard {
            text,
            ..Default::default()
        };
        assert!(
            editor
                .clipboard(ClipboardAction::Paste, &mut clipboard)
                .is_err()
        );
        assert_eq!(editor.input.buffer.text(), "keep");
        assert_eq!(editor.input.buffer.selection(), selection);
        assert_eq!(editor.input.buffer.history_stats(), history);
        assert!(editor.paragraph.selection_caret(&guard).is_ok());
        assert_eq!(clipboard.writes, 0);
    }
    let mut clipboard = MemoryClipboard {
        text: Some("".into()),
        ..Default::default()
    };
    assert!(
        !editor
            .clipboard(ClipboardAction::Paste, &mut clipboard)
            .unwrap()
    );
    assert_eq!(editor.input.buffer.selection(), selection);
    clipboard.fail_read = true;
    assert!(
        editor
            .clipboard(ClipboardAction::Paste, &mut clipboard)
            .is_err()
    );
    assert_eq!(editor.input.buffer.text(), "keep");
    assert_eq!(editor.input.buffer.history_stats(), history);
}

#[test]
fn cut_validates_layout_before_writing_and_rejects_lossy_nul_copy() {
    let text = format!("{}\n{}", "W".repeat(140), "W".repeat(140));
    let mut editor = editor(&text);
    editor
        .edit(|input| input.buffer.set_selection(Selection::new(140, 141)))
        .unwrap();
    let mut clipboard = MemoryClipboard {
        text: Some("keep".into()),
        ..Default::default()
    };
    assert!(
        editor
            .clipboard(ClipboardAction::Cut, &mut clipboard)
            .is_err()
    );
    assert_eq!(editor.input.buffer.text(), text);
    assert_eq!(clipboard.writes, 0);
    assert_eq!(clipboard.text.as_deref(), Some("keep"));
    editor
        .edit(|input| {
            input.buffer.select_all()?;
            input.buffer.insert("a\0b")?;
            input.buffer.select_all()
        })
        .unwrap();
    for action in [ClipboardAction::Copy, ClipboardAction::Cut] {
        assert!(editor.clipboard(action, &mut clipboard).is_err());
        assert_eq!(editor.input.buffer.text(), "a\0b");
        assert_eq!(clipboard.writes, 0);
    }
}

#[test]
fn clipboard_does_no_io_during_composition_or_without_text_focus() {
    let mut editor = editor("keep");
    editor.edit(|input| input.buffer.select_all()).unwrap();
    editor.ime(&Ime::Enabled).unwrap();
    editor.ime(&Ime::Preedit("draft".into(), None)).unwrap();
    let mut clipboard = MemoryClipboard {
        text: Some("paste".into()),
        ..Default::default()
    };
    for action in [
        ClipboardAction::Copy,
        ClipboardAction::Cut,
        ClipboardAction::Paste,
    ] {
        assert!(!editor.clipboard(action, &mut clipboard).unwrap());
    }
    assert_eq!(editor.input.buffer.display_text(), "draft");
    editor.focus(false);
    for action in [
        ClipboardAction::Copy,
        ClipboardAction::Cut,
        ClipboardAction::Paste,
    ] {
        assert!(!editor.clipboard(action, &mut clipboard).unwrap());
    }
    assert_eq!(editor.input.buffer.text(), "keep");
    assert_eq!((clipboard.reads, clipboard.writes), (0, 0));
}

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
fn vertical_navigation_keeps_goal_column_and_shift_anchor_without_editing_history() {
    let mut editor = editor("abcdefghij\nx\nabcdefghij");
    editor
        .edit(|input| input.buffer.set_selection(Selection::caret(8)))
        .unwrap();
    let history = editor.input.buffer.history_stats();
    let snapshot_guard = editor
        .paragraph
        .layout_selection(Selection::caret(8))
        .unwrap();
    editor
        .key(&Key::Named(NamedKey::ArrowDown), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection().head, 12);
    editor
        .key(&Key::Named(NamedKey::ArrowDown), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection().head, 21);
    editor.modifiers(ModifiersState::SHIFT);
    editor
        .key(&Key::Named(NamedKey::ArrowUp), None, true, false)
        .unwrap();
    editor
        .key(&Key::Named(NamedKey::ArrowUp), None, true, false)
        .unwrap();
    assert_eq!(editor.input.buffer.selection(), Selection::new(21, 8));
    assert_eq!(editor.input.buffer.history_stats(), history);
    assert!(editor.paragraph.selection_caret(&snapshot_guard).is_ok());
}

#[test]
fn wrap_affinity_survives_render_geometry_hit_testing_and_affinity_only_keys() {
    let mut editor = editor(&"one two three four five ".repeat(4));
    editor
        .edit(|input| input.buffer.set_selection(Selection::caret(0)))
        .unwrap();
    editor
        .key(&Key::Named(NamedKey::End), None, true, false)
        .unwrap();
    let head = editor.input.buffer.selection().head;
    let end = editor.current_caret().unwrap();
    assert_eq!(end.y, 0.0);
    assert!(
        editor
            .key(&Key::Named(NamedKey::ArrowRight), None, true, false)
            .unwrap()
            .changed
    );
    assert_eq!(editor.input.buffer.selection().head, head);
    assert!(editor.current_caret().unwrap().y > end.y);
    assert!(
        editor
            .key(&Key::Named(NamedKey::ArrowLeft), None, true, false)
            .unwrap()
            .changed
    );
    assert_eq!(editor.input.buffer.selection().head, head);
    assert_eq!(editor.current_caret().unwrap().y, end.y);
    let rect = editor.rect(editor.current_caret().unwrap());
    editor
        .begin_drag(
            Point::new(rect.origin.x + 0.1, rect.origin.y + rect.size.height * 0.5),
            false,
        )
        .unwrap();
    editor.end_drag();
    assert_eq!(editor.input.buffer.selection().head, head);
    assert_eq!(editor.current_caret().unwrap().y, end.y);
}

#[test]
fn rebuild_invalidates_layout_navigation_and_cancels_drag() {
    let mut editor = editor("one\ntwo\nthree");
    editor
        .key(&Key::Named(NamedKey::ArrowUp), None, true, false)
        .unwrap();
    let old = editor.current_navigation().unwrap().clone();
    editor.begin_drag(Point::new(310.0, 300.0), false).unwrap();
    editor.refresh(1.5).unwrap();
    assert!(editor.navigation.is_none());
    assert!(!editor.dragging());
    assert_eq!(
        editor.paragraph.selection_caret(&old),
        Err(zest::text::raster::RasterError::ForeignSelection)
    );
    editor
        .key(&Key::Named(NamedKey::ArrowDown), None, true, false)
        .unwrap();
    assert!(editor.current_caret().unwrap().height > 0.0);
    editor.ime(&Ime::Enabled).unwrap();
    editor.ime(&Ime::Preedit("é".into(), Some((2, 2)))).unwrap();
    let display = editor.input.buffer.display_text().into_owned();
    assert!(editor.current_navigation().is_none());
    assert!(
        !editor
            .key(&Key::Named(NamedKey::ArrowUp), None, true, false)
            .unwrap()
            .changed
    );
    assert_eq!(editor.input.buffer.display_text(), display);
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
        editor.modifiers(if cfg!(target_os = "macos") {
            ModifiersState::SUPER
        } else {
            ModifiersState::CONTROL
        });
        editor
            .key(
                &Key::Named(winit::keyboard::NamedKey::Home),
                None,
                true,
                false,
            )
            .unwrap();
        editor.modifiers(ModifiersState::empty());
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
