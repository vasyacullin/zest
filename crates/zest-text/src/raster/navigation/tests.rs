use super::*;
use crate::raster::{RasterOptions, Rasterizer};

fn paragraph(text: &str, width: f32) -> Paragraph {
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
