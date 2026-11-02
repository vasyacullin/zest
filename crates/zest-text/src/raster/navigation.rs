use super::{Affinity, Cursor, Paragraph, RasterError, TextRect};
use crate::Selection;
use std::sync::Arc;

/// Layout-aware movement; line start/end follow logical text order within the
/// current displayed line. Left/right follow visual cluster order; word variants
/// follow visual word boundaries. Unextended horizontal moves collapse ranges
/// toward the requested visual edge before moving further.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Navigation {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

/// Direction for viewport-sized movement through displayed lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageDirection {
    Up,
    Down,
}

/// Selection tied to an immutable paragraph snapshot. Retains caret affinity
/// at bidi/wrap boundaries and the desired horizontal position for consecutive
/// vertical moves. Reconstruct it after replacing the paragraph snapshot.
#[derive(Clone, Debug)]
pub struct LayoutSelection {
    identity: Arc<()>,
    inner: parley::Selection,
    // Keep the vertical goal independently of grapheme normalization. A font
    // fallback can expose interior stops inside an extended emoji grapheme;
    // snapping the public endpoints must not replace the original layout-pixel X.
    vertical_goal: Option<f64>,
}
impl LayoutSelection {
    pub fn selection(&self) -> Selection {
        Selection::new(self.inner.anchor().index(), self.inner.focus().index())
    }
    /// Compares visual endpoints, ignoring the saved vertical goal column.
    pub fn same_position(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.inner.anchor() == other.inner.anchor()
            && self.inner.focus() == other.inner.focus()
    }
}
impl Paragraph {
    /// Resolves model offsets into a state with canonical initial affinity.
    /// Both offsets must be extended grapheme boundaries. Use selection_at to
    /// retain the actual clicked side of a boundary instead.
    pub fn layout_selection(&self, selection: Selection) -> Result<LayoutSelection, RasterError> {
        for offset in [selection.anchor, selection.head] {
            if !crate::boundary(&self.text, offset) {
                return Err(RasterError::InvalidGraphemeOffset(offset));
            }
        }
        self.wrap_selection(parley::Selection::new(
            Cursor::from_byte_index(&self.layout, selection.anchor, Affinity::Downstream),
            Cursor::from_byte_index(&self.layout, selection.head, Affinity::Downstream),
        ))
    }
    /// Hit testing which retains the clicked side of a bidi or soft-wrap boundary.
    pub fn selection_at(&self, x: f64, y: f64) -> Result<LayoutSelection, RasterError> {
        let cursor = self.point_cursor(x, y)?;
        self.wrap_selection(parley::Selection::new(cursor, cursor))
    }
    /// Retains the existing anchor while hit testing a new focus. Resets the
    /// vertical goal column, as pointer movement starts a new navigation sequence.
    pub fn extend_selection(
        &self,
        anchor: &LayoutSelection,
        x: f64,
        y: f64,
    ) -> Result<LayoutSelection, RasterError> {
        self.validate_selection(anchor)?;
        self.wrap_selection(anchor.inner.extend(self.point_cursor(x, y)?))
    }
    /// Moves without mutating the paragraph or the model. Consecutive vertical
    /// steps retain their goal X even if a fallback glyph requires grapheme
    /// snapping. Other moves reset that goal. Foreign snapshot states are rejected.
    pub fn move_selection(
        &self,
        selection: &LayoutSelection,
        movement: Navigation,
        extend: bool,
    ) -> Result<LayoutSelection, RasterError> {
        self.validate_selection(selection)?;
        if matches!(movement, Navigation::Up | Navigation::Down) {
            let current = self.cursor_line(selection.inner.focus());
            let (target, direction) = if movement == Navigation::Up {
                (current.saturating_sub(1), PageDirection::Up)
            } else {
                (
                    current
                        .saturating_add(1)
                        .min(self.layout.len().saturating_sub(1)),
                    PageDirection::Down,
                )
            };
            return self.vertical_selection(
                selection,
                target,
                (target == current).then_some(direction),
                extend,
            );
        }
        let move_once = |current: parley::Selection| match movement {
            Navigation::Left => current.previous_visual(&self.layout, extend),
            Navigation::Right => current.next_visual(&self.layout, extend),
            Navigation::WordLeft if !extend && !current.is_collapsed() => {
                current.previous_visual(&self.layout, false)
            }
            Navigation::WordRight if !extend && !current.is_collapsed() => {
                current.next_visual(&self.layout, false)
            }
            Navigation::WordLeft => current.previous_visual_word(&self.layout, extend),
            Navigation::WordRight => current.next_visual_word(&self.layout, extend),
            Navigation::Up => current.previous_line(&self.layout, extend),
            Navigation::Down => current.next_line(&self.layout, extend),
            Navigation::LineStart => current.line_start(&self.layout, extend),
            Navigation::LineEnd => current.line_end(&self.layout, extend),
            Navigation::DocumentStart | Navigation::DocumentEnd => {
                let offset = if movement == Navigation::DocumentStart {
                    0
                } else {
                    self.text.len()
                };
                let cursor = Cursor::from_byte_index(&self.layout, offset, Affinity::Downstream);
                if extend {
                    current.extend(cursor)
                } else {
                    parley::Selection::new(cursor, cursor)
                }
            }
        };
        let mut current = selection.inner;
        // A shaped cluster can be finer than a Unicode grapheme (e.g. emoji
        // fallback). Skip interior horizontal stops without splitting the model's
        // graphemes. Keep affinity-only transitions at valid byte boundaries.
        for _ in 0..self.text.len().saturating_mul(2).saturating_add(4) {
            let candidate = move_once(current);
            if !matches!(
                movement,
                Navigation::Left | Navigation::Right | Navigation::WordLeft | Navigation::WordRight
            ) || crate::boundary(&self.text, candidate.focus().index())
            {
                return self.wrap_selection(candidate);
            }
            if candidate.focus() == current.focus() {
                return Ok(selection.clone());
            }
            current = candidate;
        }
        Err(RasterError::InvalidGraphemeOffset(current.focus().index()))
    }
    /// Moves approximately one viewport height in logical pixels, using actual
    /// displayed-line metrics. A positive finite height is required. Moves at
    /// least one line when possible and clamps to the first/last line. Another
    /// outward step on that line reaches its text edge. Retains the desired X
    /// across page and Up/Down moves, including short lines and grapheme snapping.
    /// Shift-style extension preserves the anchor. Foreign snapshots are rejected.
    pub fn move_selection_page(
        &self,
        selection: &LayoutSelection,
        direction: PageDirection,
        height: f64,
        extend: bool,
    ) -> Result<LayoutSelection, RasterError> {
        self.validate_selection(selection)?;
        if !height.is_finite() || height <= 0.0 {
            return Err(RasterError::InvalidOptions);
        }
        let last = self.layout.len().saturating_sub(1);
        let current = self.cursor_line(selection.inner.focus());
        let Some(line) = self.layout.get(current) else {
            return Ok(selection.clone());
        };
        let metrics = line.metrics();
        let center = f64::from(metrics.block_min_coord + metrics.block_max_coord) * 0.5;
        let distance = height * f64::from(self.bitmap.scale);
        let target_y = match direction {
            PageDirection::Up => center - distance,
            PageDirection::Down => center + distance,
        };
        // Extreme finite heights may overflow the intermediate distance to
        // infinity; comparisons still saturate to the first/last layout line.
        let target = self
            .layout
            .lines()
            .position(|line| target_y < f64::from(line.metrics().block_max_coord))
            .unwrap_or(last);
        let target = match direction {
            PageDirection::Up => target.min(current.saturating_sub(1)),
            PageDirection::Down => target.max(current.saturating_add(1).min(last)),
        };
        self.vertical_selection(
            selection,
            target,
            (target == current).then_some(direction),
            extend,
        )
    }
    fn cursor_line(&self, cursor: Cursor) -> usize {
        let rect = cursor.geometry(&self.layout, 0.0);
        // Neighboring f32 line bounds can overlap by a few ULPs. The caret's
        // center identifies its visual line without depending on that seam.
        let y = ((rect.y0 + rect.y1) * 0.5) as f32;
        self.layout
            .lines()
            .position(|line| y < line.metrics().block_max_coord)
            .unwrap_or(self.layout.len().saturating_sub(1))
    }
    fn vertical_selection(
        &self,
        selection: &LayoutSelection,
        target: usize,
        edge: Option<PageDirection>,
        extend: bool,
    ) -> Result<LayoutSelection, RasterError> {
        let Some(line) = self.layout.get(target) else {
            return Ok(selection.clone());
        };
        let goal = selection
            .vertical_goal
            .unwrap_or_else(|| selection.inner.focus().geometry(&self.layout, 0.0).x0);
        let metrics = line.metrics();
        let y = (metrics.block_min_coord + metrics.block_max_coord) * 0.5;
        let cursor = Cursor::from_point(&self.layout, goal as f32, y);
        let candidate = parley::Selection::new(cursor, cursor);
        let candidate = match edge {
            Some(PageDirection::Up) => candidate.line_start(&self.layout, false),
            Some(PageDirection::Down) => candidate.line_end(&self.layout, false),
            None => candidate,
        };
        let normalized = self.wrap_selection(candidate)?;
        let mut focus = normalized.inner.focus();
        // At a wrapped bidi boundary, point hit testing can choose the side
        // whose caret geometry belongs to the neighboring line. Prefer the
        // other affinity when it puts the same grapheme boundary on our target.
        if self.cursor_line(focus) != target {
            let affinity = if focus.affinity() == Affinity::Upstream {
                Affinity::Downstream
            } else {
                Affinity::Upstream
            };
            let other = Cursor::from_byte_index(&self.layout, focus.index(), affinity);
            if self.cursor_line(other) == target {
                focus = other;
            }
        }
        let inner = if extend {
            selection.inner.extend(focus)
        } else {
            parley::Selection::new(focus, focus)
        };
        let mut result = self.wrap_selection(inner)?;
        result.vertical_goal = Some(goal);
        Ok(result)
    }
    /// Logical caret geometry honoring the focus cursor's affinity.
    pub fn selection_caret(&self, selection: &LayoutSelection) -> Result<TextRect, RasterError> {
        self.validate_selection(selection)?;
        Ok(self.rect(
            selection
                .inner
                .focus()
                .geometry(&self.layout, self.bitmap.scale),
        ))
    }
    /// Logical highlight rectangles honoring both endpoint affinities.
    pub fn selection_geometry(
        &self,
        selection: &LayoutSelection,
    ) -> Result<Vec<TextRect>, RasterError> {
        self.validate_selection(selection)?;
        Ok(selection
            .inner
            .geometry(&self.layout)
            .into_iter()
            .map(|(rect, _)| self.rect(rect))
            .collect())
    }
    fn point_cursor(&self, x: f64, y: f64) -> Result<Cursor, RasterError> {
        // Reuse the public finite-coordinate checks before preserving affinity.
        self.hit_test(x, y)?;
        Ok(Cursor::from_point(
            &self.layout,
            (x * f64::from(self.bitmap.scale)) as f32,
            (y * f64::from(self.bitmap.scale)) as f32,
        ))
    }
    fn wrap_selection(&self, inner: parley::Selection) -> Result<LayoutSelection, RasterError> {
        let normalize = |cursor: Cursor| -> Result<Cursor, RasterError> {
            if crate::boundary(&self.text, cursor.index()) {
                return Ok(cursor);
            }
            let offset = crate::snap_forward(&self.text, cursor.index());
            let cursor = Cursor::from_byte_index(&self.layout, offset, cursor.affinity());
            if crate::boundary(&self.text, cursor.index()) {
                Ok(cursor)
            } else {
                Err(RasterError::InvalidGraphemeOffset(cursor.index()))
            }
        };
        let anchor = normalize(inner.anchor())?;
        let focus = normalize(inner.focus())?;
        // Preserve cursors and their affinities unless normalization moved an endpoint.
        let inner = if anchor == inner.anchor() && focus == inner.focus() {
            inner
        } else {
            parley::Selection::new(anchor, focus)
        };
        Ok(LayoutSelection {
            identity: self.identity.clone(),
            inner,
            vertical_goal: None,
        })
    }
    fn validate_selection(&self, selection: &LayoutSelection) -> Result<(), RasterError> {
        if Arc::ptr_eq(&self.identity, &selection.identity) {
            Ok(())
        } else {
            Err(RasterError::ForeignSelection)
        }
    }
}

#[cfg(test)]
mod tests;
