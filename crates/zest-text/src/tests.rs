use super::*;

#[test]
fn movement_and_deletion_treat_unicode_clusters_atomically() {
    let clusters = ["Ж", "e\u{301}", "👩🏽‍💻", "🇯🇵", "\r\n", "日", "ש"];
    let text = clusters.concat();
    let mut buffer = TextBuffer::new(&text);
    for cluster in clusters.iter().rev() {
        let old = buffer.selection().head;
        assert!(
            buffer
                .move_cursor(Movement::PreviousGrapheme, false)
                .unwrap()
        );
        assert_eq!(&buffer.text()[buffer.selection().head..old], *cluster);
    }
    assert!(
        !buffer
            .move_cursor(Movement::PreviousGrapheme, false)
            .unwrap()
    );
    for cluster in clusters {
        let old = buffer.selection().head;
        buffer.move_cursor(Movement::NextGrapheme, false).unwrap();
        assert_eq!(&buffer.text()[old..buffer.selection().head], cluster);
    }
    assert!(!buffer.move_cursor(Movement::NextGrapheme, false).unwrap());
    for cluster in clusters.iter().rev() {
        let old = buffer.text().len();
        assert!(buffer.backspace().unwrap());
        assert_eq!(buffer.text().len(), old - cluster.len());
    }
    assert_eq!(buffer.text(), "");
    assert!(!buffer.backspace().unwrap());
    assert!(!buffer.delete_forward().unwrap());
    for _ in clusters {
        buffer.undo().unwrap();
    }
    buffer.move_cursor(Movement::DocumentStart, false).unwrap();
    for cluster in clusters {
        let old = buffer.text().len();
        assert!(buffer.delete_forward().unwrap());
        assert_eq!(buffer.text().len(), old - cluster.len());
    }
}

#[test]
fn selection_extension_crosses_anchor_and_collapse_does_not_skip() {
    let mut buffer = TextBuffer::new("a日b");
    buffer.set_selection(Selection::caret(1)).unwrap();
    buffer.move_cursor(Movement::NextGrapheme, true).unwrap();
    assert_eq!(buffer.selected_text(), "日");
    buffer
        .move_cursor(Movement::PreviousGrapheme, true)
        .unwrap();
    buffer
        .move_cursor(Movement::PreviousGrapheme, true)
        .unwrap();
    assert_eq!(buffer.selection(), Selection::new(1, 0));
    buffer.move_cursor(Movement::NextGrapheme, false).unwrap();
    assert_eq!(buffer.selection(), Selection::caret(1));
    buffer.select_all().unwrap();
    buffer
        .move_cursor(Movement::PreviousGrapheme, false)
        .unwrap();
    assert_eq!(buffer.selection(), Selection::caret(0));
    buffer.move_cursor(Movement::DocumentEnd, true).unwrap();
    assert_eq!(buffer.selected_text(), "a日b");
}

#[test]
fn invalid_document_offsets_leave_text_selection_and_history_unchanged() {
    let mut buffer = TextBuffer::new("Жe\u{301}👩🏽‍💻");
    buffer.insert("!").unwrap();
    let before = buffer.selection();
    let history = buffer.history_stats();
    for offset in [1, 3, 4, 9, usize::MAX] {
        assert_eq!(
            buffer.set_selection(Selection::new(0, offset)),
            Err(EditError::InvalidBoundary { offset })
        );
        assert_eq!(buffer.selection(), before);
        assert_eq!(buffer.history_stats(), history);
        assert_eq!(buffer.text(), "Жe\u{301}👩🏽‍💻!");
    }
}

#[test]
fn replacement_undo_and_redo_restore_directional_selection() {
    let mut buffer = TextBuffer::new("one 👩🏽‍💻 two");
    let end = "one 👩🏽‍💻".len();
    let selection = Selection::new(end, 4);
    buffer.set_selection(selection).unwrap();
    buffer.insert("日").unwrap();
    assert_eq!(buffer.text(), "one 日 two");
    assert_eq!(buffer.selection(), Selection::caret(7));
    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "one 👩🏽‍💻 two");
    assert_eq!(buffer.selection(), selection);
    buffer.redo().unwrap();
    assert_eq!(buffer.text(), "one 日 two");
    assert_eq!(buffer.selection(), Selection::caret(7));
}

#[test]
fn joins_resegment_with_forward_affinity_and_undo_retains_exact_bytes() {
    // Inserting a base before a leading combining mark or a ZWJ between emoji
    // leaves the nominal caret inside a new cluster. Removing a separator can
    // also merge two regional indicators or CR/LF into one cluster.
    for (initial, selection, inserted, expected) in [
        ("\u{301}!", Selection::caret(0), "e", "e\u{301}!"),
        ("👩💻", Selection::caret("👩".len()), "\u{200d}", "👩‍💻"),
        ("🇯x🇵", Selection::new(4, 5), "", "🇯🇵"),
        ("\rx\n", Selection::new(1, 2), "", "\r\n"),
    ] {
        let mut buffer = TextBuffer::new(initial);
        buffer.set_selection(selection).unwrap();
        buffer.insert(inserted).unwrap();
        assert_eq!(buffer.text(), expected);
        assert_eq!(
            buffer.selection(),
            Selection::caret(expected.graphemes(true).next().unwrap().len())
        );
        buffer.undo().unwrap();
        assert_eq!(buffer.text(), initial);
        assert_eq!(buffer.selection(), selection);
        buffer.redo().unwrap();
        assert_eq!(buffer.text(), expected);
    }
}

#[test]
fn preedit_updates_are_transient_and_commit_is_one_undo_transaction() {
    let mut buffer = TextBuffer::new("a OLD z");
    let selection = Selection::new(5, 2);
    buffer.set_selection(selection).unwrap();
    buffer.set_preedit("に", Some(Selection::caret(3))).unwrap();
    buffer
        .set_preedit("にほん", Some(Selection::new(0, 9)))
        .unwrap();
    assert_eq!(buffer.text(), "a OLD z");
    assert_eq!(buffer.display_text(), "a にほん z");
    assert_eq!(buffer.selection(), selection);
    assert_eq!(buffer.history_stats(), HistoryStats::default());
    assert!(buffer.commit_preedit("日本"));
    assert_eq!(buffer.text(), "a 日本 z");
    assert!(buffer.preedit().is_none());
    assert_eq!(buffer.history_stats().undo_edits, 1);
    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "a OLD z");
    assert_eq!(buffer.selection(), selection);
    buffer.redo().unwrap();
    assert_eq!(buffer.text(), "a 日本 z");
}

#[test]
fn cancelling_or_empty_preedit_preserves_document_selection_and_redo() {
    let mut buffer = TextBuffer::new("日本");
    buffer.insert("!").unwrap();
    buffer.undo().unwrap();
    buffer.select_all().unwrap();
    let history = buffer.history_stats();
    let selection = buffer.selection();
    buffer.set_preedit("cancel", None).unwrap();
    assert_eq!(buffer.display_text(), "cancel");
    assert!(buffer.cancel_preedit());
    assert!(!buffer.cancel_preedit());
    buffer.set_preedit("cancel again", None).unwrap();
    assert!(buffer.set_preedit("", None).unwrap());
    assert!(!buffer.set_preedit("", None).unwrap());
    assert_eq!(buffer.text(), "日本");
    assert_eq!(buffer.display_text(), "日本");
    assert_eq!(buffer.selection(), selection);
    assert_eq!(buffer.history_stats(), history);
    buffer.redo().unwrap();
    assert_eq!(buffer.text(), "日本!");
}

#[test]
fn malformed_ime_offsets_are_atomic_but_scalar_boundaries_are_accepted() {
    let mut buffer = TextBuffer::new("base");
    // A scalar boundary inside the e + accent grapheme is valid for an IME.
    buffer
        .set_preedit("e\u{301}", Some(Selection::caret(1)))
        .unwrap();
    let preedit = buffer.preedit().cloned();
    for offset in [1, 4, usize::MAX] {
        assert_eq!(
            buffer.set_preedit("日", Some(Selection::caret(offset))),
            Err(EditError::InvalidPreeditCursor { offset })
        );
        assert_eq!(buffer.preedit(), preedit.as_ref());
    }
    assert_eq!(buffer.text(), "base");
    assert_eq!(buffer.history_stats(), HistoryStats::default());
}

#[test]
fn document_commands_cannot_change_an_active_composition_target() {
    let mut buffer = TextBuffer::new("base");
    buffer.insert("!").unwrap();
    buffer.select_all().unwrap();
    buffer.set_preedit("draft", None).unwrap();
    let expected = Err(EditError::CompositionActive);
    assert_eq!(buffer.insert("bad"), expected);
    assert_eq!(buffer.backspace(), expected);
    assert_eq!(buffer.delete_forward(), expected);
    assert_eq!(buffer.move_cursor(Movement::NextGrapheme, false), expected);
    assert_eq!(buffer.set_selection(Selection::caret(0)), expected);
    assert_eq!(buffer.select_all(), expected);
    assert_eq!(buffer.undo(), expected);
    assert_eq!(buffer.redo(), expected);
    assert_eq!(buffer.text(), "base!");
    assert_eq!(buffer.display_text(), "draft");
    assert_eq!(buffer.history_stats().undo_edits, 1);
}

#[test]
fn direct_empty_and_identical_ime_commits_have_defined_history() {
    let mut buffer = TextBuffer::new("x");
    assert!(buffer.commit_preedit("日"));
    assert_eq!(buffer.text(), "x日");
    buffer.select_all().unwrap();
    buffer.set_preedit("draft", None).unwrap();
    assert!(buffer.commit_preedit("x日"));
    assert_eq!(buffer.history_stats().undo_edits, 1);
    buffer.select_all().unwrap();
    buffer.commit_preedit("");
    assert_eq!(buffer.text(), "");
    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "x日");
    assert_eq!(buffer.selection(), Selection::new(0, "x日".len()));
}

#[test]
fn no_op_commands_preserve_redo_and_report_selection_changes() {
    let mut buffer = TextBuffer::new("a");
    buffer.insert("b").unwrap();
    buffer.undo().unwrap();
    assert!(!buffer.insert("").unwrap());
    assert!(!buffer.delete_forward().unwrap());
    assert!(!buffer.move_cursor(Movement::DocumentEnd, false).unwrap());
    buffer.select_all().unwrap();
    assert!(buffer.insert("a").unwrap()); // collapse selection, no text edit
    assert_eq!(buffer.history_stats().redo_edits, 1);
    assert!(!buffer.insert("").unwrap());
    buffer.redo().unwrap();
    assert_eq!(buffer.text(), "ab");
}

#[test]
fn a_new_edit_after_undo_discards_only_the_redo_branch() {
    let mut buffer = TextBuffer::default();
    for text in ["a", "b", "c"] {
        buffer.insert(text).unwrap();
    }
    buffer.undo().unwrap();
    buffer.undo().unwrap();
    buffer.insert("日").unwrap();
    assert_eq!(buffer.text(), "a日");
    assert_eq!(
        buffer.history_stats(),
        HistoryStats {
            undo_edits: 2,
            redo_edits: 0,
            payload_bytes: 4
        }
    );
    assert!(!buffer.redo().unwrap());
    buffer.undo().unwrap();
    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "");
}

#[test]
fn count_and_byte_budgets_evict_oldest_edits_and_cover_both_stacks() {
    for limits in [
        HistoryLimits {
            edits: 2,
            bytes: 100,
        },
        HistoryLimits {
            edits: 100,
            bytes: 2,
        },
    ] {
        let mut buffer = TextBuffer::with_history("", limits);
        for text in ["a", "b", "c"] {
            buffer.insert(text).unwrap();
        }
        assert_eq!(buffer.history_stats().payload_bytes, 2);
        buffer.undo().unwrap();
        buffer.undo().unwrap();
        assert!(!buffer.undo().unwrap());
        assert_eq!(buffer.text(), "a");
        assert_eq!(
            buffer.history_stats(),
            HistoryStats {
                undo_edits: 0,
                redo_edits: 2,
                payload_bytes: 2
            }
        );
        buffer.redo().unwrap();
        buffer.redo().unwrap();
        assert!(!buffer.redo().unwrap());
        assert_eq!(buffer.text(), "abc");
        buffer.clear_history();
        assert_eq!(buffer.history_stats(), HistoryStats::default());
        assert_eq!(buffer.text(), "abc");
    }
}

#[test]
fn oversized_or_disabled_history_breaks_the_delta_chain_safely() {
    for limits in [
        HistoryLimits { edits: 8, bytes: 2 },
        HistoryLimits {
            edits: 0,
            bytes: 100,
        },
        HistoryLimits { edits: 8, bytes: 0 },
    ] {
        let mut buffer = TextBuffer::with_history("", limits);
        buffer.insert("a").unwrap();
        buffer.insert("日").unwrap();
        assert_eq!(buffer.history_stats(), HistoryStats::default());
        assert!(!buffer.undo().unwrap());
        assert_eq!(buffer.text(), "a日");
        buffer.insert("!").unwrap();
        if limits.edits != 0 && limits.bytes != 0 {
            buffer.undo().unwrap();
            assert_eq!(buffer.text(), "a日");
        }
    }
    let mut buffer = TextBuffer::with_history(
        "ab",
        HistoryLimits {
            edits: 10,
            bytes: 3,
        },
    );
    buffer.select_all().unwrap();
    buffer.insert("cd").unwrap(); // removed + inserted = four payload bytes
    assert_eq!(buffer.history_stats(), HistoryStats::default());
}

#[test]
fn mixed_unicode_edit_sequences_round_trip_every_text_and_selection() {
    let samples = [
        "a", "Ж", "\u{301}", "🇯", "🇵", "👩", "\u{200d}", "💻", "\r\n", "日本", "שָׁ",
    ];
    for seed in 0..12 {
        let mut buffer = TextBuffer::with_history(
            "",
            HistoryLimits {
                edits: 512,
                bytes: 1_000_000,
            },
        );
        let mut edits = Vec::new();
        for step in 0..80 {
            let boundaries: Vec<_> = buffer
                .text()
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([buffer.text().len()])
                .collect();
            let a = boundaries[(seed + step * 7) % boundaries.len()];
            let b = boundaries[(seed * 3 + step * 11) % boundaries.len()];
            buffer.set_selection(Selection::new(a, b)).unwrap();
            let before = (buffer.text().to_owned(), buffer.selection());
            match step % 4 {
                0 => {
                    buffer.backspace().unwrap();
                }
                1 => {
                    buffer.delete_forward().unwrap();
                }
                _ => {
                    buffer
                        .insert(samples[(seed + step) % samples.len()])
                        .unwrap();
                }
            }
            assert!(boundary(buffer.text(), buffer.selection().head));
            if buffer.text() != before.0 {
                edits.push((before, (buffer.text().to_owned(), buffer.selection())));
            }
        }
        for (before, _) in edits.iter().rev() {
            assert!(buffer.undo().unwrap());
            assert_eq!((buffer.text().to_owned(), buffer.selection()), *before);
        }
        assert!(!buffer.undo().unwrap());
        for (_, after) in &edits {
            assert!(buffer.redo().unwrap());
            assert_eq!((buffer.text().to_owned(), buffer.selection()), *after);
        }
        assert!(!buffer.redo().unwrap());
    }
}
