//! Small enum dispatch over statically typed HAL backends; no native handles
//! cross API boundaries and failed initializations drop before the next attempt.
use super::*;
use crate::{Backend, BackendFailure, ImageStats, UploadStats, selection};

enum Selected {
    #[cfg(not(target_vendor = "apple"))]
    Vulkan(Gpu<hal::api::Vulkan>),
    #[cfg(target_os = "windows")]
    Dx12(Gpu<hal::api::Dx12>),
    #[cfg(target_vendor = "apple")]
    Metal(Gpu<hal::api::Metal>),
}
pub(crate) struct NativeRenderer {
    selected: Selected,
    failures: Vec<BackendFailure>,
}

macro_rules! dispatch {
    ($selected:expr, $method:ident $(,$arg:expr)* $(,)?) => {
        match $selected {
            #[cfg(not(target_vendor = "apple"))]
            Selected::Vulkan(gpu) => gpu.$method($($arg),*),
            #[cfg(target_os = "windows")]
            Selected::Dx12(gpu) => gpu.$method($($arg),*),
            #[cfg(target_vendor = "apple")]
            Selected::Metal(gpu) => gpu.$method($($arg),*),
        }
    };
}

impl NativeRenderer {
    pub(crate) fn new(
        window: Arc<dyn SurfaceWindow>,
        width: u32,
        height: u32,
        options: RendererOptions,
    ) -> Result<Self, RenderError> {
        let (selected, failures) =
            selection::select(options.backend, selection::compiled_backends(), |backend| {
                match backend {
                    #[cfg(not(target_vendor = "apple"))]
                    Backend::Vulkan => {
                        Gpu::<hal::api::Vulkan>::new(window.clone(), width, height, options)
                            .map(Selected::Vulkan)
                    }
                    #[cfg(target_os = "windows")]
                    Backend::Dx12 => {
                        Gpu::<hal::api::Dx12>::new(window.clone(), width, height, options)
                            .map(Selected::Dx12)
                    }
                    #[cfg(target_vendor = "apple")]
                    Backend::Metal => {
                        Gpu::<hal::api::Metal>::new(window.clone(), width, height, options)
                            .map(Selected::Metal)
                    }
                    _ => Err(error("backend selection", "unsupported native API")),
                }
            })?;
        Ok(Self { selected, failures })
    }
    pub(crate) fn backend(&self) -> Backend {
        match &self.selected {
            #[cfg(not(target_vendor = "apple"))]
            Selected::Vulkan(_) => Backend::Vulkan,
            #[cfg(target_os = "windows")]
            Selected::Dx12(_) => Backend::Dx12,
            #[cfg(target_vendor = "apple")]
            Selected::Metal(_) => Backend::Metal,
        }
    }
    pub(crate) fn failures(&self) -> &[BackendFailure] {
        &self.failures
    }
    pub(crate) fn adapter_name(&self) -> &str {
        dispatch!(&self.selected, adapter_name)
    }
    pub(crate) fn last_frame_stats(&self) -> FrameStats {
        dispatch!(&self.selected, last_frame_stats)
    }
    pub(crate) fn image_stats(&self) -> ImageStats {
        dispatch!(&self.selected, image_stats)
    }
    pub(crate) fn upload_stats(&self) -> UploadStats {
        dispatch!(&self.selected, upload_stats)
    }
    pub(crate) fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        dispatch!(&mut self.selected, resize, width, height)
    }
    pub(crate) fn render(
        &mut self,
        primitives: &[Primitive],
        scale: ScaleFactor,
    ) -> Result<RenderStatus, RenderError> {
        dispatch!(&mut self.selected, render, primitives, scale)
    }
    pub(crate) fn capture(
        &mut self,
        primitives: &[Primitive],
        scale: ScaleFactor,
        width: u32,
        height: u32,
    ) -> Result<Capture, RenderError> {
        dispatch!(
            &mut self.selected,
            capture,
            primitives,
            scale,
            width,
            height
        )
    }
    pub(crate) fn wait_idle(&mut self) -> Result<(), RenderError> {
        dispatch!(&mut self.selected, wait_idle)
    }
    pub(crate) fn upload_rgba8(
        &mut self,
        width: u32,
        height: u32,
        bytes: &[u8],
    ) -> Result<ImageId, RenderError> {
        dispatch!(&mut self.selected, upload_rgba8, width, height, bytes)
    }
    pub(crate) fn remove_image(&mut self, id: ImageId) -> Result<bool, RenderError> {
        dispatch!(&mut self.selected, remove_image, id)
    }
}
