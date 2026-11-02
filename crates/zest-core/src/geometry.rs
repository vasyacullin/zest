//! Coordinates carry their space in the type. Physical coordinates retain
//! subpixel precision; a platform adapter must choose how to round surface sizes.
//!
//! ```compile_fail
//! use zest_core::geometry::{Logical, Physical, Point, Rect};
//! let bounds = Rect::<Logical>::from_xywh(0.0, 0.0, 100.0, 50.0);
//! bounds.contains(Point::<Physical>::new(10.0, 10.0));
//! ```
//!
//! ```compile_fail
//! use zest_core::geometry::{Canvas, Logical, Point, Rect};
//! let bounds = Rect::<Logical>::from_xywh(0.0, 0.0, 100.0, 50.0);
//! bounds.contains(Point::<Canvas>::new(10.0, 10.0));
//! ```

use std::{fmt, marker::PhantomData};

/// Device-independent window coordinates, before the window's DPI scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Logical {}

/// Device pixels, including fractional positions used during rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Physical {}

/// Document coordinates, before canvas pan/zoom and the window's DPI scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Canvas {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point<Space> {
    pub x: f64,
    pub y: f64,
    space: PhantomData<Space>,
}

impl<Space> Point<Space> {
    pub const fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            space: PhantomData,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size<Space> {
    pub width: f64,
    pub height: f64,
    space: PhantomData<Space>,
}

impl<Space> Size<Space> {
    pub const fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            space: PhantomData,
        }
    }
}

/// Axis-aligned bounds. Containment includes the minimum edges and excludes the
/// maximum edges, so adjacent rectangles do not both own their shared edge.
/// Nonpositive extents or nonfinite coordinates/edges describe empty bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect<Space> {
    pub origin: Point<Space>,
    pub size: Size<Space>,
}

impl<Space> Rect<Space> {
    pub const fn new(origin: Point<Space>, size: Size<Space>) -> Self {
        Self { origin, size }
    }

    pub const fn from_xywh(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::new(Point::new(x, y), Size::new(width, height))
    }

    pub fn is_empty(&self) -> bool {
        !self.origin.x.is_finite()
            || !self.origin.y.is_finite()
            || !self.size.width.is_finite()
            || !self.size.height.is_finite()
            || self.size.width <= 0.0
            || self.size.height <= 0.0
            || !self.max_x().is_finite()
            || !self.max_y().is_finite()
            || self.max_x() <= self.origin.x
            || self.max_y() <= self.origin.y
    }

    pub fn contains(&self, point: Point<Space>) -> bool {
        !self.is_empty()
            && point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x < self.max_x()
            && point.y < self.max_y()
    }

    /// Returns no rectangle for disjoint, touching or empty bounds.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        if self.is_empty() || other.is_empty() {
            return None;
        }
        let x = self.origin.x.max(other.origin.x);
        let y = self.origin.y.max(other.origin.y);
        let max_x = self.max_x().min(other.max_x());
        let max_y = self.max_y().min(other.max_y());
        (max_x > x && max_y > y).then(|| Self::from_xywh(x, y, max_x - x, max_y - y))
    }

    fn max_x(&self) -> f64 {
        self.origin.x + self.size.width
    }

    fn max_y(&self) -> f64 {
        self.origin.y + self.size.height
    }
}

/// Physical pixels per logical pixel. Must be finite and strictly positive.
/// Canvas pan/zoom is a separate transform and never an implicit DPI conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleFactor(f64);

impl ScaleFactor {
    pub fn new(value: f64) -> Result<Self, InvalidScaleFactor> {
        if value.is_finite() && value > 0.0 {
            Ok(Self(value))
        } else {
            Err(InvalidScaleFactor)
        }
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl Default for ScaleFactor {
    fn default() -> Self {
        Self(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidScaleFactor;

impl fmt::Display for InvalidScaleFactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DPI scale factor must be finite and strictly positive")
    }
}

impl std::error::Error for InvalidScaleFactor {}

impl Point<Logical> {
    pub fn to_physical(self, scale: ScaleFactor) -> Point<Physical> {
        Point::new(self.x * scale.get(), self.y * scale.get())
    }
}

impl Point<Physical> {
    pub fn to_logical(self, scale: ScaleFactor) -> Point<Logical> {
        Point::new(self.x / scale.get(), self.y / scale.get())
    }
}

impl Size<Logical> {
    pub fn to_physical(self, scale: ScaleFactor) -> Size<Physical> {
        Size::new(self.width * scale.get(), self.height * scale.get())
    }
}

impl Size<Physical> {
    pub fn to_logical(self, scale: ScaleFactor) -> Size<Logical> {
        Size::new(self.width / scale.get(), self.height / scale.get())
    }
}

impl Rect<Logical> {
    pub fn to_physical(self, scale: ScaleFactor) -> Rect<Physical> {
        Rect::new(self.origin.to_physical(scale), self.size.to_physical(scale))
    }
}

impl Rect<Physical> {
    pub fn to_logical(self, scale: ScaleFactor) -> Rect<Logical> {
        Rect::new(self.origin.to_logical(scale), self.size.to_logical(scale))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_dpi_preserves_subpixels_and_round_trips() {
        let rect = Rect::<Logical>::from_xywh(-10.5, 2.25, 101.5, 50.25);
        for factor in [1.0, 1.25, 1.5, 2.0] {
            let scale = ScaleFactor::new(factor).unwrap();
            let physical = rect.to_physical(scale);
            assert_eq!(physical.origin.x, -10.5 * factor);
            assert_eq!(physical.origin.y, 2.25 * factor);
            assert_eq!(physical.size.width, 101.5 * factor);
            assert_eq!(physical.size.height, 50.25 * factor);
            assert_eq!(physical.to_logical(scale), rect);
        }
    }

    #[test]
    fn invalid_dpi_is_rejected() {
        for factor in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(ScaleFactor::new(factor), Err(InvalidScaleFactor));
        }
        assert_eq!(ScaleFactor::default().get(), 1.0);
    }

    #[test]
    fn adjacent_rectangles_have_unambiguous_hit_edges() {
        let left = Rect::<Logical>::from_xywh(-10.0, 0.0, 10.0, 10.0);
        let right = Rect::<Logical>::from_xywh(0.0, 0.0, 10.0, 10.0);
        let edge = Point::new(0.0, 5.0);
        assert!(!left.contains(edge));
        assert!(right.contains(edge));
        assert!(left.contains(Point::new(-10.0, 0.0)));
        assert!(!right.contains(Point::new(5.0, 10.0)));
        assert_eq!(left.intersection(&right), None);
        assert!(!right.contains(Point::new(f64::NAN, 5.0)));
    }

    #[test]
    fn nested_clips_intersect_in_the_same_space() {
        let outer = Rect::<Canvas>::from_xywh(-10.0, -10.0, 30.0, 30.0);
        let inner = Rect::<Canvas>::from_xywh(5.0, -20.0, 30.0, 30.0);
        let expected = Rect::from_xywh(5.0, -10.0, 15.0, 20.0);
        assert_eq!(outer.intersection(&inner), Some(expected));
        assert_eq!(inner.intersection(&outer), Some(expected));
        assert_eq!(outer.intersection(&outer), Some(outer));
        assert_eq!(
            outer.intersection(&Rect::from_xywh(50.0, 50.0, 1.0, 1.0)),
            None
        );
    }

    #[test]
    fn empty_and_invalid_bounds_cannot_receive_hits_or_clips() {
        let valid = Rect::<Physical>::from_xywh(0.0, 0.0, 10.0, 10.0);
        for invalid in [
            Rect::from_xywh(0.0, 0.0, 0.0, 10.0),
            Rect::from_xywh(0.0, 0.0, 10.0, -1.0),
            Rect::from_xywh(f64::NAN, 0.0, 10.0, 10.0),
            Rect::from_xywh(0.0, f64::INFINITY, 10.0, 10.0),
            Rect::from_xywh(0.0, 0.0, f64::INFINITY, 10.0),
            Rect::from_xywh(f64::MAX, 0.0, f64::MAX, 10.0),
            Rect::from_xywh(f64::MAX, 0.0, 1.0, 10.0),
        ] {
            assert!(invalid.is_empty());
            assert!(!invalid.contains(Point::new(1.0, 1.0)));
            assert_eq!(invalid.intersection(&valid), None);
            assert_eq!(valid.intersection(&invalid), None);
        }
    }
}
