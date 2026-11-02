use super::*;

pub(super) fn check(renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let before = renderer.upload_stats();
    let live = renderer.image_stats().live;
    let mut images = Vec::new();
    // Keep every texture alive while repeatedly wrapping its shared source
    // buffer. The later grid verifies old uploads survived reuse of their bytes.
    for index in 0..64u32 {
        let color = [
            (index * 71 % 256) as u8,
            (index * 43 % 256) as u8,
            (index * 113 % 256) as u8,
            255,
        ];
        let id = renderer.upload_rgba8(17, 3, &color.repeat(17 * 3))?;
        images.push((id, color));
    }
    let list = DisplayList::build(|p| {
        for (index, &(id, _)) in images.iter().enumerate() {
            p.image(Image::new(
                id,
                Rect::from_xywh((index % 8 * 4) as f64, (index / 8 * 4) as f64, 4.0, 4.0),
            ))?;
        }
        Ok(())
    })?;
    let capture = renderer.capture_list(&list, ScaleFactor::default(), 37, 35)?;
    for (index, &(_, color)) in images.iter().enumerate() {
        expect_pixel(
            &capture,
            (index % 8 * 4 + 1) as u32,
            (index / 8 * 4 + 1) as u32,
            color,
        )?;
    }
    if renderer.last_frame_stats().draw_calls != 64 {
        return Err("upload grid lost an image material".into());
    }
    for (id, _) in images {
        renderer.remove_image(id)?;
    }
    renderer.wait_idle()?;
    let wrapped = renderer.upload_stats();
    if wrapped.capacity != 4096
        || wrapped.buffer_allocations != 1
        || wrapped.ring_uploads < before.ring_uploads + 64
        || wrapped.wraps <= before.wraps
        || wrapped.pending_bytes != 0
        || wrapped.peak_pending_bytes > wrapped.capacity
    {
        return Err(format!("upload ring failed its wrap/reuse checks: {wrapped:?}").into());
    }
    // Exact-capacity wrap after reclaim must not charge the unused tail gap.
    let width = (wrapped.capacity / 4) as u32;
    let boundary = renderer.upload_rgba8(width, 1, &[199, 22, 100, 255].repeat(width as usize))?;
    let list = DisplayList::build(|p| {
        p.image(Image::new(boundary, Rect::from_xywh(0.0, 0.0, 16.0, 8.0)))
    })?;
    let capture = renderer.capture_list(&list, ScaleFactor::default(), 17, 9)?;
    expect_pixel(&capture, 4, 4, [199, 22, 100, 255])?;
    renderer.remove_image(boundary)?;
    renderer.wait_idle()?;
    // Raw bytes alone exceed the ring regardless of row-pitch alignment.
    let width = (wrapped.capacity / 4 + 1) as u32;
    let id = renderer.upload_rgba8(width, 1, &[64, 128, 192, 255].repeat(width as usize))?;
    let list =
        DisplayList::build(|p| p.image(Image::new(id, Rect::from_xywh(0.0, 0.0, 16.0, 8.0))))?;
    let capture = renderer.capture_list(&list, ScaleFactor::default(), 17, 9)?;
    expect_pixel(&capture, 4, 4, [64, 128, 192, 255])?;
    renderer.remove_image(id)?;
    let tail = renderer.upload_rgba8(1, 1, &[231, 7, 80, 255])?;
    let list =
        DisplayList::build(|p| p.image(Image::new(tail, Rect::from_xywh(0.0, 0.0, 16.0, 8.0))))?;
    let capture = renderer.capture_list(&list, ScaleFactor::default(), 17, 9)?;
    expect_pixel(&capture, 4, 4, [231, 7, 80, 255])?;
    renderer.remove_image(tail)?;
    renderer.wait_idle()?;
    let after = renderer.upload_stats();
    if after.dedicated_allocations != before.dedicated_allocations + 1
        || after.buffer_allocations != 1
        || after.pending_bytes != 0
        || renderer.image_stats().live != live
        || renderer.image_stats().retired != 0
    {
        return Err(format!("dedicated fallback or staging retirement failed: {after:?}").into());
    }
    println!(
        "GPU upload checks passed: one {}-byte ring, {} wraps, {} fence waits, 64 retained images verified and oversized fallback; coherent={:?}",
        after.capacity, after.wraps, after.waits, after.coherent
    );
    Ok(())
}
