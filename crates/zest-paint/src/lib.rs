//! Backend-independent paint snapshots. Quads/images stay in painter order;
//! nested clips intersect on the CPU, adjusting image UVs with their bounds.
use std::fmt;
use zest_core::geometry::{Logical, Rect};
use zest_core::resource::ResourceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageResource {}
pub type ImageId = ResourceId<ImageResource>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Uv {}

/// Normalized UVs address straight-alpha sRGB pixels. Tint is linear RGBA.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Image {
    pub id: ImageId,
    pub bounds: Rect<Logical>,
    pub uv: Rect<Uv>,
    pub tint: [f32; 4],
}
impl Image {
    pub fn new(id: ImageId, bounds: Rect<Logical>) -> Self {
        Self {
            id,
            bounds,
            uv: Rect::from_xywh(0.0, 0.0, 1.0, 1.0),
            tint: [1.0; 4],
        }
    }
    pub fn validate(&self) -> Result<(), PaintError> {
        Quad {
            bounds: self.bounds,
            color: self.tint,
        }
        .validate()?;
        if self.uv.is_empty()
            || self.uv.origin.x < 0.0
            || self.uv.origin.y < 0.0
            || self.uv.origin.x + self.uv.size.width > 1.0
            || self.uv.origin.y + self.uv.size.height > 1.0
        {
            return Err(PaintError::InvalidUv);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Primitive {
    Quad(Quad),
    Image(Image),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Quad(usize),
    Image(usize),
}

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
    InvalidUv,
}
impl fmt::Display for PaintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidBounds => "quad bounds must be finite and nonempty",
            Self::InvalidColor => "RGBA channels must be finite and in [0, 1]",
            Self::InvalidClip => "clip coordinates and edges must be finite",
            Self::InvalidUv => "UV bounds must be finite, nonempty and within [0, 1]",
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

/// An immutable, validated sequence of clipped quads and images. Contains opaque
/// image identities, no native GPU handles/ECS entities; buildable without a GPU.
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
    images: Vec<Image>,
    order: Vec<Command>,
}

impl DisplayList {
    /// A failed builder returns no partial display list.
    pub fn build(
        build: impl FnOnce(&mut Painter<'_>) -> Result<(), PaintError>,
    ) -> Result<Self, PaintError> {
        let mut list = Self::default();
        build(&mut Painter {
            list: &mut list,
            clip: Clip::Unbounded,
        })?;
        Ok(list)
    }
    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }
    pub fn len(&self) -> usize {
        self.order.len()
    }
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
    /// All primitives in painter order. `quads()` is the solid-only subset.
    pub fn primitives(
        &self,
    ) -> impl ExactSizeIterator<Item = Primitive> + DoubleEndedIterator + '_ {
        self.order.iter().map(|command| match *command {
            Command::Quad(i) => Primitive::Quad(self.quads[i]),
            Command::Image(i) => Primitive::Image(self.images[i]),
        })
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
    list: &'a mut DisplayList,
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
        self.list.order.push(Command::Quad(self.list.quads.len()));
        self.list.quads.push(quad);
        Ok(())
    }

    pub fn image(&mut self, mut image: Image) -> Result<(), PaintError> {
        image.validate()?;
        if image.tint[3] == 0.0 {
            return Ok(());
        }
        let bounds = match self.clip {
            Clip::Empty => return Ok(()),
            Clip::Unbounded => image.bounds,
            Clip::Bounds(clip) => match image.bounds.intersection(&clip) {
                Some(bounds) => bounds,
                None => return Ok(()),
            },
        };
        let original = image.bounds;
        let crop = |start: f64,
                    length: f64,
                    source_start: f64,
                    source_length: f64,
                    uv_start: f64,
                    uv_length: f64| {
            // Intersecting edges and subtracting their origins can round an
            // unchanged extent upward (24.8 -> 24.80000000000001). Clamp the
            // normalized edges before mapping, without relaxing input validation.
            let first = ((start - source_start) / source_length).clamp(0.0, 1.0);
            let last = ((start + length - source_start) / source_length).clamp(first, 1.0);
            let a = (uv_start + first * uv_length).clamp(0.0, 1.0);
            let b = (uv_start + last * uv_length).clamp(a, 1.0);
            (a, (b - a).min(1.0 - a))
        };
        let (u, width) = crop(
            bounds.origin.x,
            bounds.size.width,
            original.origin.x,
            original.size.width,
            image.uv.origin.x,
            image.uv.size.width,
        );
        let (v, height) = crop(
            bounds.origin.y,
            bounds.size.height,
            original.origin.y,
            original.size.height,
            image.uv.origin.y,
            image.uv.size.height,
        );
        if width == 0.0 || height == 0.0 {
            return Ok(());
        }
        image.uv = Rect::from_xywh(u, v, width, height);
        image.bounds = bounds;
        self.list.order.push(Command::Image(self.list.images.len()));
        self.list.images.push(image);
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
        let start = (
            self.list.quads.len(),
            self.list.images.len(),
            self.list.order.len(),
        );
        let result = build(&mut Painter {
            list: self.list,
            clip,
        });
        if result.is_err() {
            self.list.quads.truncate(start.0);
            self.list.images.truncate(start.1);
            self.list.order.truncate(start.2);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_image_edges_remain_valid_after_unchanged_and_nested_clips() {
        let mut source = image();
        source.bounds = Rect::from_xywh(300.0, 280.0, 19.2, 24.8);
        let list = DisplayList::build(|p| {
            p.with_clip(Rect::from_xywh(300.0, 280.0, 320.0, 68.0), |p| {
                p.image(source)
            })
        })
        .unwrap();
        let Primitive::Image(clipped) = list.primitives().next().unwrap() else {
            panic!("image")
        };
        clipped.validate().unwrap();
        assert_eq!(clipped.uv.origin, source.uv.origin);
        assert!((clipped.uv.size.height - 1.0).abs() < 1e-14);
        let list = DisplayList::build(|p| {
            p.with_clip(Rect::from_xywh(304.1, 281.2, 15.1, 23.6), |p| {
                p.with_clip(Rect::from_xywh(307.3, 282.4, 11.9, 22.4), |p| {
                    p.image(source)
                })
            })
        })
        .unwrap();
        for primitive in list.primitives() {
            if let Primitive::Image(image) = primitive {
                image.validate().unwrap();
            }
        }
        let mut invalid = source;
        invalid.uv.size.height = 1.0001;
        assert!(DisplayList::build(|p| p.image(invalid)).is_err());
    }
    fn image() -> Image {
        let mut ids = zest_core::resource::ResourceArena::<ImageResource, _>::default();
        Image::new(ids.insert(()), Rect::from_xywh(0.0, 0.0, 100.0, 80.0))
    }
    #[test]
    fn image_clipping_preserves_uv_mapping_and_mixed_order() {
        let source = image();
        let list = DisplayList::build(|p| {
            p.quad(quad(0.0))?;
            p.with_clip(Rect::from_xywh(25.0, 20.0, 50.0, 40.0), |p| p.image(source))?;
            p.quad(quad(10.0))
        })
        .unwrap();
        let commands: Vec<_> = list.primitives().collect();
        assert!(matches!(commands[0], Primitive::Quad(_)));
        let Primitive::Image(clipped) = commands[1] else {
            panic!("image order")
        };
        assert_eq!(clipped.bounds, Rect::from_xywh(25.0, 20.0, 50.0, 40.0));
        assert_eq!(clipped.uv, Rect::from_xywh(0.25, 0.25, 0.5, 0.5));
        assert!(matches!(commands[2], Primitive::Quad(_)));
        assert_eq!(list.len(), 3);
        assert_eq!(list.quads().len(), 2);
    }
    #[test]
    fn invalid_uv_and_failed_image_scope_do_not_escape() {
        let source = image();
        assert_eq!(
            DisplayList::build(|p| p.image(Image {
                uv: Rect::from_xywh(0.0, 0.0, 2.0, 1.0),
                ..source
            })),
            Err(PaintError::InvalidUv)
        );
        assert_eq!(
            DisplayList::build(|p| p.image(Image {
                uv: Rect::from_xywh(f64::NAN, 0.0, 1.0, 1.0),
                ..source
            })),
            Err(PaintError::InvalidUv)
        );
        let list = DisplayList::build(|p| {
            let result = p.with_clip(source.bounds, |p| {
                p.image(source)?;
                p.quad(quad(0.0))?;
                Err::<(), _>(PaintError::InvalidUv)
            });
            assert_eq!(result, Err(PaintError::InvalidUv));
            p.image(source)
        })
        .unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.quads().is_empty());
    }
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
