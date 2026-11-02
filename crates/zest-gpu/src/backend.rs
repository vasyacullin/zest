//! All HAL unsafety lives here. The wrapper owns every resource and the window,
//! serializes queue access, and waits before recycling or destroying GPU data.
use crate::batch::{Batch, STRIDE, VERTEX_BYTES, prepare};
use crate::{Capture, FrameStats, RenderError, RenderStatus, SurfaceWindow, error};
use zest_core::resource::ResourceArena;
use zest_paint::{ImageId, ImageResource, Primitive};
mod images;
use images::TextureResource;
use std::{
    borrow::{Borrow, Cow},
    num::NonZeroU64,
    sync::Arc,
    time::Duration,
};
use wgpu_hal::{
    self as hal, Adapter as _, CommandEncoder as _, Device as _, Instance as _, Queue as _,
    Surface as _,
};
use wgpu_types as wgt;
use zest_core::geometry::ScaleFactor;

#[cfg(not(target_vendor = "apple"))]
type Native = hal::api::Vulkan;
#[cfg(target_vendor = "apple")]
type Native = hal::api::Metal;
pub(crate) type NativeRenderer = Gpu<Native>;

pub(crate) fn shader() -> Result<hal::NagaShader, RenderError> {
    let source = include_str!("quad.wgsl");
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|e| error("WGSL parse", e.emit_to_string(source)))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| error("WGSL validation", e))?;
    Ok(hal::NagaShader {
        module: Cow::Owned(module),
        info,
        debug_source: None,
    })
}

struct Frame<A: hal::Api> {
    encoder: Option<A::CommandEncoder>,
    command: Option<A::CommandBuffer>,
    view: Option<A::TextureView>,
    upload: Option<A::Buffer>,
    vertex: Option<A::Buffer>,
    target: Option<A::Texture>,
    readback: Option<A::Buffer>,
    texture_staging: Option<A::Buffer>,
    submitted: u64,
    capacity: u64,
    initialized: bool,
}
impl<A: hal::Api> Default for Frame<A> {
    fn default() -> Self {
        Self {
            encoder: None,
            command: None,
            view: None,
            upload: None,
            vertex: None,
            target: None,
            readback: None,
            texture_staging: None,
            submitted: 0,
            capacity: 0,
            initialized: false,
        }
    }
}

// Automatic field drop order after explicit resource cleanup: queue/device,
// surface, adapter, instance, then the owner of the native window/display.
struct State<A: hal::Api> {
    frames: Vec<Frame<A>>,
    pipeline: Option<A::RenderPipeline>,
    layout: Option<A::PipelineLayout>,
    shader: Option<A::ShaderModule>,
    fence: Option<A::Fence>,
    image_layout: Option<A::BindGroupLayout>,
    sampler: Option<A::Sampler>,
    images: ResourceArena<ImageResource, TextureResource<A>>,
    retired: Vec<TextureResource<A>>,
    white: Option<ImageId>,
    queue: A::Queue,
    device: A::Device,
    surface: A::Surface,
    _adapter: A::Adapter,
    _instance: A::Instance,
    _window: Arc<dyn SurfaceWindow>,
    config: hal::SurfaceConfiguration,
    adapter_name: String,
    max_dimension: u32,
    max_buffer_size: u64,
    copy_pitch_alignment: u64,
    configured: bool,
    failed: bool,
    next_value: u64,
    frame_index: usize,
    last_stats: FrameStats,
}

pub(crate) struct Gpu<A: hal::Api> {
    state: Option<Box<State<A>>>,
}

impl<A: hal::Api> Gpu<A> {
    pub(crate) fn new(
        window: Arc<dyn SurfaceWindow>,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderError> {
        let display = window
            .display_handle()
            .map_err(|e| error("display handle", e))?;
        let handle = window
            .window_handle()
            .map_err(|e| error("window handle", e))?;
        // SAFETY: borrowed display/window handles are valid and their Arc owner
        // is retained until after the surface and instance are dropped.
        let instance = unsafe {
            A::Instance::init(&hal::InstanceDescriptor {
                name: "Zest",
                flags: wgt::InstanceFlags::from_build_config().with_env(),
                memory_budget_thresholds: wgt::MemoryBudgetThresholds::default(),
                backend_options: wgt::BackendOptions::default(),
                telemetry: None,
                display: Some(display),
            })
        }
        .map_err(|e| error("HAL instance", e))?;
        // SAFETY: same instance/display, live owned native window as above.
        let surface = unsafe { instance.create_surface(display.as_raw(), handle.as_raw()) }
            .map_err(|e| error("surface", e))?;
        let limits = wgt::Limits::default();
        // SAFETY: instance and surface are live and used only on this thread.
        let mut adapters = unsafe { instance.enumerate_adapters(Some(&surface)) };
        adapters.sort_by_key(|a| match a.info.device_type {
            wgt::DeviceType::DiscreteGpu => 0,
            wgt::DeviceType::IntegratedGpu => 1,
            _ => 2,
        });
        let mut selected = None;
        for exposed in adapters {
            if !limits.check_limits(&exposed.capabilities.limits) {
                continue;
            }
            // SAFETY: surface belongs to this instance; no queue work exists.
            let Some(caps) = (unsafe { exposed.adapter.surface_capabilities(&surface) }) else {
                continue;
            };
            let format = [
                wgt::TextureFormat::Bgra8UnormSrgb,
                wgt::TextureFormat::Rgba8UnormSrgb,
            ]
            .into_iter()
            .find(|format| {
                caps.formats.iter().any(|f| {
                    f.format == *format && f.color_spaces.contains(wgt::SurfaceColorSpaces::SRGB)
                })
            });
            if let Some(format) = format
                && caps.present_modes.contains(&wgt::PresentMode::Fifo)
                && caps.usage.contains(wgt::TextureUses::COLOR_TARGET)
                && !caps.composite_alpha_modes.is_empty()
            {
                // SAFETY: querying this live adapter does not create resources.
                let available = unsafe { exposed.adapter.texture_format_capabilities(format) };
                if !available.contains(
                    hal::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND
                        | hal::TextureFormatCapabilities::COPY_SRC,
                ) {
                    continue;
                }
                // SAFETY: querying the same live adapter for the fixed upload format.
                let sampled = unsafe {
                    exposed
                        .adapter
                        .texture_format_capabilities(wgt::TextureFormat::Rgba8UnormSrgb)
                };
                if !sampled.contains(
                    hal::TextureFormatCapabilities::SAMPLED
                        | hal::TextureFormatCapabilities::SAMPLED_LINEAR
                        | hal::TextureFormatCapabilities::COPY_DST,
                ) {
                    continue;
                }
                selected = Some((exposed, caps, format));
                break;
            }
        }
        let (exposed, caps, format) = selected.ok_or_else(|| {
            error(
                "adapter",
                "no device supports default limits, an sRGB color target and FIFO presentation",
            )
        })?;
        // SAFETY: requested limits were checked and no optional features are used.
        let hal::OpenDevice { device, queue } = unsafe {
            exposed.adapter.open(
                wgt::Features::empty(),
                &limits,
                &wgt::MemoryHints::MemoryUsage,
            )
        }
        .map_err(|e| error("open device", e))?;
        let config = hal::SurfaceConfiguration {
            maximum_frame_latency: 2u32.clamp(
                *caps.maximum_frame_latency.start(),
                *caps.maximum_frame_latency.end(),
            ),
            present_mode: wgt::PresentMode::Fifo,
            composite_alpha_mode: if caps
                .composite_alpha_modes
                .contains(&wgt::CompositeAlphaMode::Opaque)
            {
                wgt::CompositeAlphaMode::Opaque
            } else {
                caps.composite_alpha_modes[0]
            },
            format,
            color_space: wgt::SurfaceColorSpace::Srgb,
            extent: wgt::Extent3d {
                width: 0,
                height: 0,
                depth_or_array_layers: 1,
            },
            usage: wgt::TextureUses::COLOR_TARGET,
            view_formats: vec![],
        };
        let mut gpu = Self {
            state: Some(Box::new(State {
                frames: vec![],
                pipeline: None,
                layout: None,
                shader: None,
                fence: None,
                image_layout: None,
                sampler: None,
                images: ResourceArena::default(),
                retired: Vec::new(),
                white: None,
                queue,
                device,
                surface,
                _adapter: exposed.adapter,
                _instance: instance,
                _window: window,
                config,
                adapter_name: format!("{:?}: {}", A::VARIANT, exposed.info.name),
                max_dimension: limits.max_texture_dimension_2d,
                max_buffer_size: limits.max_buffer_size,
                copy_pitch_alignment: exposed.capabilities.alignments.buffer_copy_pitch.get(),
                configured: false,
                failed: false,
                next_value: 1,
                frame_index: 0,
                last_stats: FrameStats::default(),
            })),
        };
        gpu.state.as_mut().unwrap().initialize()?;
        gpu.resize(width, height)?;
        Ok(gpu)
    }

    pub(crate) fn adapter_name(&self) -> &str {
        &self.state.as_ref().unwrap().adapter_name
    }
    pub(crate) fn last_frame_stats(&self) -> FrameStats {
        self.state.as_ref().unwrap().last_stats
    }
    pub(crate) fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        self.state.as_mut().unwrap().resize(width, height)
    }
    pub(crate) fn wait_idle(&mut self) -> Result<(), RenderError> {
        let state = self.state.as_mut().unwrap();
        let result = state.wait_idle().and_then(|_| state.collect_retired());
        if result.is_err() {
            state.failed = true;
        }
        result
    }
    pub(crate) fn render(
        &mut self,
        primitives: &[Primitive],
        scale: ScaleFactor,
    ) -> Result<RenderStatus, RenderError> {
        let state = self.state.as_mut().unwrap();
        if state.failed {
            return Err(error(
                "renderer",
                "terminal HAL error; recreate the renderer",
            ));
        }
        if !state.configured {
            return Ok(RenderStatus::Suspended);
        }
        let batch = prepare(
            primitives,
            scale,
            state.config.extent.width,
            state.config.extent.height,
            state.max_buffer_size,
        )?;
        state.validate_images(&batch)?;
        let result = state.render(&batch);
        if result.is_err() {
            state.failed = true;
        }
        result
    }

    pub(crate) fn capture(
        &mut self,
        primitives: &[Primitive],
        scale: ScaleFactor,
        width: u32,
        height: u32,
    ) -> Result<Capture, RenderError> {
        let state = self.state.as_mut().unwrap();
        if state.failed {
            return Err(error(
                "capture",
                "terminal HAL error; recreate the renderer",
            ));
        }
        if width == 0 || height == 0 || width > state.max_dimension || height > state.max_dimension
        {
            return Err(error(
                "capture",
                "nonzero dimensions within adapter limits are required",
            ));
        }
        let batch = prepare(primitives, scale, width, height, state.max_buffer_size)?;
        state.validate_images(&batch)?;
        let result = state.capture(&batch, width, height);
        if result.is_err() {
            state.failed = true;
        }
        result
    }
}

impl<A: hal::Api> State<A> {
    fn initialize(&mut self) -> Result<(), RenderError> {
        let naga_shader = shader()?;
        // SAFETY: fixed descriptors match validated WGSL: position/UV/color,
        // one sampled texture/sampler group, 32-byte vertices and one sRGB target.
        // Each successful allocation is stored immediately for failure cleanup.
        unsafe {
            self.shader = Some(
                self.device
                    .create_shader_module(
                        &hal::ShaderModuleDescriptor {
                            label: Some("Zest quad"),
                            runtime_checks: wgt::ShaderRuntimeChecks::checked(),
                        },
                        hal::ShaderInput::Naga(naga_shader),
                    )
                    .map_err(|e| error("shader module", e))?,
            );
            self.image_layout = Some(
                self.device
                    .create_bind_group_layout(&hal::BindGroupLayoutDescriptor {
                        label: Some("image layout"),
                        flags: hal::BindGroupLayoutFlags::empty(),
                        entries: &[
                            wgt::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgt::ShaderStages::FRAGMENT,
                                ty: wgt::BindingType::Texture {
                                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                                    view_dimension: wgt::TextureViewDimension::D2,
                                    multisampled: false,
                                },
                                count: None,
                            },
                            wgt::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgt::ShaderStages::FRAGMENT,
                                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                                count: None,
                            },
                        ],
                    })
                    .map_err(|e| error("image layout", e))?,
            );
            self.sampler = Some(
                self.device
                    .create_sampler(&hal::SamplerDescriptor {
                        label: Some("nearest image sampler"),
                        address_modes: [wgt::AddressMode::ClampToEdge; 3],
                        mag_filter: wgt::FilterMode::Nearest,
                        min_filter: wgt::FilterMode::Nearest,
                        mipmap_filter: wgt::MipmapFilterMode::Nearest,
                        lod_clamp: 0.0..0.0,
                        compare: None,
                        anisotropy_clamp: 1,
                        border_color: None,
                    })
                    .map_err(|e| error("image sampler", e))?,
            );
            self.layout = Some(
                self.device
                    .create_pipeline_layout(&hal::PipelineLayoutDescriptor {
                        label: Some("quad layout"),
                        flags: hal::PipelineLayoutFlags::empty(),
                        bind_group_layouts: &[self.image_layout.as_ref()],
                        immediate_size: 0,
                    })
                    .map_err(|e| error("pipeline layout", e))?,
            );
            let constants = naga::back::PipelineConstants::default();
            let stage = |entry_point| hal::ProgrammableStage {
                module: self.shader.as_ref().unwrap(),
                entry_point,
                constants: &constants,
                zero_initialize_workgroup_memory: true,
            };
            self.pipeline = Some(
                self.device
                    .create_render_pipeline(&hal::RenderPipelineDescriptor {
                        label: Some("quad pipeline"),
                        layout: self.layout.as_ref().unwrap(),
                        vertex_processor: hal::VertexProcessor::Standard {
                            vertex_stage: stage("vs_main"),
                            vertex_buffers: &[Some(hal::VertexBufferLayout {
                                array_stride: STRIDE as u64,
                                step_mode: wgt::VertexStepMode::Vertex,
                                attributes: &[
                                    wgt::VertexAttribute {
                                        format: wgt::VertexFormat::Float32x2,
                                        offset: 0,
                                        shader_location: 0,
                                    },
                                    wgt::VertexAttribute {
                                        format: wgt::VertexFormat::Float32x2,
                                        offset: 8,
                                        shader_location: 1,
                                    },
                                    wgt::VertexAttribute {
                                        format: wgt::VertexFormat::Float32x4,
                                        offset: 16,
                                        shader_location: 2,
                                    },
                                ],
                            })],
                        },
                        fragment_stage: Some(stage("fs_main")),
                        primitive: wgt::PrimitiveState::default(),
                        depth_stencil: None,
                        multisample: wgt::MultisampleState::default(),
                        color_targets: &[Some(wgt::ColorTargetState {
                            format: self.config.format,
                            blend: Some(wgt::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                            write_mask: wgt::ColorWrites::ALL,
                        })],
                        multiview_mask: None,
                        cache: None,
                    })
                    .map_err(|e| error("render pipeline", e))?,
            );
            self.fence = Some(self.device.create_fence().map_err(|e| error("fence", e))?);
            for _ in 0..2 {
                self.frames.push(Frame::default());
                let frame = self.frames.last_mut().unwrap();
                frame.encoder = Some(
                    self.device
                        .create_command_encoder(&hal::CommandEncoderDescriptor {
                            label: Some("quad frame"),
                            queue: &self.queue,
                        })
                        .map_err(|e| error("encoder", e))?,
                );
            }
        }
        self.white = Some(self.upload_image(1, 1, &[255; 4])?);
        Ok(())
    }

    fn wait_idle(&mut self) -> Result<(), RenderError> {
        // SAFETY: &mut self cannot overlap any other queue call; the
        // queue is private. This also waits for pending presentation work.
        unsafe { self.queue.wait_for_idle() }.map_err(|e| error("wait for GPU idle", e))
    }

    fn recycle(&mut self, index: usize) {
        let frame = &mut self.frames[index];
        // SAFETY: caller has waited for this slot's submitted fence value or
        // the whole queue. Encoder is closed, and owns exactly this command.
        unsafe {
            if let Some(encoder) = &mut frame.encoder {
                encoder.reset_all(frame.command.take().into_iter());
            }
            if let Some(view) = frame.view.take() {
                self.device.destroy_texture_view(view);
            }
            if let Some(texture) = frame.target.take() {
                self.device.destroy_texture(texture);
            }
            if let Some(buffer) = frame.readback.take() {
                self.device.destroy_buffer(buffer);
            }
            if let Some(buffer) = frame.texture_staging.take() {
                self.device.destroy_buffer(buffer);
            }
        }
        frame.submitted = 0;
    }

    // Caller has waited for this slot and reclaimed its commands and views.
    fn prepare_frame(&mut self, index: usize, bytes: &[u8]) -> Result<(), RenderError> {
        if bytes.is_empty() {
            return Ok(());
        }
        let frame = &mut self.frames[index];
        let size = bytes.len() as u64;
        // SAFETY: CPU generation checked size/vertex limits and 4-byte alignment.
        // No GPU accesses remain for this slot. New buffers reset usage tracking;
        // growing both together prevents stale COPY_SRC/VERTEX state assumptions.
        unsafe {
            if size > frame.capacity {
                if let Some(buffer) = frame.vertex.take() {
                    self.device.destroy_buffer(buffer);
                }
                if let Some(buffer) = frame.upload.take() {
                    self.device.destroy_buffer(buffer);
                }
                frame.initialized = false;
                let capacity = size.next_power_of_two().min(self.max_buffer_size);
                frame.upload = Some(
                    self.device
                        .create_buffer(&hal::BufferDescriptor {
                            label: Some("batch upload"),
                            size: capacity,
                            usage: wgt::BufferUses::MAP_WRITE | wgt::BufferUses::COPY_SRC,
                            memory_flags: hal::MemoryFlags::PREFER_COHERENT,
                        })
                        .map_err(|e| error("upload buffer", e))?,
                );
                frame.vertex = Some(
                    self.device
                        .create_buffer(&hal::BufferDescriptor {
                            label: Some("batch vertices"),
                            size: capacity,
                            usage: wgt::BufferUses::COPY_DST | wgt::BufferUses::VERTEX,
                            memory_flags: hal::MemoryFlags::empty(),
                        })
                        .map_err(|e| error("vertex buffer", e))?,
                );
                frame.capacity = capacity;
            }
            // The nonzero write range fits capacity. Flush covers all writes on
            // noncoherent memory; the pointer is not used after unmapping.
            let upload = frame.upload.as_ref().unwrap();
            let mapping = self
                .device
                .map_buffer(upload, 0..size)
                .map_err(|e| error("map upload", e))?;
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), mapping.ptr.as_ptr(), bytes.len());
            if !mapping.is_coherent {
                self.device
                    .flush_mapped_ranges(upload, std::iter::once(0..size));
            }
            self.device.unmap_buffer(upload);
        }
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if self.failed {
            return Err(error("resize", "terminal HAL error; recreate the renderer"));
        }
        if width > self.max_dimension || height > self.max_dimension {
            return Err(error(
                "resize",
                format!("surface exceeds {} pixels", self.max_dimension),
            ));
        }
        self.wait_idle().inspect_err(|_| self.failed = true)?;
        for index in 0..self.frames.len() {
            self.recycle(index);
        }
        if self.configured {
            // SAFETY: queue idle includes presentations; all acquired textures
            // were consumed, and all views/commands have just been destroyed.
            unsafe {
                self.surface.unconfigure(&self.device);
            }
            self.configured = false;
        }
        self.config.extent.width = width;
        self.config.extent.height = height;
        if width == 0 || height == 0 {
            return Ok(());
        }
        // SAFETY: supported format/modes/usages, nonzero checked dimensions,
        // same device, no outstanding work, views or acquired surface textures.
        unsafe { self.surface.configure(&self.device, &self.config) }
            .map_err(|e| error("configure surface", e))?;
        self.configured = true;
        Ok(())
    }

    fn render(&mut self, batch: &Batch) -> Result<RenderStatus, RenderError> {
        self.collect_retired()?;
        let bytes = &batch.bytes;
        let index = self.frame_index;
        let fence = self.fence.as_ref().unwrap();
        let submitted = self.frames[index].submitted;
        if submitted != 0 {
            // SAFETY: this value was submitted on our sole queue and fence.
            let completed = unsafe { self.device.wait(fence, submitted, None) }
                .map_err(|e| error("frame fence", e))?;
            if !completed {
                return Err(error("frame fence", "completion was not established"));
            }
        }
        self.recycle(index);
        self.prepare_frame(index, bytes)?;
        let frame = &mut self.frames[index];
        let fence = self.fence.as_ref().unwrap();
        let value = self.next_value;
        let next = value
            .checked_add(1)
            .ok_or_else(|| error("submission", "fence value exhausted"))?;
        // SAFETY: configured surface; no previous texture remains acquired;
        // every submission uses this same fence, kept alive through completion.
        let acquired = match unsafe {
            self.surface
                .acquire_texture(Some(Duration::from_millis(250)), fence)
        } {
            Ok(texture) => texture,
            Err(hal::SurfaceError::Timeout | hal::SurfaceError::Occluded) => {
                return Ok(RenderStatus::Skipped);
            }
            Err(hal::SurfaceError::Outdated) => return Ok(RenderStatus::NeedsResize),
            Err(e) => return Err(error("acquire surface", e)),
        };
        // SAFETY: acquired texture is live with the configured format/usage;
        // view is retained in this frame slot until its submission completes.
        let view = unsafe {
            self.device.create_texture_view(
                acquired.texture.borrow(),
                &hal::TextureViewDescriptor {
                    label: Some("quad target"),
                    format: self.config.format,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
        };
        match view {
            Ok(view) => frame.view = Some(view),
            Err(e) => {
                // SAFETY: acquired exactly once, not submitted or presented.
                unsafe {
                    self.surface.discard_texture(acquired.texture);
                }
                return Err(error("target view", e));
            }
        }
        let draws = bind_draws(&self.images, self.white.unwrap(), batch);
        if let Err(e) = encode::<A>(
            frame,
            acquired.texture.borrow(),
            Pass {
                pipeline: self.pipeline.as_ref().unwrap(),
                layout: self.layout.as_ref().unwrap(),
                draws: &draws,
                extent: self.config.extent,
                byte_len: bytes.len(),
                readback: None,
            },
        ) {
            // SAFETY: encoding failed and was discarded; texture is not presented.
            unsafe {
                self.surface.discard_texture(acquired.texture);
            }
            return Err(e);
        }
        // SAFETY: one closed command buffer; resources, view, encoder and fence
        // remain alive. All surface writes are listed here and end in PRESENT.
        let submitted = unsafe {
            self.queue.submit(
                &[frame.command.as_ref().unwrap()],
                &[&acquired.texture],
                (fence, value),
            )
        };
        if let Err(e) = submitted {
            // SAFETY: this acquisition has not been presented or discarded.
            // Renderer becomes terminal; shutdown waits before resource release.
            unsafe {
                self.surface.discard_texture(acquired.texture);
            }
            return Err(error("queue submit", e));
        }
        frame.submitted = value;
        frame.initialized |= !bytes.is_empty();
        let stats = batch_stats(batch, frame.capacity);
        self.next_value = next;
        self.frame_index = (index + 1) % self.frames.len();
        // SAFETY: texture was submitted above in PRESENT state; this consumes
        // the sole acquisition. Queue access is serialized by &mut self.
        unsafe { self.queue.present(&self.surface, acquired.texture) }
            .map_err(|e| error("present", e))?;
        self.mark_images_used(batch, value);
        self.last_stats = stats;
        Ok(RenderStatus::Presented)
    }

    fn capture(&mut self, batch: &Batch, width: u32, height: u32) -> Result<Capture, RenderError> {
        self.collect_retired()?;
        let bytes = &batch.bytes;
        let pitch =
            (u64::from(width) * 4).div_ceil(self.copy_pitch_alignment) * self.copy_pitch_alignment;
        let size = pitch * u64::from(height);
        let pitch = u32::try_from(pitch).map_err(|e| error("capture pitch", e))?;
        if size > self.max_buffer_size {
            return Err(error("capture", "readback exceeds buffer size limit"));
        }
        self.wait_idle()?;
        let index = self.frame_index;
        self.recycle(index);
        self.prepare_frame(index, bytes)?;
        let frame = &mut self.frames[index];
        let extent = wgt::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let value = self.next_value;
        let next = value
            .checked_add(1)
            .ok_or_else(|| error("capture", "fence value exhausted"))?;
        // SAFETY: dimensions/size and format capabilities were checked. Target
        // and readback are stored before any subsequent fallible operation.
        unsafe {
            frame.target = Some(
                self.device
                    .create_texture(&hal::TextureDescriptor {
                        label: Some("quad capture"),
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgt::TextureDimension::D2,
                        format: self.config.format,
                        usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
                        memory_flags: hal::MemoryFlags::empty(),
                        view_formats: vec![],
                    })
                    .map_err(|e| error("capture texture", e))?,
            );
            frame.view = Some(
                self.device
                    .create_texture_view(
                        frame.target.as_ref().unwrap(),
                        &hal::TextureViewDescriptor {
                            label: Some("capture target"),
                            format: self.config.format,
                            dimension: wgt::TextureViewDimension::D2,
                            usage: wgt::TextureUses::COLOR_TARGET,
                            range: wgt::ImageSubresourceRange::default(),
                        },
                    )
                    .map_err(|e| error("capture view", e))?,
            );
            frame.readback = Some(
                self.device
                    .create_buffer(&hal::BufferDescriptor {
                        label: Some("quad readback"),
                        size,
                        usage: wgt::BufferUses::MAP_READ | wgt::BufferUses::COPY_DST,
                        memory_flags: hal::MemoryFlags::PREFER_COHERENT,
                    })
                    .map_err(|e| error("readback buffer", e))?,
            );
        }
        // Split borrows: target/readback stay owned in this slot throughout GPU use.
        let target = frame.target.take().unwrap();
        let readback = frame.readback.take().unwrap();
        let draws = bind_draws(&self.images, self.white.unwrap(), batch);
        let encoded = encode::<A>(
            frame,
            &target,
            Pass {
                pipeline: self.pipeline.as_ref().unwrap(),
                layout: self.layout.as_ref().unwrap(),
                draws: &draws,
                extent,
                byte_len: bytes.len(),
                readback: Some((&readback, pitch)),
            },
        );
        frame.target = Some(target);
        frame.readback = Some(readback);
        encoded?;
        let fence = self.fence.as_ref().unwrap();
        // SAFETY: closed command, live owned resources and monotonically
        // increasing shared fence. This is offscreen and uses no surface texture.
        unsafe {
            self.queue
                .submit(&[frame.command.as_ref().unwrap()], &[], (fence, value))
        }
        .map_err(|e| error("capture submit", e))?;
        frame.submitted = value;
        frame.initialized |= !bytes.is_empty();
        self.next_value = next;
        // SAFETY: waits for the submission above before reading GPU writes.
        let completed = unsafe { self.device.wait(fence, value, None) }
            .map_err(|e| error("capture fence", e))?;
        if !completed {
            return Err(error("capture fence", "completion was not established"));
        }
        let mut packed = vec![0; (width as usize) * (height as usize) * 4];
        // SAFETY: MAP_READ range covers the allocation; GPU copy has completed.
        // Invalidate precedes reads on noncoherent memory. Rows fit both buffers,
        // and the mapped pointer is not used after unmap.
        unsafe {
            let buffer = frame.readback.as_ref().unwrap();
            let mapping = self
                .device
                .map_buffer(buffer, 0..size)
                .map_err(|e| error("map readback", e))?;
            if !mapping.is_coherent {
                self.device
                    .invalidate_mapped_ranges(buffer, std::iter::once(0..size));
            }
            for row in 0..height as usize {
                std::ptr::copy_nonoverlapping(
                    mapping.ptr.as_ptr().add(row * pitch as usize),
                    packed.as_mut_ptr().add(row * width as usize * 4),
                    width as usize * 4,
                );
            }
            self.device.unmap_buffer(buffer);
        }
        if self.config.format == wgt::TextureFormat::Bgra8UnormSrgb {
            for pixel in packed.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        self.recycle(index);
        self.last_stats = batch_stats(batch, self.frames[index].capacity);
        self.mark_images_used(batch, value);
        Ok(Capture {
            width,
            height,
            rgba8: packed,
        })
    }

    fn release(&mut self) {
        for index in 0..self.frames.len() {
            self.recycle(index);
        }
        for image in self.images.drain().chain(self.retired.drain(..)) {
            images::destroy_image::<A>(&self.device, image);
        }
        // SAFETY: caller proved queue idle, so nothing references any resource.
        // Commands/views are recycled first, then buffers and encoders, then
        // pipelines/layout/shader/fence; unconfigure precedes device destruction.
        unsafe {
            for frame in &mut self.frames {
                if let Some(buffer) = frame.vertex.take() {
                    self.device.destroy_buffer(buffer);
                }
                if let Some(buffer) = frame.upload.take() {
                    self.device.destroy_buffer(buffer);
                }
                drop(frame.encoder.take());
            }
            if let Some(pipeline) = self.pipeline.take() {
                self.device.destroy_render_pipeline(pipeline);
            }
            if let Some(layout) = self.layout.take() {
                self.device.destroy_pipeline_layout(layout);
            }
            if let Some(layout) = self.image_layout.take() {
                self.device.destroy_bind_group_layout(layout);
            }
            if let Some(sampler) = self.sampler.take() {
                self.device.destroy_sampler(sampler);
            }
            if let Some(shader) = self.shader.take() {
                self.device.destroy_shader_module(shader);
            }
            if let Some(fence) = self.fence.take() {
                self.device.destroy_fence(fence);
            }
            if self.configured {
                self.surface.unconfigure(&self.device);
                self.configured = false;
            }
        }
    }
}

fn batch_stats(batch: &Batch, capacity: u64) -> FrameStats {
    let bytes = &batch.bytes;
    FrameStats {
        quads: bytes.len() / VERTEX_BYTES,
        draw_calls: batch.draws.len() as u32,
        uploaded_bytes: bytes.len() as u64,
        vertex_capacity: capacity,
    }
}

struct BoundDraw<'a, A: hal::Api> {
    group: &'a A::BindGroup,
    first: u32,
    count: u32,
}
fn bind_draws<'a, A: hal::Api>(
    images: &'a ResourceArena<ImageResource, TextureResource<A>>,
    white: ImageId,
    batch: &Batch,
) -> Vec<BoundDraw<'a, A>> {
    batch
        .draws
        .iter()
        .map(|draw| BoundDraw {
            group: images
                .get(&draw.image.unwrap_or(white))
                .expect("validated image")
                .group
                .as_ref()
                .expect("initialized image"),
            first: draw.first,
            count: draw.count,
        })
        .collect()
}
struct Pass<'a, A: hal::Api> {
    pipeline: &'a A::RenderPipeline,
    layout: &'a A::PipelineLayout,
    draws: &'a [BoundDraw<'a, A>],
    extent: wgt::Extent3d,
    byte_len: usize,
    readback: Option<(&'a A::Buffer, u32)>,
}

fn encode<A: hal::Api>(
    frame: &mut Frame<A>,
    target: &A::Texture,
    pass: Pass<'_, A>,
) -> Result<(), RenderError> {
    let Pass {
        pipeline,
        layout,
        draws,
        extent,
        byte_len,
        readback,
    } = pass;
    let encoder = frame.encoder.as_mut().unwrap();
    // SAFETY: encoder is closed and all old commands were reclaimed after fence
    // completion. Every barrier matches our tracked per-slot resource usage.
    unsafe { encoder.begin_encoding(Some("Zest quad frame")) }
        .map_err(|e| error("begin encoding", e))?;
    // SAFETY: checked buffer sizes/offsets, COPY_SRC/COPY_DST usage, aligned
    // copies. Render pass uses a live matching view, one sample and no depth.
    // The complete vertex batch fits its binding. Texture groups match the
    // pipeline layout and refer to owned images in RESOURCE state; no indices.
    let result = unsafe {
        if byte_len != 0 {
            encoder.transition_buffers(
                [
                    hal::BufferBarrier {
                        buffer: frame.upload.as_ref().unwrap(),
                        usage: hal::StateTransition {
                            from: if frame.initialized {
                                wgt::BufferUses::COPY_SRC
                            } else {
                                wgt::BufferUses::empty()
                            },
                            to: wgt::BufferUses::COPY_SRC,
                        },
                    },
                    hal::BufferBarrier {
                        buffer: frame.vertex.as_ref().unwrap(),
                        usage: hal::StateTransition {
                            from: if frame.initialized {
                                wgt::BufferUses::VERTEX
                            } else {
                                wgt::BufferUses::empty()
                            },
                            to: wgt::BufferUses::COPY_DST,
                        },
                    },
                ]
                .into_iter(),
            );
            encoder.copy_buffer_to_buffer(
                frame.upload.as_ref().unwrap(),
                frame.vertex.as_ref().unwrap(),
                std::iter::once(hal::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: NonZeroU64::new(byte_len as u64).unwrap(),
                }),
            );
            encoder.transition_buffers(std::iter::once(hal::BufferBarrier {
                buffer: frame.vertex.as_ref().unwrap(),
                usage: hal::StateTransition {
                    from: wgt::BufferUses::COPY_DST,
                    to: wgt::BufferUses::VERTEX,
                },
            }));
        }
        encoder.transition_textures(std::iter::once(hal::TextureBarrier {
            texture: target,
            range: wgt::ImageSubresourceRange::default(),
            usage: hal::StateTransition {
                from: wgt::TextureUses::UNINITIALIZED,
                to: wgt::TextureUses::COLOR_TARGET,
            },
        }));
        let pass = encoder.begin_render_pass(&hal::RenderPassDescriptor {
            label: Some("quad"),
            extent,
            sample_count: 1,
            color_attachments: &[Some(hal::ColorAttachment {
                target: hal::Attachment {
                    view: frame.view.as_ref().unwrap(),
                    usage: wgt::TextureUses::COLOR_TARGET,
                },
                depth_slice: None,
                resolve_target: None,
                ops: hal::AttachmentOps::LOAD_CLEAR | hal::AttachmentOps::STORE,
                clear_value: wgt::Color {
                    r: 0.015,
                    g: 0.02,
                    b: 0.03,
                    a: 1.0,
                },
            })],
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        match pass {
            Err(e) => Err(error("begin render pass", e)),
            Ok(()) => {
                encoder.set_render_pipeline(pipeline);
                encoder.set_viewport(
                    &hal::Rect {
                        x: 0.0,
                        y: 0.0,
                        w: extent.width as f32,
                        h: extent.height as f32,
                    },
                    0.0..1.0,
                );
                encoder.set_scissor_rect(&hal::Rect {
                    x: 0,
                    y: 0,
                    w: extent.width,
                    h: extent.height,
                });
                if byte_len != 0 {
                    encoder.set_vertex_buffer(
                        0,
                        hal::BufferBinding::new_unchecked(
                            frame.vertex.as_ref().unwrap(),
                            0,
                            NonZeroU64::new(byte_len as u64),
                        ),
                    );
                    for draw in draws {
                        encoder.set_bind_group(layout, 0, draw.group, &[]);
                        encoder.draw(draw.first, draw.count, 0, 1);
                    }
                }
                encoder.end_render_pass();
                encoder.transition_textures(std::iter::once(hal::TextureBarrier {
                    texture: target,
                    range: wgt::ImageSubresourceRange::default(),
                    usage: hal::StateTransition {
                        from: wgt::TextureUses::COLOR_TARGET,
                        to: if readback.is_some() {
                            wgt::TextureUses::COPY_SRC
                        } else {
                            wgt::TextureUses::PRESENT
                        },
                    },
                }));
                if let Some((buffer, pitch)) = readback {
                    encoder.transition_buffers(std::iter::once(hal::BufferBarrier {
                        buffer,
                        usage: hal::StateTransition {
                            from: wgt::BufferUses::empty(),
                            to: wgt::BufferUses::COPY_DST,
                        },
                    }));
                    encoder.copy_texture_to_buffer(
                        target,
                        wgt::TextureUses::COPY_SRC,
                        buffer,
                        std::iter::once(hal::BufferTextureCopy {
                            buffer_layout: wgt::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(pitch),
                                rows_per_image: Some(extent.height),
                            },
                            texture_base: hal::TextureCopyBase {
                                mip_level: 0,
                                array_layer: 0,
                                origin: wgt::Origin3d::ZERO,
                                aspect: hal::FormatAspects::COLOR,
                            },
                            size: extent.into(),
                        }),
                    );
                    encoder.transition_buffers(std::iter::once(hal::BufferBarrier {
                        buffer,
                        usage: hal::StateTransition {
                            from: wgt::BufferUses::COPY_DST,
                            to: wgt::BufferUses::MAP_READ,
                        },
                    }));
                }
                encoder.end_encoding().map_err(|e| error("end encoding", e))
            }
        }
    };
    match result {
        Ok(command) => {
            frame.command = Some(command);
            Ok(())
        }
        Err(e) => {
            // SAFETY: an error occurred after begin_encoding; HAL requires
            // discarding the recording before any reuse or destruction.
            unsafe {
                encoder.discard_encoding();
            }
            Err(e)
        }
    }
}

impl<A: hal::Api> Drop for Gpu<A> {
    fn drop(&mut self) {
        let Some(mut state) = self.state.take() else {
            return;
        };
        if let Err(e) = state.wait_idle() {
            // Completion cannot be proved after a device/driver error. Retain
            // the entire owner (including window) rather than free live storage.
            eprintln!("zest-gpu: {e}; retaining native resources until process exit");
            std::mem::forget(state);
            return;
        }
        state.release();
    }
}
