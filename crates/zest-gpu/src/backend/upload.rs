use super::*;
use crate::{
    UploadStats,
    upload_ring::{Reservation, Ring},
};

pub(super) fn validate_capacity(
    capacity: u64,
    max_buffer: u64,
    alignment: u64,
) -> Result<(), RenderError> {
    if !capacity.is_power_of_two()
        || capacity < 256
        || capacity > max_buffer
        || !alignment.is_power_of_two()
        || alignment > capacity
    {
        return Err(error(
            "upload ring",
            "capacity must be a power of two, at least 256 bytes, aligned for this adapter and within its buffer limit",
        ));
    }
    Ok(())
}

pub(super) struct UploadRing<A: hal::Api> {
    pub buffer: Option<A::Buffer>,
    pub initialized: bool,
    allocator: Ring,
    alignment: u64,
    counters: UploadStats,
}
impl<A: hal::Api> UploadRing<A> {
    pub fn new(capacity: u64, alignment: u64) -> Self {
        Self {
            buffer: None,
            initialized: false,
            allocator: Ring::new(capacity),
            alignment,
            counters: UploadStats {
                capacity,
                ..UploadStats::default()
            },
        }
    }
    pub fn stats(&self) -> UploadStats {
        UploadStats {
            pending_bytes: self.allocator.used(),
            ..self.counters
        }
    }
    pub fn commit(&mut self, reservation: Reservation, fence: u64) -> Result<(), RenderError> {
        if !self.allocator.commit(reservation, fence) {
            return Err(error(
                "upload ring",
                "invalid reservation or submission order",
            ));
        }
        self.initialized = true;
        self.counters.ring_uploads += 1;
        self.counters.staged_bytes += reservation.size;
        self.counters.wraps += u64::from(reservation.wrapped);
        self.counters.peak_pending_bytes =
            self.counters.peak_pending_bytes.max(self.allocator.used());
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) struct Pixels<'a> {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub size: u64,
    pub data: &'a [u8],
}

struct Mapped<'a, A: hal::Api> {
    device: &'a A::Device,
    buffer: &'a A::Buffer,
}
impl<A: hal::Api> Drop for Mapped<'_, A> {
    fn drop(&mut self) {
        // SAFETY: guard is created only after map_buffer succeeds, and is the
        // sole owner of unmapping this mapping, including on error/unwind.
        unsafe {
            self.device.unmap_buffer(self.buffer);
        }
    }
}

fn write_pixels<A: hal::Api>(
    device: &A::Device,
    buffer: &A::Buffer,
    offset: u64,
    pixels: Pixels<'_>,
    coherent: Option<bool>,
) -> Result<bool, RenderError> {
    let end = offset
        .checked_add(pixels.size)
        .ok_or_else(|| error("upload mapping", "range overflow"))?;
    // SAFETY: caller validated byte count/pitch and the owned buffer range, and
    // proved no GPU access overlaps this allocation. Ring offsets satisfy the
    // adapter's copy alignment. Fresh dedicated buffers have no prior GPU use.
    let mapping = unsafe { device.map_buffer(buffer, offset..end) }
        .map_err(|e| error("upload mapping", e))?;
    let guard = Mapped::<A> { device, buffer };
    if coherent.is_some_and(|expected| expected != mapping.is_coherent) {
        return Err(error(
            "upload mapping",
            "buffer coherence changed between mappings",
        ));
    }
    // SAFETY: ptr starts at the mapped range, all rows fit that range and the
    // source slice. Padding is initialized too. Noncoherent ring writes occur
    // only after all previous ring reads complete, avoiding atom-granularity
    // flush overlap with pending GPU reads. Guard unmaps before return.
    unsafe {
        std::ptr::write_bytes(mapping.ptr.as_ptr(), 0, pixels.size as usize);
        for row in 0..pixels.height as usize {
            std::ptr::copy_nonoverlapping(
                pixels.data.as_ptr().add(row * pixels.width as usize * 4),
                mapping.ptr.as_ptr().add(row * pixels.pitch as usize),
                pixels.width as usize * 4,
            );
        }
        if !mapping.is_coherent {
            device.flush_mapped_ranges(buffer, std::iter::once(offset..end));
        }
    }
    drop(guard);
    Ok(mapping.is_coherent)
}

impl<A: hal::Api> Gpu<A> {
    pub(crate) fn upload_stats(&self) -> UploadStats {
        self.state.as_ref().unwrap().upload_ring.stats()
    }
}

impl<A: hal::Api> State<A> {
    pub(super) fn reclaim_uploads(&mut self) -> Result<(), RenderError> {
        if self.upload_ring.allocator.oldest_fence().is_none() {
            return Ok(());
        }
        // SAFETY: all committed ranges correspond to successful submissions on
        // this shared fence. Querying completion does not wait or reset it.
        let completed = unsafe { self.device.get_fence_value(self.fence.as_ref().unwrap()) }
            .map_err(|e| error("upload ring fence", e))?;
        self.upload_ring.allocator.reclaim(completed);
        Ok(())
    }
    fn wait_upload(&mut self, value: u64) -> Result<(), RenderError> {
        self.upload_ring.counters.waits += 1;
        // SAFETY: value is taken only from committed (successfully submitted)
        // ring ranges. &mut self serializes all queue and allocator operations.
        let completed = unsafe { self.device.wait(self.fence.as_ref().unwrap(), value, None) }
            .map_err(|e| error("upload ring wait", e))?;
        if !completed {
            return Err(error("upload ring wait", "completion was not established"));
        }
        self.upload_ring.allocator.reclaim(value);
        Ok(())
    }
    /// Returns None for an oversized dedicated staging allocation. A returned
    /// ring reservation becomes live only after the caller submits and commits.
    pub(super) fn stage_texture(
        &mut self,
        index: usize,
        pixels: Pixels<'_>,
    ) -> Result<Option<Reservation>, RenderError> {
        if pixels.size > self.upload_ring.allocator.capacity() {
            // SAFETY: upload_layout checked nonzero size and the buffer limit;
            // this frame slot was waited/recycled before entering this method.
            let buffer = unsafe {
                self.device.create_buffer(&hal::BufferDescriptor {
                    label: Some("oversized texture staging"),
                    size: pixels.size,
                    usage: wgt::BufferUses::MAP_WRITE | wgt::BufferUses::COPY_SRC,
                    memory_flags: hal::MemoryFlags::PREFER_COHERENT,
                })
            }
            .map_err(|e| error("dedicated staging", e))?;
            self.frames[index].texture_staging = Some(buffer);
            self.upload_ring.counters.dedicated_allocations += 1;
            write_pixels::<A>(
                &self.device,
                self.frames[index].texture_staging.as_ref().unwrap(),
                0,
                pixels,
                None,
            )?;
            return Ok(None);
        }
        self.reclaim_uploads()?;
        if self.upload_ring.counters.coherent == Some(false)
            && let Some(last) = self.upload_ring.allocator.latest_fence()
        {
            self.wait_upload(last)?;
        }
        let reservation = loop {
            if let Some(range) = self
                .upload_ring
                .allocator
                .reserve(pixels.size, self.upload_ring.alignment)
            {
                break range;
            }
            let oldest = self
                .upload_ring
                .allocator
                .oldest_fence()
                .ok_or_else(|| error("upload ring", "reservation counters exhausted"))?;
            self.wait_upload(oldest)?;
        };
        if self.upload_ring.buffer.is_none() {
            // SAFETY: capacity/alignment were checked before device creation.
            // The ring owns this allocation until shutdown after queue idle.
            self.upload_ring.buffer = Some(
                unsafe {
                    self.device.create_buffer(&hal::BufferDescriptor {
                        label: Some("texture upload ring"),
                        size: self.upload_ring.allocator.capacity(),
                        usage: wgt::BufferUses::MAP_WRITE | wgt::BufferUses::COPY_SRC,
                        memory_flags: hal::MemoryFlags::PREFER_COHERENT,
                    })
                }
                .map_err(|e| error("upload ring buffer", e))?,
            );
            self.upload_ring.counters.buffer_allocations += 1;
        }
        let coherent = write_pixels::<A>(
            &self.device,
            self.upload_ring.buffer.as_ref().unwrap(),
            reservation.offset,
            pixels,
            self.upload_ring.counters.coherent,
        )?;
        self.upload_ring.counters.coherent = Some(coherent);
        Ok(Some(reservation))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_validation_rejects_bad_limits_before_allocation() {
        for capacity in [0, 128, 300, 8192] {
            assert!(validate_capacity(capacity, 4096, 256).is_err());
        }
        assert!(validate_capacity(256, 4096, 512).is_err());
        assert!(validate_capacity(1024, 4096, 3).is_err());
        assert!(validate_capacity(1024, 4096, 256).is_ok());
    }
}
