use crate::{Axis, ScrollMetrics};
use zest_core::geometry::{Logical, Point, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarPart {
    BeforeThumb,
    Thumb,
    AfterThumb,
}

/// Shared logical geometry for painting/hitting a scrollbar. This owns no scene
/// state or pointer capture. The caller retains the grab offset during a drag
/// and cancels it when the track, viewport or content range changes.
///
/// ```
/// use zest_core::geometry::{Point, Rect};
/// use zest_layout::{Axis, ScrollMetrics, ScrollbarGeometry};
/// let bar = ScrollbarGeometry::new(Rect::from_xywh(0.0, 0.0, 8.0, 100.0),
///     Axis::Column, 100.0, ScrollMetrics {
///         offset: Point::new(0.0, 0.0), max_offset: Point::new(0.0, 300.0),
///     }, 16.0).unwrap();
/// let pointer = Point::new(4.0, 12.5);
/// let grab = bar.grab_offset(pointer).unwrap();
/// assert_eq!(bar.drag_offset(pointer, grab), Some(0.0));
/// assert_eq!(bar.drag_offset(Point::new(4.0, 1000.0), grab), Some(300.0));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarGeometry {
    track: Rect<Logical>,
    thumb: Rect<Logical>,
    axis: Axis,
    viewport_extent: f64,
    offset: f64,
    max_offset: f64,
}
impl ScrollbarGeometry {
    /// Row is horizontal, Column vertical. Returns None for empty/invalid tracks,
    /// invalid selected-axis metrics, or no overflow. Finite oversized offsets
    /// clamp to the range; the minimum thumb size clamps to the track length.
    pub fn new(
        track: Rect<Logical>,
        axis: Axis,
        viewport_extent: f64,
        metrics: ScrollMetrics,
        min_thumb: f64,
    ) -> Option<Self> {
        let (offset, max_offset, length) = match axis {
            Axis::Row => (metrics.offset.x, metrics.max_offset.x, track.size.width),
            Axis::Column => (metrics.offset.y, metrics.max_offset.y, track.size.height),
        };
        if track.is_empty()
            || !viewport_extent.is_finite()
            || viewport_extent <= 0.0
            || !offset.is_finite()
            || offset < 0.0
            || !max_offset.is_finite()
            || max_offset <= 0.0
            || !min_thumb.is_finite()
            || min_thumb < 0.0
        {
            return None;
        }
        // Avoid overflow in viewport + range for very large finite metrics.
        let fraction = if max_offset >= viewport_extent {
            let ratio = viewport_extent / max_offset;
            ratio / (1.0 + ratio)
        } else {
            1.0 / (1.0 + max_offset / viewport_extent)
        };
        let thumb_length = (length * fraction).max(min_thumb.min(length)).min(length);
        if thumb_length <= 0.0 {
            return None;
        }
        let offset = offset.min(max_offset);
        let position = (length - thumb_length) * (offset / max_offset);
        let thumb = match axis {
            Axis::Row => Rect::from_xywh(
                track.origin.x + position,
                track.origin.y,
                thumb_length,
                track.size.height,
            ),
            Axis::Column => Rect::from_xywh(
                track.origin.x,
                track.origin.y + position,
                track.size.width,
                thumb_length,
            ),
        };
        Some(Self {
            track,
            thumb,
            axis,
            viewport_extent,
            offset,
            max_offset,
        })
    }
    pub fn track(self) -> Rect<Logical> {
        self.track
    }
    pub fn thumb(self) -> Rect<Logical> {
        self.thumb
    }
    pub fn viewport_extent(self) -> f64 {
        self.viewport_extent
    }
    pub fn max_offset(self) -> f64 {
        self.max_offset
    }
    pub fn offset(self) -> f64 {
        self.offset
    }
    pub fn hit_test(self, point: Point<Logical>) -> Option<ScrollbarPart> {
        if !self.track.contains(point) {
            return None;
        }
        if self.thumb.contains(point) {
            return Some(ScrollbarPart::Thumb);
        }
        let before = match self.axis {
            Axis::Row => point.x < self.thumb.origin.x,
            Axis::Column => point.y < self.thumb.origin.y,
        };
        Some(if before {
            ScrollbarPart::BeforeThumb
        } else {
            ScrollbarPart::AfterThumb
        })
    }
    pub fn page_offset(self, part: ScrollbarPart) -> f64 {
        let step = self.viewport_extent.min(self.max_offset);
        match part {
            ScrollbarPart::BeforeThumb => (self.offset - step).max(0.0),
            ScrollbarPart::Thumb => self.offset,
            ScrollbarPart::AfterThumb => (self.offset + step).min(self.max_offset),
        }
    }
    /// Pointer-to-thumb-start distance captured at press time. Uses the original
    /// grab location so pressing within a thumb never jumps the scroll position.
    pub fn grab_offset(self, point: Point<Logical>) -> Option<f64> {
        if self.hit_test(point) != Some(ScrollbarPart::Thumb) {
            return None;
        }
        Some(match self.axis {
            Axis::Row => point.x - self.thumb.origin.x,
            Axis::Column => point.y - self.thumb.origin.y,
        })
    }
    /// Map a captured pointer, including positions outside the track, to a bounded
    /// content offset. Invalid pointers/grab offsets return None. A minimum-size
    /// thumb filling a tiny track retains the current offset without division.
    pub fn drag_offset(self, point: Point<Logical>, grab: f64) -> Option<f64> {
        let (pointer, start, length, thumb_length) = match self.axis {
            Axis::Row => (
                point.x,
                self.track.origin.x,
                self.track.size.width,
                self.thumb.size.width,
            ),
            Axis::Column => (
                point.y,
                self.track.origin.y,
                self.track.size.height,
                self.thumb.size.height,
            ),
        };
        if !point.x.is_finite()
            || !point.y.is_finite()
            || !grab.is_finite()
            || grab < 0.0
            || grab > thumb_length
        {
            return None;
        }
        let travel = length - thumb_length;
        Some(if travel <= 0.0 {
            self.offset
        } else {
            ((pointer - start - grab).clamp(0.0, travel) / travel) * self.max_offset
        })
    }
}

#[cfg(test)]
mod tests;
