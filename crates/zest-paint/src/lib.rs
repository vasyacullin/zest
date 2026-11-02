//! Backend-independent paint snapshots. Quads stay in painter order; nested
//! axis-aligned clips intersect on the CPU in logical window coordinates.
use std::fmt;
use zest_core::geometry::{Logical, Rect};

/// Straight-alpha linear RGBA in [0, 1]. The renderer premultiplies on upload.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub bounds: Rect<Logical>,
    pub color: [f32; 4],
}

impl Quad {
    pub fn validate(&self) -> Result<(), PaintError> {
        if !finite_rect(self.bounds) || self.bounds.is_empty() {
            return Err(PaintError::InvalidBounds);
        }
        if self
            .color
            .iter()
            .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
        {
            return Err(PaintError::InvalidColor);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintError {
    InvalidBounds,
    InvalidColor,
    InvalidClip,
}
impl fmt::Display for PaintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidBounds => "quad bounds must be finite and nonempty",
            Self::InvalidColor => "RGBA channels must be finite and in [0, 1]",
            Self::InvalidClip => "clip coordinates and edges must be finite",
        })
    }
}
impl std::error::Error for PaintError {}

fn finite_rect(rect: Rect<Logical>) -> bool {
    [
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
        rect.origin.x + rect.size.width,
        rect.origin.y + rect.size.height,
    ]
    .iter()
    .all(|n| n.is_finite())
}

/// An immutable, validated sequence of clipped solid-color quads. It contains
/// neither ECS entities nor GPU handles and may be built without a GPU backend.
///
/// ```
/// use zest_paint::{DisplayList, Quad};
/// use zest_core::geometry::{Logical, Rect};
/// let list = DisplayList::build(|painter| {
///     painter.with_clip(Rect::<Logical>::from_xywh(0.0, 0.0, 100.0, 80.0), |painter| {
///         painter.quad(Quad { bounds: Rect::from_xywh(50.0, 10.0, 100.0, 30.0), color: [1.0, 0.0, 0.0, 1.0] })
///     })
/// })?;
/// assert_eq!(list.quads()[0].bounds.size.width, 50.0);
/// # Ok::<(), zest_paint::PaintError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    quads: Vec<Quad>,
}

impl DisplayList {
    /// A failed builder returns no partial display list.
    pub fn build(
        build: impl FnOnce(&mut Painter<'_>) -> Result<(), PaintError>,
    ) -> Result<Self, PaintError> {
        let mut list = Self::default();
        build(&mut Painter {
            quads: &mut list.quads,
            clip: Clip::Unbounded,
        })?;
        Ok(list)
    }
    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }
    pub fn len(&self) -> usize {
        self.quads.len()
    }
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }
}

#[derive(Clone, Copy)]
enum Clip {
    Unbounded,
    Bounds(Rect<Logical>),
    Empty,
}

/// Builder for one snapshot. Clip scopes borrow a child painter, so the parent's
/// clip is restored even if application code catches a panic in a child scope.
pub struct Painter<'a> {
    quads: &'a mut Vec<Quad>,
    clip: Clip,
}
impl Painter<'_> {
    /// Invalid input is rejected even in a fully clipped region. Fully clipped
    /// or transparent quads emit no geometry; all other calls retain their order.
    pub fn quad(&mut self, mut quad: Quad) -> Result<(), PaintError> {
        quad.validate()?;
        if quad.color[3] == 0.0 {
            return Ok(());
        }
        match self.clip {
            Clip::Empty => return Ok(()),
            Clip::Bounds(bounds) => match quad.bounds.intersection(&bounds) {
                None => return Ok(()),
                Some(bounds) => quad.bounds = bounds,
            },
            Clip::Unbounded => {}
        }
        self.quads.push(quad);
        Ok(())
    }

    /// Clips intersect with every ancestor. Empty/negative extents hide the
    /// child scope; nonfinite clips return an error without calling its builder.
    /// On a returned error, commands from this child scope are rolled back.
    pub fn with_clip<R>(
        &mut self,
        bounds: Rect<Logical>,
        build: impl FnOnce(&mut Painter<'_>) -> Result<R, PaintError>,
    ) -> Result<R, PaintError> {
        if !finite_rect(bounds) {
            return Err(PaintError::InvalidClip);
        }
        let clip = if bounds.is_empty() {
            Clip::Empty
        } else {
            match self.clip {
                Clip::Empty => Clip::Empty,
                Clip::Unbounded => Clip::Bounds(bounds),
                Clip::Bounds(parent) => parent
                    .intersection(&bounds)
                    .map_or(Clip::Empty, Clip::Bounds),
            }
        };
        let start = self.quads.len();
        let result = build(&mut Painter {
            quads: self.quads,
            clip,
        });
        if result.is_err() {
            self.quads.truncate(start);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quad(x: f64) -> Quad {
        Quad {
            bounds: Rect::from_xywh(x, 0.0, 20.0, 20.0),
            color: [1.0, 0.0, 0.0, 1.0],
        }
    }

    #[test]
    fn nested_clips_intersect_all_ancestors_and_restore_for_siblings() {
        let list = DisplayList::build(|p| {
            p.with_clip(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), |p| {
                p.with_clip(Rect::from_xywh(5.0, -10.0, 20.0, 30.0), |p| {
                    p.quad(quad(0.0))
                })?;
                p.quad(quad(0.0))
            })?;
            p.quad(quad(0.0))
        })
        .unwrap();
        assert_eq!(list.quads()[0].bounds, Rect::from_xywh(5.0, 0.0, 5.0, 10.0));
        assert_eq!(
            list.quads()[1].bounds,
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0)
        );
        assert_eq!(list.quads()[2].bounds, quad(0.0).bounds);
    }

    #[test]
    fn empty_disjoint_and_transparent_quads_emit_no_geometry() {
        let list = DisplayList::build(|p| {
            for width in [0.0, -1.0] {
                p.with_clip(Rect::from_xywh(0.0, 0.0, width, 10.0), |p| {
                    p.quad(quad(0.0))
                })?;
            }
            p.with_clip(Rect::from_xywh(20.0, 0.0, 10.0, 10.0), |p| {
                p.quad(quad(0.0))
            })?;
            p.quad(Quad {
                color: [1.0, 0.0, 0.0, 0.0],
                ..quad(0.0)
            })
        })
        .unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn painter_order_is_not_sorted_by_color_or_position() {
        let list = DisplayList::build(|p| {
            for x in [10.0, -5.0, 0.0] {
                p.quad(quad(x))?;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            list.quads()
                .iter()
                .map(|q| q.bounds.origin.x)
                .collect::<Vec<_>>(),
            [10.0, -5.0, 0.0]
        );
    }

    #[test]
    fn invalid_data_is_rejected_even_when_clipped_out() {
        assert_eq!(
            DisplayList::build(|p| p.quad(Quad {
                color: [f32::NAN; 4],
                ..quad(0.0)
            })),
            Err(PaintError::InvalidColor)
        );
        assert_eq!(
            DisplayList::build(|p| p.with_clip(Rect::from_xywh(0.0, 0.0, 0.0, 0.0), |p| p
                .quad(quad(f64::NAN)))),
            Err(PaintError::InvalidBounds)
        );
        let mut ran = false;
        assert_eq!(
            DisplayList::build(|p| p.with_clip(
                Rect::from_xywh(f64::INFINITY, 0.0, 1.0, 1.0),
                |_| {
                    ran = true;
                    Ok(())
                }
            )),
            Err(PaintError::InvalidClip)
        );
        assert!(!ran);
    }

    #[test]
    fn failed_scope_rolls_back_its_commands_and_retains_parent_clip() {
        let list = DisplayList::build(|p| {
            p.with_clip(Rect::from_xywh(0.0, 0.0, 10.0, 10.0), |p| {
                assert_eq!(
                    p.with_clip(Rect::from_xywh(0.0, 0.0, 5.0, 5.0), |p| {
                        p.quad(quad(0.0))?;
                        Err::<(), _>(PaintError::InvalidColor)
                    }),
                    Err(PaintError::InvalidColor)
                );
                p.quad(quad(0.0))
            })
        })
        .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list.quads()[0].bounds.size.width, 10.0);
    }

    #[test]
    fn caught_panic_in_child_does_not_leak_clip_to_next_sibling() {
        let list = DisplayList::build(|p| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = p.with_clip(
                    Rect::from_xywh(0.0, 0.0, 1.0, 1.0),
                    |_| -> Result<(), PaintError> {
                        panic!("application painter");
                    },
                );
            }));
            assert!(result.is_err());
            p.quad(quad(0.0))
        })
        .unwrap();
        assert_eq!(list.quads(), [quad(0.0)]);
    }
}
