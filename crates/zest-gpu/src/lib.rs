//! Experimental direct HAL quad batches with two reusable frame slots.
//! Vulkan on Windows/Linux; Metal on Apple targets. Adjacent compatible materials
//! batch without changing painter order. Supports rectangular clips and nearest
//! RGBA8 images; widget rendering, text, device recovery and fallback are pending.
#![deny(unsafe_code)]

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{fmt, marker::PhantomData, rc::Rc, sync::Arc};
use zest_core::geometry::ScaleFactor;
pub use zest_paint::Quad;
use zest_paint::{DisplayList, ImageId, Primitive};

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

/// Packed, top-to-bottom sRGB RGBA8 pixels from an offscreen render.
#[derive(Clone, Debug)]
pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

/// CPU counters for the last successful presentation or offscreen capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Rectangle primitives submitted, including textured images.
    pub quads: usize,
    pub draw_calls: u32,
    /// Vertex bytes for this render/capture; excludes separate image uploads.
    pub uploaded_bytes: u64,
    /// Capacity in bytes of the selected slot, retained until growth or drop.
    pub vertex_capacity: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageStats {
    pub live: usize,
    pub retired: usize,
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
    pub fn last_frame_stats(&self) -> FrameStats {
        self.inner.last_frame_stats()
    }

    /// Waits for queued rendering/presentation before replacing the swapchain.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        self.inner.resize(width, height)
    }

    pub fn render(&mut self, quad: Quad, scale: ScaleFactor) -> Result<RenderStatus, RenderError> {
        self.inner.render(&[Primitive::Quad(quad)], scale)
    }

    /// One draw per adjacent material run; solid-only lists use one draw. Empty lists clear
    /// the target without uploading vertices or issuing a draw.
    pub fn render_list(
        &mut self,
        list: &DisplayList,
        scale: ScaleFactor,
    ) -> Result<RenderStatus, RenderError> {
        self.inner
            .render(&list.primitives().collect::<Vec<_>>(), scale)
    }

    /// Includes pending presentation and drains retired images after completion.
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
        self.inner
            .capture(&[Primitive::Quad(quad)], scale, width, height)
    }

    /// Offscreen counterpart to `render_list`, with synchronous RGBA8 readback.
    pub fn capture_list(
        &mut self,
        list: &DisplayList,
        scale: ScaleFactor,
        width: u32,
        height: u32,
    ) -> Result<Capture, RenderError> {
        self.inner
            .capture(&list.primitives().collect::<Vec<_>>(), scale, width, height)
    }

    /// Uploads top-to-bottom, straight-alpha sRGB RGBA8 pixels. Sampling is
    /// nearest/clamp-to-edge. GPU upload completes in queue order before draws.
    pub fn upload_rgba8(
        &mut self,
        width: u32,
        height: u32,
        rgba8: &[u8],
    ) -> Result<ImageId, RenderError> {
        self.inner.upload_rgba8(width, height, rgba8)
    }
    /// Invalidates the handle immediately; native resources wait for last use.
    /// Returns false for stale/foreign handles. Retirement is collected by later
    /// GPU work, `wait_idle`, or shutdown, and never blocks removal itself.
    pub fn remove_image(&mut self, id: ImageId) -> Result<bool, RenderError> {
        self.inner.remove_image(id)
    }
    pub fn image_stats(&self) -> ImageStats {
        self.inner.image_stats()
    }
}

mod batch;
