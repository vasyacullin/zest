use super::*;
use crate::raster::{RasterOptions, Rasterizer};
use unicode_segmentation::UnicodeSegmentation;

#[test]
fn vertical_arrows_terminate_at_document_edges_through_narrow_wrapped_unicode() {
    let text = "one e\u{301} 👩🏽‍💻 🇯🇵 العربية two\nthree\n";
    for width in [15.0, 30.0, 50.0, 95.0] {
        let paragraph = paragraph(text, width);
        for offset in text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([text.len()])
        {
            for movement in [Navigation::Up, Navigation::Down] {
                let mut state = paragraph
                    .layout_selection(Selection::caret(offset))
                    .unwrap();
                let mut stopped = false;
                for _ in 0..text.len() * 2 + 4 {
                    let next = paragraph.move_selection(&state, movement, true).unwrap();
                    assert_eq!(next.selection().anchor, offset);
                    assert!(crate::boundary(text, next.selection().head));
                    let old = paragraph.selection_caret(&state).unwrap();
                    let new = paragraph.selection_caret(&next).unwrap();
                    assert!(if movement == Navigation::Up {
                        new.y <= old.y
                    } else {
                        new.y >= old.y
                    });
                    if next.same_position(&state) {
                        stopped = true;
                        break;
                    }
                    state = next;
                }
                assert!(stopped);
                assert_eq!(
                    state.selection().head,
                    if movement == Navigation::Up {
                        0
                    } else {
                        text.len()
                    },
                    "{width}, {offset}, {movement:?}"
                );
            }
        }
    }
}

#[test]
fn arrow_navigation_uses_the_caret_center_at_overlapping_fractional_line_bounds() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let paragraph = paragraph_at_scale("abcdefghij\nx", 400.0, scale);
        let top = paragraph.layout_selection(Selection::caret(8)).unwrap();
        let bottom = paragraph
            .move_selection(&top, Navigation::Down, false)
            .unwrap();
        assert_eq!(bottom.selection(), Selection::caret(12));
        let back = paragraph
            .move_selection(&bottom, Navigation::Up, false)
            .unwrap();
        assert_eq!(back.selection(), Selection::caret(8));
        let empty = paragraph_at_scale("one\n", 400.0, scale);
        let end = empty.layout_selection(Selection::caret(4)).unwrap();
        let up = empty.move_selection(&end, Navigation::Up, false).unwrap();
        assert_eq!(up.selection(), Selection::caret(0));
    }
}

#[test]
fn page_motion_preserves_goal_column_across_short_lines_and_arrow_moves() {
    let text = "abcdefghij\nx\nabcdefghij\nx\nabcdefghij";
    let paragraph = paragraph(text, 400.0);
    let start = paragraph.layout_selection(Selection::caret(8)).unwrap();
    let height = paragraph.selection_caret(&start).unwrap().height;
    let short = paragraph
        .move_selection_page(&start, PageDirection::Down, height, false)
        .unwrap();
    assert_eq!(short.selection(), Selection::caret(12));
    let next = paragraph
        .move_selection(&short, Navigation::Down, false)
        .unwrap();
    assert_eq!(next.selection(), Selection::caret(21));
    let bottom = paragraph
        .move_selection_page(&next, PageDirection::Down, height * 2.0, true)
        .unwrap();
    assert_eq!(bottom.selection(), Selection::new(21, 34));
    let top = paragraph
        .move_selection_page(&bottom, PageDirection::Up, height * 4.0, true)
        .unwrap();
    assert_eq!(top.selection(), Selection::new(21, 8));
}

#[test]
fn page_edges_reach_document_bounds_and_keep_the_vertical_goal() {
    let paragraph = paragraph("abcdefghij\nx\nabcdefghij", 400.0);
    let start = paragraph.layout_selection(Selection::caret(8)).unwrap();
    let bottom = paragraph
        .move_selection_page(&start, PageDirection::Down, f64::MAX, false)
        .unwrap();
    assert_eq!(bottom.selection(), Selection::caret(21));
    let edge = paragraph
        .move_selection_page(&bottom, PageDirection::Down, 100.0, true)
        .unwrap();
    assert_eq!(edge.selection(), Selection::new(21, paragraph.text().len()));
    let again = paragraph
        .move_selection_page(&edge, PageDirection::Down, 100.0, true)
        .unwrap();
    assert!(again.same_position(&edge));
    let top = paragraph
        .move_selection_page(&edge, PageDirection::Up, f64::MAX, false)
        .unwrap();
    assert_eq!(top.selection(), Selection::caret(8));
    let edge = paragraph
        .move_selection_page(&top, PageDirection::Up, 100.0, true)
        .unwrap();
    assert_eq!(edge.selection(), Selection::new(8, 0));
    let bottom = paragraph
        .move_selection_page(&edge, PageDirection::Down, f64::MAX, false)
        .unwrap();
    assert_eq!(bottom.selection(), Selection::caret(21));
}

#[test]
fn page_heights_are_logical_at_fractional_dpi_and_tiny_pages_advance_one_line() {
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let paragraph =
            paragraph_at_scale("abcdefghij\nx\nabcdefghij\nx\nabcdefghij", 400.0, scale);
        let start = paragraph.layout_selection(Selection::caret(8)).unwrap();
        let height = paragraph.selection_caret(&start).unwrap().height;
        let moved = paragraph
            .move_selection_page(&start, PageDirection::Down, height * 2.0, false)
            .unwrap();
        assert_eq!(moved.selection(), Selection::caret(21), "scale {scale}");
        let tiny = paragraph
            .move_selection_page(&start, PageDirection::Down, f64::MIN_POSITIVE, false)
            .unwrap();
        assert_eq!(tiny.selection(), Selection::caret(12));
        let back = paragraph
            .move_selection_page(&tiny, PageDirection::Up, f64::MIN_POSITIVE, false)
            .unwrap();
        assert_eq!(back.selection(), Selection::caret(8));
    }
}

#[test]
fn page_motion_handles_wrap_affinity_rtl_graphemes_and_trailing_empty_lines() {
    for text in [
        "",
        "one\n",
        "one two three four five six seven",
        "one العربية e\u{301} 👩🏽‍💻 🇯🇵 two\nthree\n",
    ] {
        for width in [15.0, 30.0, 95.0] {
            let paragraph = paragraph(text, width);
            for offset in text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([text.len()])
            {
                for direction in [PageDirection::Up, PageDirection::Down] {
                    let mut state = paragraph
                        .layout_selection(Selection::caret(offset))
                        .unwrap();
                    let mut stopped = false;
                    for _ in 0..text.len() * 2 + 4 {
                        let next = paragraph
                            .move_selection_page(&state, direction, 45.0, true)
                            .unwrap();
                        assert_eq!(next.selection().anchor, offset);
                        assert!(crate::boundary(text, next.selection().head));
                        let old = paragraph.selection_caret(&state).unwrap();
                        let new = paragraph.selection_caret(&next).unwrap();
                        assert!(match direction {
                            PageDirection::Up => new.y <= old.y,
                            PageDirection::Down => new.y >= old.y,
                        });
                        paragraph.selection_geometry(&next).unwrap();
                        if next.same_position(&state) {
                            stopped = true;
                            break;
                        }
                        state = next;
                    }
                    assert!(stopped, "{text:?}, {offset}, {direction:?}");
                    assert_eq!(
                        state.selection().head,
                        match direction {
                            PageDirection::Up => 0,
                            PageDirection::Down => text.len(),
                        },
                        "{text:?}, {offset}, {direction:?}, caret {:?}, lines {:?}",
                        paragraph.selection_caret(&state).unwrap(),
                        paragraph
                            .layout
                            .lines()
                            .map(|l| (l.metrics().block_min_coord, l.metrics().block_max_coord))
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
fn invalid_page_sizes_and_foreign_snapshots_are_rejected_without_changing_selection() {
    let first = paragraph("one\ntwo", 400.0);
    let second = paragraph("one\ntwo", 400.0);
    let state = first.layout_selection(Selection::new(2, 0)).unwrap();
    for height in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            first.move_selection_page(&state, PageDirection::Down, height, true),
            Err(RasterError::InvalidOptions)
        ));
    }
    assert!(matches!(
        second.move_selection_page(&state, PageDirection::Down, 100.0, false),
        Err(RasterError::ForeignSelection)
    ));
    assert_eq!(state.selection(), Selection::new(2, 0));
    first.selection_caret(&state).unwrap();
}

#[test]
fn visual_word_steps_select_whole_words_and_collapse_existing_ranges() {
    let paragraph = paragraph("one two three", 400.0);
    let start = paragraph.layout_selection(Selection::caret(0)).unwrap();
    let first = paragraph
        .move_selection(&start, Navigation::WordRight, true)
        .unwrap();
    assert_eq!(first.selection(), Selection::new(0, 3));
    let second = paragraph
        .move_selection(&first, Navigation::WordRight, true)
        .unwrap();
    assert_eq!(second.selection(), Selection::new(0, 7));
    let collapsed = paragraph
        .move_selection(&second, Navigation::WordLeft, false)
        .unwrap();
    assert_eq!(collapsed.selection(), Selection::caret(0));
    let end = paragraph
        .move_selection(&second, Navigation::WordRight, false)
        .unwrap();
    assert_eq!(end.selection(), Selection::caret(7));
    let previous = paragraph
        .move_selection(&end, Navigation::WordLeft, true)
        .unwrap();
    assert_eq!(previous.selection(), Selection::new(7, 4));
    let previous = paragraph
        .move_selection(&previous, Navigation::WordLeft, true)
        .unwrap();
    assert_eq!(previous.selection(), Selection::new(7, 0));
}

#[test]
fn visual_word_left_in_rtl_advances_bytes_while_retaining_the_anchor() {
    let paragraph = paragraph("واحد اثنان", 400.0);
    let start = paragraph.layout_selection(Selection::caret(0)).unwrap();
    let left = paragraph
        .move_selection(&start, Navigation::WordLeft, true)
        .unwrap();
    assert_eq!(left.selection().anchor, 0);
    assert!(left.selection().head >= "واحد".len());
    assert!(
        paragraph.selection_caret(&left).unwrap().x < paragraph.selection_caret(&start).unwrap().x
    );
    let right = paragraph
        .move_selection(&left, Navigation::WordRight, true)
        .unwrap();
    assert_eq!(right.selection(), Selection::caret(0));
}

#[test]
fn visual_word_steps_terminate_on_graphemes_across_bidi_wraps_and_missing_emoji() {
    for text in [
        "",
        "  \n\t",
        "one العربية e\u{301} 👩🏽‍💻 🇯🇵 two\nthree",
        "\u{301}can't 日,日",
    ] {
        let paragraph = paragraph(text, 95.0);
        for offset in text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .chain([text.len()])
        {
            for movement in [Navigation::WordLeft, Navigation::WordRight] {
                let mut current = paragraph
                    .layout_selection(Selection::caret(offset))
                    .unwrap();
                let mut visited = vec![current.clone()];
                loop {
                    let next = paragraph.move_selection(&current, movement, true).unwrap();
                    assert_eq!(next.selection().anchor, offset);
                    assert!(crate::boundary(text, next.selection().head));
                    paragraph.selection_caret(&next).unwrap();
                    paragraph.selection_geometry(&next).unwrap();
                    if next.same_position(&current) {
                        break;
                    }
                    assert!(
                        !visited.iter().any(|state| next.same_position(state)),
                        "cycle: {text:?}, {movement:?}"
                    );
                    assert!(visited.len() <= text.len() * 2 + 4);
                    visited.push(next.clone());
                    current = next;
                }
            }
        }
    }
}

fn paragraph(text: &str, width: f32) -> Paragraph {
    paragraph_at_scale(text, width, 1.0)
}

fn paragraph_at_scale(text: &str, width: f32, scale: f32) -> Paragraph {
    let mut raster = Rasterizer::new();
    raster
        .register_font(include_bytes!("../../../../../assets/fonts/NotoSans.ttf").to_vec())
        .unwrap();
    raster
        .register_font(include_bytes!("../../../../../assets/fonts/NotoSansArabic.ttf").to_vec())
        .unwrap();
    raster
        .paragraph(
            text,
            &["Noto Sans", "Noto Sans Arabic"],
            RasterOptions {
                width,
                scale,
                ..Default::default()
            },
        )
        .unwrap()
}

#[test]
fn vertical_motion_retains_goal_column_across_a_short_line() {
    let text = "abcdefghij\nx\nabcdefghij";
    let paragraph = paragraph(text, 400.0);
    let start = paragraph.layout_selection(Selection::caret(8)).unwrap();
    let middle = paragraph
        .move_selection(&start, Navigation::Down, false)
        .unwrap();
    assert_eq!(middle.selection().head, 12);
    let bottom = paragraph
        .move_selection(&middle, Navigation::Down, false)
        .unwrap();
    assert_eq!(bottom.selection().head, 21);
    let middle = paragraph
        .move_selection(&bottom, Navigation::Up, false)
        .unwrap();
    let top = paragraph
        .move_selection(&middle, Navigation::Up, false)
        .unwrap();
    assert_eq!(top.selection().head, 8);
}

#[test]
fn vertical_goal_survives_grapheme_snapping_in_an_emoji_fallback_line() {
    let emoji = "👩🏽‍💻";
    let text = format!("abcdefghij\n{emoji}\nabcdefghij");
    let paragraph = paragraph(&text, 400.0);
    let bottom_start = "abcdefghij\n".len() + emoji.len() + 1;
    for column in 1..10 {
        let top = paragraph
            .layout_selection(Selection::caret(column))
            .unwrap();
        let middle = paragraph
            .move_selection(&top, Navigation::Down, false)
            .unwrap();
        assert!(crate::boundary(&text, middle.selection().head));
        let bottom = paragraph
            .move_selection(&middle, Navigation::Down, false)
            .unwrap();
        assert_eq!(
            bottom.selection().head,
            bottom_start + column,
            "column {column}"
        );
    }
}

#[test]
fn soft_wrap_affinity_changes_visual_line_without_changing_the_byte_offset() {
    let paragraph = paragraph("one two three four five", 95.0);
    let start = paragraph.layout_selection(Selection::caret(0)).unwrap();
    let end = paragraph
        .move_selection(&start, Navigation::LineEnd, false)
        .unwrap();
    let end_rect = paragraph.selection_caret(&end).unwrap();
    assert_eq!(end_rect.y, paragraph.selection_caret(&start).unwrap().y);
    let right = paragraph
        .move_selection(&end, Navigation::Right, false)
        .unwrap();
    assert_eq!(right.selection(), end.selection());
    assert!(!right.same_position(&end));
    assert!(paragraph.selection_caret(&right).unwrap().y > end_rect.y);
    let back = paragraph
        .move_selection(&right, Navigation::Left, false)
        .unwrap();
    assert!(back.same_position(&end));
    let point = paragraph
        .selection_at(end_rect.x + 0.1, end_rect.y + end_rect.height * 0.5)
        .unwrap();
    assert_eq!(point.selection(), end.selection());
    assert_eq!(paragraph.selection_caret(&point).unwrap().y, end_rect.y);
}

#[test]
fn visual_left_in_rtl_advances_logical_bytes_and_right_returns_to_the_same_caret() {
    let paragraph = paragraph("العربية", 400.0);
    let start = paragraph.layout_selection(Selection::caret(0)).unwrap();
    let left = paragraph
        .move_selection(&start, Navigation::Left, false)
        .unwrap();
    assert!(left.selection().head > 0);
    assert!(
        paragraph.selection_caret(&left).unwrap().x < paragraph.selection_caret(&start).unwrap().x
    );
    let right = paragraph
        .move_selection(&left, Navigation::Right, false)
        .unwrap();
    assert_eq!(right.selection().head, 0);
    assert!(
        (paragraph.selection_caret(&right).unwrap().x
            - paragraph.selection_caret(&start).unwrap().x)
            .abs()
            < 1e-4
    );
}

#[test]
fn mixed_bidi_horizontal_navigation_makes_visual_progress_and_terminates() {
    let text = "abc العربية xyz";
    let paragraph = paragraph(text, 400.0);
    let mut state = paragraph.layout_selection(Selection::caret(0)).unwrap();
    let mut stopped = false;
    for _ in 0..text.len() * 2 + 4 {
        let next = paragraph
            .move_selection(&state, Navigation::Right, false)
            .unwrap();
        assert!(crate::boundary(text, next.selection().head));
        assert!(
            paragraph.selection_caret(&next).unwrap().x + 1e-4
                >= paragraph.selection_caret(&state).unwrap().x
        );
        if next.same_position(&state) {
            stopped = true;
            break;
        }
        state = next;
    }
    assert!(stopped);
    assert_eq!(state.selection().head, text.len());
}

#[test]
fn navigation_never_splits_extended_graphemes_and_shift_keeps_anchor() {
    let ligature = paragraph("fi", 400.0);
    let start = ligature.layout_selection(Selection::caret(0)).unwrap();
    let between = ligature
        .move_selection(&start, Navigation::Right, false)
        .unwrap();
    assert_eq!(
        between.selection().head,
        1,
        "a ligature still has an editable grapheme boundary"
    );
    let text = "Ae\u{301} 👩🏽‍💻 fi 🇯🇵";
    let paragraph = paragraph(text, 400.0);
    let mut state = paragraph.layout_selection(Selection::caret(0)).unwrap();
    for _ in 0..text.len() * 2 + 4 {
        let next = paragraph
            .move_selection(&state, Navigation::Right, true)
            .unwrap();
        assert_eq!(next.selection().anchor, 0);
        assert!(crate::boundary(text, next.selection().head));
        if next.same_position(&state) {
            break;
        }
        state = next;
    }
    assert_eq!(state.selection(), Selection::new(0, text.len()));
    let collapsed = paragraph
        .move_selection(&state, Navigation::Left, false)
        .unwrap();
    assert!(collapsed.selection().is_caret());
    assert_eq!(collapsed.selection().head, 0);
}

#[test]
fn empty_paragraph_navigation_stays_valid_in_every_direction() {
    let paragraph = paragraph("", 400.0);
    let start = paragraph.layout_selection(Selection::caret(0)).unwrap();
    for movement in [
        Navigation::Left,
        Navigation::Right,
        Navigation::WordLeft,
        Navigation::WordRight,
        Navigation::Up,
        Navigation::Down,
        Navigation::LineStart,
        Navigation::LineEnd,
        Navigation::DocumentStart,
        Navigation::DocumentEnd,
    ] {
        for extend in [false, true] {
            let moved = paragraph.move_selection(&start, movement, extend).unwrap();
            assert_eq!(moved.selection(), Selection::caret(0));
            assert!(paragraph.selection_caret(&moved).unwrap().height > 0.0);
        }
    }
}

#[test]
fn foreign_snapshots_and_non_grapheme_offsets_are_rejected() {
    let first = paragraph("e\u{301}", 400.0);
    let second = paragraph("e\u{301}", 400.0);
    assert!(matches!(
        first.layout_selection(Selection::caret(1)),
        Err(RasterError::InvalidGraphemeOffset(1))
    ));
    let state = first.layout_selection(Selection::caret(0)).unwrap();
    assert_eq!(
        second.selection_caret(&state),
        Err(RasterError::ForeignSelection)
    );
    assert!(matches!(
        second.move_selection(&state, Navigation::Right, false),
        Err(RasterError::ForeignSelection)
    ));
    assert!(matches!(
        second.selection_geometry(&state),
        Err(RasterError::ForeignSelection)
    ));
    assert!(matches!(
        second.extend_selection(&state, 0.0, 0.0),
        Err(RasterError::ForeignSelection)
    ));
}
