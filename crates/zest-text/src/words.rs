//! Logical word steps use UAX #29 segments, retaining punctuation and symbols.
//! Segment the full document so apostrophes/combining marks keep their context.
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn previous_word(text: &str, offset: usize) -> usize {
    let start = text
        .split_word_bound_indices()
        .take_while(|(start, _)| *start < offset)
        .filter(|(_, segment)| !segment.chars().all(char::is_whitespace))
        .map(|(start, _)| start)
        .last()
        .unwrap_or(0);
    // Word boundaries can be finer than extended graphemes. Snap outward so
    // deletion never leaves a partial cluster, including unusual control joins.
    text.grapheme_indices(true)
        .map(|(index, _)| index)
        .take_while(|index| *index <= start)
        .last()
        .unwrap_or(0)
}

pub(super) fn next_word(text: &str, offset: usize) -> usize {
    let end = text
        .split_word_bound_indices()
        .find(|(start, segment)| {
            start + segment.len() > offset && !segment.chars().all(char::is_whitespace)
        })
        .map(|(start, segment)| start + segment.len())
        .unwrap_or(text.len());
    crate::snap_forward(text, end)
}
