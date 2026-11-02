use crate::{RenderError, error};
use zest_core::geometry::{Rect, ScaleFactor};
use zest_paint::{ImageId, Primitive};

pub(crate) const STRIDE: usize = 32;
pub(crate) const VERTEX_BYTES: usize = 6 * STRIDE;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Draw {
    pub image: Option<ImageId>,
    pub first: u32,
    pub count: u32,
}
pub(crate) struct Batch {
    pub bytes: Vec<u8>,
    pub draws: Vec<Draw>,
}

pub(crate) fn prepare(
    primitives: &[Primitive],
    scale: ScaleFactor,
    width: u32,
    height: u32,
    max_bytes: u64,
) -> Result<Batch, RenderError> {
    if width == 0 || height == 0 {
        return Err(error("batch", "nonzero target dimensions are required"));
    }
    let len = primitives
        .len()
        .checked_mul(VERTEX_BYTES)
        .filter(|n| *n as u64 <= max_bytes && *n / STRIDE <= u32::MAX as usize)
        .ok_or_else(|| error("batch", "vertex data exceeds adapter buffer or draw limits"))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|e| error("vertex allocation", e))?;
    let mut draws: Vec<Draw> = Vec::new();
    for (index, primitive) in primitives.iter().enumerate() {
        let (bounds, uv, color, image) = match *primitive {
            Primitive::Quad(q) => {
                q.validate().map_err(|e| error("quad", e))?;
                (q.bounds, Rect::from_xywh(0.0, 0.0, 1.0, 1.0), q.color, None)
            }
            Primitive::Image(i) => {
                i.validate().map_err(|e| error("image", e))?;
                (i.bounds, i.uv, i.tint, Some(i.id))
            }
        };
        let rect = bounds.to_physical(scale);
        let x0 = (2.0 * rect.origin.x / f64::from(width) - 1.0) as f32;
        let x1 = (2.0 * (rect.origin.x + rect.size.width) / f64::from(width) - 1.0) as f32;
        let y0 = (1.0 - 2.0 * rect.origin.y / f64::from(height)) as f32;
        let y1 = (1.0 - 2.0 * (rect.origin.y + rect.size.height) / f64::from(height)) as f32;
        if [x0, x1, y0, y1].iter().any(|v| !v.is_finite()) {
            return Err(error("geometry", "coordinates exceed the finite GPU range"));
        }
        let [u0, v0, u1, v1] = [
            uv.origin.x,
            uv.origin.y,
            uv.origin.x + uv.size.width,
            uv.origin.y + uv.size.height,
        ]
        .map(|v| v as f32);
        let [r, g, b, a] = color;
        for [x, y, u, v] in [
            [x0, y0, u0, v0],
            [x0, y1, u0, v1],
            [x1, y0, u1, v0],
            [x1, y0, u1, v0],
            [x0, y1, u0, v1],
            [x1, y1, u1, v1],
        ] {
            for value in [x, y, u, v, r * a, g * a, b * a, a] {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
        }
        if let Some(last) = draws.last_mut().filter(|d| d.image == image) {
            last.count += 6;
        } else {
            draws.push(Draw {
                image,
                first: (index * 6) as u32,
                count: 6,
            });
        }
    }
    Ok(Batch { bytes, draws })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shipped_wgsl_is_valid() {
        crate::backend::shader().unwrap();
    }
    use zest_core::resource::ResourceArena;
    use zest_paint::{Image, ImageResource, Quad};
    fn quad() -> Primitive {
        Primitive::Quad(Quad {
            bounds: Rect::from_xywh(0.0, 0.0, 100.0, 50.0),
            color: [1.0, 0.5, 0.0, 0.5],
        })
    }
    #[test]
    fn fractional_dpi_uv_and_premultiplied_layout() {
        let batch = prepare(&[quad()], ScaleFactor::new(1.5).unwrap(), 300, 150, 4096).unwrap();
        let floats: Vec<_> = batch
            .bytes
            .chunks_exact(4)
            .map(|v| f32::from_ne_bytes(v.try_into().unwrap()))
            .collect();
        assert_eq!(&floats[..8], &[-1.0, 1.0, 0.0, 0.0, 0.5, 0.25, 0.0, 0.5]);
        assert_eq!(&floats[40..44], &[0.0, 0.0, 1.0, 1.0]);
    }
    #[test]
    fn only_adjacent_materials_merge_without_reordering() {
        let mut ids = ResourceArena::<ImageResource, _>::default();
        let id = ids.insert(());
        let image = Primitive::Image(Image::new(id, Rect::from_xywh(0.0, 0.0, 20.0, 20.0)));
        let batch = prepare(
            &[quad(), quad(), image, image, quad()],
            ScaleFactor::default(),
            100,
            100,
            4096,
        )
        .unwrap();
        assert_eq!(
            batch.draws,
            [
                Draw {
                    image: None,
                    first: 0,
                    count: 12
                },
                Draw {
                    image: Some(id),
                    first: 12,
                    count: 12
                },
                Draw {
                    image: None,
                    first: 24,
                    count: 6
                }
            ]
        );
        assert!(
            prepare(
                &[quad(), quad()],
                ScaleFactor::default(),
                100,
                100,
                VERTEX_BYTES as u64
            )
            .is_err()
        );
        assert!(
            prepare(&[], ScaleFactor::default(), 100, 100, 0)
                .unwrap()
                .bytes
                .is_empty()
        );
    }
    #[test]
    fn invalid_inputs_fail_before_gpu_access() {
        for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let Primitive::Quad(mut q) = quad() else {
                unreachable!()
            };
            q.color[0] = value;
            assert!(
                prepare(
                    &[Primitive::Quad(q)],
                    ScaleFactor::default(),
                    100,
                    100,
                    4096
                )
                .is_err()
            );
        }
        assert!(prepare(&[quad()], ScaleFactor::default(), 0, 0, 4096).is_err());
    }
}
