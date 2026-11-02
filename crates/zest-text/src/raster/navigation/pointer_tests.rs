use super::tests::paragraph_at_scale;
use super::*;

fn point(paragraph: &Paragraph, offset: usize, rtl: bool) -> (f64, f64) {
    let rect = paragraph.caret(offset).unwrap();
    (
        rect.x + if rtl { -0.1 } else { 0.1 },
        rect.y + rect.height * 0.5,
    )
}

#[test]
fn word_hit_testing_selects_unicode_words_symbols_and_whitespace_without_splitting_graphemes() {
    let units = ["can't", ",", "Привет", "العربية", "e\u{301}", "👩🏽‍💻", "🇯🇵"];
    let text = units.join(" ");
    for scale in [1.0, 1.25, 1.5] {
        let paragraph = paragraph_at_scale(&text, 1000.0, scale);
        let mut offset = 0;
        for unit in units {
            let (x, y) = point(&paragraph, offset, unit == "العربية");
            let selected = paragraph
                .selection_at_granularity(x, y, SelectionGranularity::Word)
                .unwrap();
            assert_eq!(
                selected.selection().range(),
                offset..offset + unit.len(),
                "{unit}, scale {scale}"
            );
            assert!(crate::boundary(&text, selected.selection().anchor));
            assert!(crate::boundary(&text, selected.selection().head));
            offset += unit.len() + 1;
        }
    }
    let paragraph = paragraph_at_scale("one  two", 400.0, 1.0);
    let (x, y) = point(&paragraph, 3, false);
    let selected = paragraph
        .selection_at_granularity(x, y, SelectionGranularity::Word)
        .unwrap();
    assert_eq!(selected.selection().range(), 3..5);
    let text = "a 👩🏽‍💻 z";
    let paragraph = paragraph_at_scale(text, 400.0, 1.25);
    let begin = paragraph.caret(2).unwrap();
    let end = paragraph.caret("a 👩🏽‍💻".len()).unwrap();
    for step in 1..((end.x - begin.x) * 2.0) as u32 {
        let selected = paragraph
            .selection_at_granularity(
                begin.x + f64::from(step) * 0.5,
                begin.y + begin.height * 0.5,
                SelectionGranularity::Word,
            )
            .unwrap();
        assert_eq!(selected.selection().range(), 2.."a 👩🏽‍💻".len());
    }
}

#[test]
fn word_drag_retains_the_whole_initial_word_when_crossing_it() {
    let paragraph = paragraph_at_scale("one two three", 400.0, 1.25);
    let (x, y) = point(&paragraph, 4, false);
    let base = paragraph
        .selection_at_granularity(x, y, SelectionGranularity::Word)
        .unwrap();
    let (x, y) = point(&paragraph, 8, false);
    let right = paragraph
        .extend_selection_granularity(&base, x, y, SelectionGranularity::Word)
        .unwrap();
    assert_eq!(right.selection(), Selection::new(4, 13));
    let (x, y) = point(&paragraph, 0, false);
    let left = paragraph
        .extend_selection_granularity(&base, x, y, SelectionGranularity::Word)
        .unwrap();
    assert_eq!(left.selection(), Selection::new(7, 0));
    let (x, y) = point(&paragraph, 5, false);
    let inside = paragraph
        .extend_selection_granularity(&base, x, y, SelectionGranularity::Word)
        .unwrap();
    assert!(inside.same_position(&base));
}

#[test]
fn line_selection_includes_newlines_and_drag_extends_by_displayed_rows() {
    let paragraph = paragraph_at_scale("one\ntwo\n", 400.0, 1.25);
    let (x, y) = point(&paragraph, 4, false);
    let base = paragraph
        .selection_at_granularity(x, y, SelectionGranularity::Line)
        .unwrap();
    assert_eq!(base.selection(), Selection::new(4, 8));
    let (x, y) = point(&paragraph, 0, false);
    let left = paragraph
        .extend_selection_granularity(&base, x, y, SelectionGranularity::Line)
        .unwrap();
    assert_eq!(left.selection(), Selection::new(8, 0));
    let (x, y) = point(&paragraph, 8, false);
    let empty = paragraph
        .selection_at_granularity(x, y, SelectionGranularity::Line)
        .unwrap();
    assert_eq!(empty.selection(), Selection::caret(8));
    let crlf = paragraph_at_scale("one\r\ntwo\r\n", 400.0, 1.25);
    let (x, y) = point(&crlf, 5, false);
    let selected = crlf
        .selection_at_granularity(x, y, SelectionGranularity::Line)
        .unwrap();
    assert_eq!(selected.selection(), Selection::new(5, 10));
    for text in ["", "one two three four five", "العربية one e\u{301} 👩🏽‍💻 two"] {
        let paragraph = paragraph_at_scale(text, 95.0, 1.25);
        for line in paragraph.layout.lines() {
            let y = f64::from(line.metrics().block_min_coord + line.metrics().block_max_coord)
                * 0.5
                / 1.25;
            let state = paragraph
                .selection_at_granularity(5.0, y, SelectionGranularity::Line)
                .unwrap();
            let range = state.selection().range();
            assert!(
                range.start <= line.text_range().start.min(text.len())
                    && range.end >= line.text_range().end.min(text.len()),
                "{text:?}: selected {range:?}, row {:?}, y {y}",
                line.text_range()
            );
            assert!(crate::boundary(text, range.start) && crate::boundary(text, range.end));
            paragraph.selection_geometry(&state).unwrap();
        }
    }
}

#[test]
fn grouped_selection_rejects_nonfinite_points_and_foreign_drag_bases() {
    let first = paragraph_at_scale("one two", 400.0, 1.0);
    let second = paragraph_at_scale("one two", 400.0, 1.0);
    let base = first
        .selection_at_granularity(2.0, 10.0, SelectionGranularity::Word)
        .unwrap();
    for mode in [
        SelectionGranularity::Grapheme,
        SelectionGranularity::Word,
        SelectionGranularity::Line,
    ] {
        for (x, y) in [(f64::NAN, 0.0), (0.0, f64::INFINITY), (f64::MAX, 0.0)] {
            assert!(matches!(
                first.selection_at_granularity(x, y, mode),
                Err(RasterError::InvalidOptions)
            ));
            assert!(matches!(
                first.extend_selection_granularity(&base, x, y, mode),
                Err(RasterError::InvalidOptions)
            ));
        }
        assert!(matches!(
            second.extend_selection_granularity(&base, 0.0, 0.0, mode),
            Err(RasterError::ForeignSelection)
        ));
    }
    assert_eq!(base.selection(), Selection::new(0, 3));
}
