//! Window-independent UTF-8 editing. Cursor motion uses extended grapheme
//! clusters in logical string order; visual bidi navigation needs a layout engine.
//! Preedit is transient and never changes committed text or history. No font,
//! shaping, rasterization or OS event dependencies are required.
//!
//! ```
//! use zest_text::{Movement, TextBuffer};
//! let mut buffer = TextBuffer::new("Привет 👩🏽‍💻");
//! buffer.backspace()?; // delete the whole emoji cluster
//! assert_eq!(buffer.text(), "Привет ");
//! buffer.set_preedit("に", Some(zest_text::Selection::caret("に".len())))?;
//! assert_eq!(buffer.text(), "Привет ");
//! assert_eq!(buffer.display_text(), "Привет に");
//! buffer.commit_preedit("日本");
//! buffer.undo()?;
//! assert_eq!(buffer.text(), "Привет ");
//! buffer.move_cursor(Movement::DocumentStart, false)?;
//! # Ok::<(), zest_text::EditError>(())
//! ```
use std::{borrow::Cow, fmt, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

mod history;
use history::{Edit, History};
pub use history::{HistoryLimits, HistoryStats};

/// UTF-8 byte offsets, with direction preserved. A buffer accepts these only
/// at extended grapheme boundaries; IME cursors accept any scalar boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}
impl Selection {
    pub const fn caret(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
    pub fn is_caret(self) -> bool {
        self.anchor == self.head
    }
}

/// Logical document movement, independent of visual direction and wrapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    PreviousGrapheme,
    NextGrapheme,
    DocumentStart,
    DocumentEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditError {
    InvalidBoundary { offset: usize },
    InvalidPreeditCursor { offset: usize },
    CompositionActive,
}
impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBoundary { offset } => {
                write!(f, "offset {offset} is not a document grapheme boundary")
            }
            Self::InvalidPreeditCursor { offset } => {
                write!(f, "offset {offset} is not a preedit UTF-8 boundary")
            }
            Self::CompositionActive => {
                f.write_str("commit or cancel preedit before editing the document")
            }
        }
    }
}
impl std::error::Error for EditError {}

/// Uncommitted IME text replacing the buffer's current selection in the display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    text: String,
    cursor: Option<Selection>,
}
impl Preedit {
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Relative UTF-8 byte offsets supplied by the IME; None hides the cursor.
    pub fn cursor(&self) -> Option<Selection> {
        self.cursor
    }
}

/// A persistent editable document with bounded delta history. New buffers put
/// the caret at the end. Methods return whether text, selection or preedit
/// changed, allowing callers to avoid redundant redraws.
///
/// Document edits and cursor changes are rejected during preedit: the host must
/// commit or cancel composition first. An empty preedit cancels it. Each actual
/// replacement (including an IME commit) is one undo step; there is no typing
/// coalescing. Line endings and Unicode normalization are preserved verbatim.
#[derive(Debug)]
pub struct TextBuffer {
    text: String,
    selection: Selection,
    preedit: Option<Preedit>,
    history: History,
}
impl Default for TextBuffer {
    fn default() -> Self {
        Self::new("")
    }
}
impl TextBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self::with_history(text, HistoryLimits::default())
    }
    pub fn with_history(text: impl Into<String>, limits: HistoryLimits) -> Self {
        let text = text.into();
        Self {
            selection: Selection::caret(text.len()),
            text,
            preedit: None,
            history: History::new(limits),
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn selection(&self) -> Selection {
        self.selection
    }
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.range()]
    }
    pub fn preedit(&self) -> Option<&Preedit> {
        self.preedit.as_ref()
    }
    /// Display-only projection. Cursor offsets remain relative to committed
    /// text, while Preedit::cursor is relative to the preedit string.
    pub fn display_text(&self) -> Cow<'_, str> {
        match &self.preedit {
            None => Cow::Borrowed(&self.text),
            Some(preedit) => {
                let range = self.selection.range();
                let mut display = self.text.clone();
                display.replace_range(range, &preedit.text);
                Cow::Owned(display)
            }
        }
    }
    pub fn history_stats(&self) -> HistoryStats {
        self.history.stats()
    }
    pub fn clear_history(&mut self) {
        self.history.clear();
    }
    pub fn set_selection(&mut self, selection: Selection) -> Result<bool, EditError> {
        self.ensure_editable()?;
        for offset in [selection.anchor, selection.head] {
            if !boundary(&self.text, offset) {
                return Err(EditError::InvalidBoundary { offset });
            }
        }
        let changed = self.selection != selection;
        self.selection = selection;
        Ok(changed)
    }
    pub fn select_all(&mut self) -> Result<bool, EditError> {
        self.set_selection(Selection::new(0, self.text.len()))
    }
    /// Without extension, a previous/next movement collapses an existing range
    /// to its start/end. With extension, the anchor is retained when crossing it.
    pub fn move_cursor(&mut self, movement: Movement, extend: bool) -> Result<bool, EditError> {
        self.ensure_editable()?;
        let range = self.selection.range();
        let head = match movement {
            Movement::PreviousGrapheme if !extend && !self.selection.is_caret() => range.start,
            Movement::NextGrapheme if !extend && !self.selection.is_caret() => range.end,
            Movement::PreviousGrapheme => previous(&self.text, self.selection.head),
            Movement::NextGrapheme => next(&self.text, self.selection.head),
            Movement::DocumentStart => 0,
            Movement::DocumentEnd => self.text.len(),
        };
        self.set_selection(Selection::new(
            if extend { self.selection.anchor } else { head },
            head,
        ))
    }
    pub fn insert(&mut self, text: &str) -> Result<bool, EditError> {
        self.ensure_editable()?;
        Ok(self.replace(self.selection.range(), text))
    }
    pub fn backspace(&mut self) -> Result<bool, EditError> {
        self.ensure_editable()?;
        let mut range = self.selection.range();
        if self.selection.is_caret() {
            range.start = previous(&self.text, range.start);
        }
        Ok(self.replace(range, ""))
    }
    pub fn delete_forward(&mut self) -> Result<bool, EditError> {
        self.ensure_editable()?;
        let mut range = self.selection.range();
        if self.selection.is_caret() {
            range.end = next(&self.text, range.end);
        }
        Ok(self.replace(range, ""))
    }
    /// Validate cursor offsets before changing any state. IMEs can legitimately
    /// position a cursor inside a grapheme, but never inside a UTF-8 scalar.
    pub fn set_preedit(
        &mut self,
        text: impl Into<String>,
        cursor: Option<Selection>,
    ) -> Result<bool, EditError> {
        let text = text.into();
        if let Some(cursor) = cursor {
            for offset in [cursor.anchor, cursor.head] {
                if !text.is_char_boundary(offset) {
                    return Err(EditError::InvalidPreeditCursor { offset });
                }
            }
        }
        if text.is_empty() {
            return Ok(self.cancel_preedit());
        }
        let preedit = Preedit { text, cursor };
        let changed = self.preedit.as_ref() != Some(&preedit);
        self.preedit = Some(preedit);
        Ok(changed)
    }
    pub fn cancel_preedit(&mut self) -> bool {
        self.preedit.take().is_some()
    }
    /// Commit the supplied text, which may differ from the last preedit. Also
    /// accepts commits without a preceding preedit (some input methods do this).
    pub fn commit_preedit(&mut self, text: &str) -> bool {
        let composed = self.cancel_preedit();
        self.replace(self.selection.range(), text) || composed
    }
    pub fn undo(&mut self) -> Result<bool, EditError> {
        self.ensure_editable()?;
        let Some(edit) = self.history.undo.pop_back() else {
            return Ok(false);
        };
        self.text
            .replace_range(edit.start..edit.start + edit.inserted.len(), &edit.removed);
        self.selection = edit.before;
        self.history.redo.push(edit);
        Ok(true)
    }
    pub fn redo(&mut self) -> Result<bool, EditError> {
        self.ensure_editable()?;
        let Some(edit) = self.history.redo.pop() else {
            return Ok(false);
        };
        self.text
            .replace_range(edit.start..edit.start + edit.removed.len(), &edit.inserted);
        self.selection = edit.after;
        self.history.undo.push_back(edit);
        Ok(true)
    }
    fn ensure_editable(&self) -> Result<(), EditError> {
        if self.preedit.is_some() {
            Err(EditError::CompositionActive)
        } else {
            Ok(())
        }
    }
    fn replace(&mut self, range: Range<usize>, text: &str) -> bool {
        let before = self.selection;
        let text_changed = &self.text[range.clone()] != text;
        let removed = if text_changed {
            self.text[range.clone()].to_owned()
        } else {
            String::new()
        };
        self.text.replace_range(range.clone(), text);
        // Insertion/deletion can merge clusters across either join (combining
        // marks, regional indicators, ZWJ). Re-segment the complete new document
        // and use forward affinity instead of keeping an interior byte offset.
        self.selection = Selection::caret(snap_forward(&self.text, range.start + text.len()));
        if text_changed {
            self.history.record(Edit {
                start: range.start,
                removed,
                inserted: text.to_owned(),
                before,
                after: self.selection,
            });
        }
        text_changed || before != self.selection
    }
}

fn boundary(text: &str, offset: usize) -> bool {
    offset == text.len() || text.grapheme_indices(true).any(|(i, _)| i == offset)
}
fn previous(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(i, _)| i)
        .take_while(|&i| i < offset)
        .last()
        .unwrap_or(0)
}
fn next(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(i, _)| i)
        .find(|&i| i > offset)
        .unwrap_or(text.len())
}
fn snap_forward(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(i, _)| i)
        .find(|&i| i >= offset)
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests;
