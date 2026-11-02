use super::*;
use crate::ImageStats;

pub(super) struct TextureResource<A: hal::Api> {
    pub texture: Option<A::Texture>,
    pub view: Option<A::TextureView>,
    pub group: Option<A::BindGroup>,
    pub last_used: u64,
}
impl<A: hal::Api> Default for TextureResource<A> {
    fn default() -> Self {
        Self {
            texture: None,
            view: None,
            group: None,
            last_used: 0,
        }
    }
}

fn upload_layout(
    width: u32,
    height: u32,
    pixels: &[u8],
    max_dimension: u32,
    max_buffer: u64,
    alignment: u64,
) -> Result<(u32, u64), RenderError> {
    if width == 0
        || height == 0
        || width > max_dimension
        || height > max_dimension
        || u64::from(width) * u64::from(height) * 4 != pixels.len() as u64
    {
        return Err(error(
            "image upload",
            "nonzero dimensions within adapter limits and exactly width*height*4 bytes are required",
        ));
    }
    let pitch = (u64::from(width) * 4).div_ceil(alignment) * alignment;
    let size = pitch * u64::from(height);
    if size > max_buffer {
        return Err(error("image upload", "padded pixels exceed buffer limit"));
    }
    Ok((
        u32::try_from(pitch).map_err(|e| error("image row pitch", e))?,
        size,
    ))
}

impl<A: hal::Api> Gpu<A> {
    pub(crate) fn image_stats(&self) -> ImageStats {
        let state = self.state.as_ref().unwrap();
        ImageStats {
            live: state.images.len() - usize::from(state.white.is_some()),
            retired: state.retired.len(),
        }
    }
    pub(crate) fn upload_rgba8(
        &mut self,
        width: u32,
        height: u32,
        rgba8: &[u8],
    ) -> Result<ImageId, RenderError> {
        let state = self.state.as_mut().unwrap();
        if state.failed {
            return Err(error(
                "image upload",
                "terminal HAL error; recreate the renderer",
            ));
        }
        upload_layout(
            width,
            height,
            rgba8,
            state.max_dimension,
            state.max_buffer_size,
            state.copy_pitch_alignment,
        )?;
        let result = state.upload_image(width, height, rgba8);
        if result.is_err() {
            state.failed = true;
        }
        result
    }
    pub(crate) fn remove_image(&mut self, id: ImageId) -> Result<bool, RenderError> {
        let state = self.state.as_mut().unwrap();
        if state.failed {
            return Err(error(
                "image removal",
                "terminal HAL error; recreate the renderer",
            ));
        }
        let Some(image) = state.images.remove(&id) else {
            return Ok(false);
        };
        state.retired.push(image);
        Ok(true)
    }
}

impl<A: hal::Api> State<A> {
    pub(super) fn validate_images(&self, batch: &Batch) -> Result<(), RenderError> {
        for draw in &batch.draws {
            if let Some(id) = draw.image
                && self.images.get(&id).is_none()
            {
                return Err(error(
                    "display list",
                    format!("stale or foreign image handle: {id:?}"),
                ));
            }
        }
        Ok(())
    }
    pub(super) fn mark_images_used(&mut self, batch: &Batch, value: u64) {
        for draw in &batch.draws {
            self.images
                .get_mut(&draw.image.unwrap_or(self.white.unwrap()))
                .expect("validated image")
                .last_used = value;
        }
    }
    pub(super) fn collect_retired(&mut self) -> Result<(), RenderError> {
        if self.retired.is_empty() {
            return Ok(());
        }
        // SAFETY: this is our live queue fence, and all image uses signal it.
        let completed = unsafe { self.device.get_fence_value(self.fence.as_ref().unwrap()) }
            .map_err(|e| error("image retirement fence", e))?;
        let mut index = 0;
        while index < self.retired.len() {
            if self.retired[index].last_used <= completed {
                destroy_image::<A>(&self.device, self.retired.swap_remove(index));
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    pub(super) fn upload_image(
        &mut self,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<ImageId, RenderError> {
        let (pitch, size) = upload_layout(
            width,
            height,
            pixels,
            self.max_dimension,
            self.max_buffer_size,
            self.copy_pitch_alignment,
        )?;
        self.collect_retired()?;
        let index = self.frame_index;
        let fence = self.fence.as_ref().unwrap();
        let submitted = self.frames[index].submitted;
        if submitted != 0 {
            // SAFETY: this value was submitted by this renderer on this fence.
            if !unsafe { self.device.wait(fence, submitted, None) }
                .map_err(|e| error("image upload slot", e))?
            {
                return Err(error("image upload", "slot completion was not established"));
            }
        }
        self.recycle(index);
        let value = self.next_value;
        let next = value
            .checked_add(1)
            .ok_or_else(|| error("image upload", "fence value exhausted"))?;
        let staging = self.stage_texture(
            index,
            upload::Pixels {
                width,
                height,
                pitch,
                size,
                data: pixels,
            },
        )?;
        // Store partially created resources in the owner before fallible calls.
        // Any HAL failure makes the renderer terminal; shutdown waits/cleans all.
        let id = self.images.insert(TextureResource::default());
        let image = self.images.get_mut(&id).unwrap();
        let frame = &mut self.frames[index];
        // SAFETY: checked dimensions, byte count, row pitch and buffer limits.
        // Staging is prepared above, with its range protected until submission
        // completion. The texture gets one initialized mip/layer before use.
        unsafe {
            image.texture = Some(
                self.device
                    .create_texture(&hal::TextureDescriptor {
                        label: Some("RGBA8 image"),
                        size: wgt::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgt::TextureDimension::D2,
                        format: wgt::TextureFormat::Rgba8UnormSrgb,
                        usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE,
                        memory_flags: hal::MemoryFlags::empty(),
                        view_formats: vec![],
                    })
                    .map_err(|e| error("image texture", e))?,
            );
            image.view = Some(
                self.device
                    .create_texture_view(
                        image.texture.as_ref().unwrap(),
                        &hal::TextureViewDescriptor {
                            label: Some("image view"),
                            format: wgt::TextureFormat::Rgba8UnormSrgb,
                            dimension: wgt::TextureViewDimension::D2,
                            usage: wgt::TextureUses::RESOURCE,
                            range: wgt::ImageSubresourceRange::default(),
                        },
                    )
                    .map_err(|e| error("image view", e))?,
            );
            image.group = Some(
                self.device
                    .create_bind_group(&hal::BindGroupDescriptor {
                        label: Some("image group"),
                        layout: self.image_layout.as_ref().unwrap(),
                        buffers: &[],
                        samplers: &[self.sampler.as_ref().unwrap()],
                        textures: &[hal::TextureBinding {
                            view: image.view.as_ref().unwrap(),
                            usage: wgt::TextureUses::RESOURCE,
                        }],
                        acceleration_structures: &[],
                        external_textures: &[],
                        entries: &[
                            hal::BindGroupEntry {
                                binding: 0,
                                resource_index: 0,
                                count: 1,
                            },
                            hal::BindGroupEntry {
                                binding: 1,
                                resource_index: 0,
                                count: 1,
                            },
                        ],
                    })
                    .map_err(|e| error("image group", e))?,
            );
        }
        let source = if staging.is_some() {
            self.upload_ring.buffer.as_ref().unwrap()
        } else {
            frame.texture_staging.as_ref().unwrap()
        };
        let offset = staging.map_or(0, |range| range.offset);
        let source_initialized = staging.is_some() && self.upload_ring.initialized;
        let encoder = frame.encoder.as_mut().unwrap();
        // SAFETY: encoder is closed and all its previous commands were recycled.
        unsafe { encoder.begin_encoding(Some("upload image")) }
            .map_err(|e| error("image encoding", e))?;
        // SAFETY: aligned in-bounds copy from initialized staging bytes, fresh
        // texture transitioned from UNINITIALIZED through COPY_DST to RESOURCE.
        let command = unsafe {
            let texture = image.texture.as_ref().unwrap();
            encoder.transition_buffers(std::iter::once(hal::BufferBarrier {
                buffer: source,
                usage: hal::StateTransition {
                    from: if source_initialized {
                        wgt::BufferUses::COPY_SRC
                    } else {
                        wgt::BufferUses::empty()
                    },
                    to: wgt::BufferUses::COPY_SRC,
                },
            }));
            encoder.transition_textures(std::iter::once(hal::TextureBarrier {
                texture,
                range: wgt::ImageSubresourceRange::default(),
                usage: hal::StateTransition {
                    from: wgt::TextureUses::UNINITIALIZED,
                    to: wgt::TextureUses::COPY_DST,
                },
            }));
            encoder.copy_buffer_to_texture(
                source,
                texture,
                std::iter::once(hal::BufferTextureCopy {
                    buffer_layout: wgt::TexelCopyBufferLayout {
                        offset,
                        bytes_per_row: Some(pitch),
                        rows_per_image: Some(height),
                    },
                    texture_base: hal::TextureCopyBase {
                        mip_level: 0,
                        array_layer: 0,
                        origin: wgt::Origin3d::ZERO,
                        aspect: hal::FormatAspects::COLOR,
                    },
                    size: hal::CopyExtent {
                        width,
                        height,
                        depth: 1,
                    },
                }),
            );
            encoder.transition_textures(std::iter::once(hal::TextureBarrier {
                texture,
                range: wgt::ImageSubresourceRange::default(),
                usage: hal::StateTransition {
                    from: wgt::TextureUses::COPY_DST,
                    to: wgt::TextureUses::RESOURCE,
                },
            }));
            encoder.end_encoding()
        };
        frame.command = Some(match command {
            Ok(command) => command,
            Err(e) => {
                // SAFETY: HAL requires discarding a failed recording.
                unsafe {
                    encoder.discard_encoding();
                }
                return Err(error("image command", e));
            }
        });
        // SAFETY: all resources/fence/encoder remain owned. Same serialized
        // queue and increasing fence values order upload before later draws.
        unsafe {
            self.queue.submit(
                &[frame.command.as_ref().unwrap()],
                &[],
                (self.fence.as_ref().unwrap(), value),
            )
        }
        .map_err(|e| error("image submit", e))?;
        frame.submitted = value;
        image.last_used = value;
        if let Some(range) = staging {
            self.upload_ring.commit(range, value)?;
        }
        self.next_value = next;
        self.frame_index = (index + 1) % self.frames.len();
        Ok(id)
    }
}

pub(super) fn destroy_image<A: hal::Api>(device: &A::Device, mut image: TextureResource<A>) {
    // SAFETY: callers either waited for queue idle (shutdown), or proved the
    // image's last submission completed. Groups precede views and textures.
    unsafe {
        if let Some(group) = image.group.take() {
            device.destroy_bind_group(group);
        }
        if let Some(view) = image.view.take() {
            device.destroy_texture_view(view);
        }
        if let Some(texture) = image.texture.take() {
            device.destroy_texture(texture);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upload_layout_checks_dimensions_bytes_and_padded_limits() {
        assert_eq!(
            upload_layout(3, 2, &[0; 24], 1024, 1024, 256).unwrap(),
            (256, 512)
        );
        assert!(upload_layout(3, 2, &[0; 23], 1024, 1024, 256).is_err());
        assert!(upload_layout(3, 2, &[0; 24], 1024, 256, 256).is_err());
        assert!(upload_layout(0, 0, &[], 1024, 1024, 256).is_err());
        assert!(upload_layout(3, 2, &[0; 24], 2, 1024, 256).is_err());
    }
}
