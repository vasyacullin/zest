//! Experimental direct HAL renderer: one colored quad, two reusable frame slots.
//! Vulkan on Windows/Linux; Metal on Apple targets. There is no widget renderer,
//! text, texture sampling, device recovery or backend fallback yet.
#![deny(unsafe_code)]

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{fmt, marker::PhantomData, rc::Rc, sync::Arc};
use zest_core::geometry::{Logical, Rect, ScaleFactor};

#[allow(unsafe_code)]
mod backend;

/// The renderer retains this owner until its surface and GPU work are released.
pub trait SurfaceWindow: HasDisplayHandle + HasWindowHandle + Send + Sync {}
impl<T: HasDisplayHandle + HasWindowHandle + Send + Sync> SurfaceWindow for T {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderError(pub(crate) String);
impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RenderError {}

pub(crate) fn error(stage: &str, cause: impl fmt::Display) -> RenderError {
    RenderError(format!("{stage}: {cause}"))
}

/// Coordinates are logical window pixels. Color is straight-alpha linear RGBA
/// in [0, 1]; it is premultiplied before upload to the sRGB render target.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub bounds: Rect<Logical>,
    pub color: [f32; 4],
}

/// Packed, top-to-bottom sRGB RGBA8 pixels from an offscreen render.
#[derive(Clone, Debug)]
pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderStatus {
    Presented,
    /// A zero-size surface is not configured and performs no GPU submission.
    Suspended,
    /// Acquisition timed out or the window is occluded. Retry on a later event.
    Skipped,
    /// Surface is outdated; call resize with the current physical window size.
    NeedsResize,
}

/// Owns the surface, device and all renderer resources. Calls are synchronous on
/// one thread; queued GPU work may use two frame slots concurrently. Terminal
/// HAL errors require recreating the renderer. Native handles are never exposed.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<zest_gpu::Renderer>(); // native surface stays on its thread
/// ```
pub struct Renderer {
    inner: backend::NativeRenderer,
    // Native surfaces stay on their creation thread; also prevents overlapping
    // queue wait_for_idle calls through shared references on different threads.
    thread: PhantomData<Rc<()>>,
}

impl Renderer {
    /// Width and height are physical pixels. Zero dimensions start suspended.
    pub fn new<W: SurfaceWindow + 'static>(
        window: Arc<W>,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderError> {
        Ok(Self {
            inner: backend::NativeRenderer::new(window, width, height)?,
            thread: PhantomData,
        })
    }

    pub fn adapter_name(&self) -> &str {
        self.inner.adapter_name()
    }

    /// Waits for queued rendering/presentation before replacing the swapchain.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        self.inner.resize(width, height)
    }

    pub fn render(&mut self, quad: Quad, scale: ScaleFactor) -> Result<RenderStatus, RenderError> {
        self.inner.render(quad, scale)
    }

    /// Includes pending presentation; useful for a bounded smoke test or shutdown.
    pub fn wait_idle(&mut self) -> Result<(), RenderError> {
        self.inner.wait_idle()
    }

    /// Synchronous offscreen rendering/readback for tests and screenshots. This
    /// waits for GPU work and allocates a target; ordinary rendering never reads
    /// pixels back. Dimensions are physical pixels and must be nonzero.
    pub fn capture(
        &mut self,
        quad: Quad,
        scale: ScaleFactor,
        width: u32,
        height: u32,
    ) -> Result<Capture, RenderError> {
        self.inner.capture(quad, scale, width, height)
    }
}

const VERTEX_BYTES: usize = 6 * 6 * size_of::<f32>();

fn vertices(
    quad: Quad,
    scale: ScaleFactor,
    width: u32,
    height: u32,
) -> Result<[u8; VERTEX_BYTES], RenderError> {
    if quad.bounds.is_empty()
        || quad
            .color
            .iter()
            .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
    {
        return Err(error(
            "quad",
            "finite, nonempty bounds and RGBA in [0, 1] are required",
        ));
    }
    let rect = quad.bounds.to_physical(scale);
    let x0 = (2.0 * rect.origin.x / f64::from(width) - 1.0) as f32;
    let x1 = (2.0 * (rect.origin.x + rect.size.width) / f64::from(width) - 1.0) as f32;
    let y0 = (1.0 - 2.0 * rect.origin.y / f64::from(height)) as f32;
    let y1 = (1.0 - 2.0 * (rect.origin.y + rect.size.height) / f64::from(height)) as f32;
    if [x0, x1, y0, y1].iter().any(|v| !v.is_finite()) {
        return Err(error("quad", "coordinates exceed the finite GPU range"));
    }
    let [r, g, b, a] = quad.color;
    let mut bytes = [0; VERTEX_BYTES];
    for (vertex, [x, y]) in
        bytes
            .chunks_exact_mut(24)
            .zip([[x0, y0], [x0, y1], [x1, y0], [x1, y0], [x0, y1], [x1, y1]])
    {
        for (field, value) in vertex
            .chunks_exact_mut(4)
            .zip([x, y, r * a, g * a, b * a, a])
        {
            field.copy_from_slice(&value.to_ne_bytes());
        }
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quad() -> Quad {
        Quad {
            bounds: Rect::from_xywh(0.0, 0.0, 100.0, 50.0),
            color: [1.0, 0.5, 0.0, 0.5],
        }
    }
    #[test]
    fn dpi_coordinates_and_premultiplied_vertex_layout() {
        let bytes = vertices(quad(), ScaleFactor::new(1.5).unwrap(), 300, 150).unwrap();
        let floats: Vec<_> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_ne_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(&floats[..6], &[-1.0, 1.0, 0.5, 0.25, 0.0, 0.5]);
        assert_eq!(&floats[30..32], &[0.0, 0.0]);
    }
    #[test]
    fn bad_geometry_and_colors_are_rejected_before_gpu_access() {
        for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut q = quad();
            q.color[0] = value;
            assert!(vertices(q, ScaleFactor::default(), 100, 100).is_err());
        }
        for rect in [
            Rect::from_xywh(0.0, 0.0, 0.0, 1.0),
            Rect::from_xywh(f64::MAX, 0.0, 1.0, 1.0),
        ] {
            assert!(
                vertices(
                    Quad {
                        bounds: rect,
                        ..quad()
                    },
                    ScaleFactor::default(),
                    100,
                    100
                )
                .is_err()
            );
        }
        assert!(vertices(quad(), ScaleFactor::default(), 0, 0).is_err());
    }
    #[test]
    fn shipped_wgsl_is_valid() {
        backend::shader().unwrap();
    }
}
