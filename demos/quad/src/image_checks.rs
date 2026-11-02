use super::*;
use zest::{
    paint::{ImageResource, Uv},
    resource::ResourceArena,
};

pub(super) fn check(renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let baseline = renderer.image_stats().live;
    if renderer.upload_rgba8(0, 0, &[]).is_ok()
        || renderer.upload_rgba8(3, 2, &[0; 23]).is_ok()
        || renderer.image_stats().live != baseline
    {
        return Err("invalid texture data was accepted or allocated".into());
    }
    let data = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 128, 255, 255, 0, 255, 255, 0, 255, 0, 0, 255,
        255, 255,
    ];
    let id = renderer.upload_rgba8(3, 2, &data)?;
    let bounds = Rect::from_xywh(0.0, 0.0, 60.0, 40.0);
    let white = Quad {
        bounds,
        color: [1.0; 4],
    };
    let list = DisplayList::build(|p| {
        p.quad(white)?;
        p.image(Image::new(id, bounds))
    })?;
    let image = renderer.capture_list(&list, ScaleFactor::default(), 67, 63)?;
    for (x, y, color) in [
        (10, 10, [255, 0, 0, 255]),
        (30, 10, [0, 255, 0, 255]),
        (50, 10, [187, 187, 255, 255]),
        (10, 30, [255, 255, 0, 255]),
        (30, 30, [255; 4]),
        (50, 30, [0, 255, 255, 255]),
    ] {
        expect_pixel(&image, x, y, color)?;
    }
    if renderer.last_frame_stats().draw_calls != 2 {
        return Err("quad/image batching was reordered".into());
    }
    let scaled = renderer.capture_list(&list, ScaleFactor::new(1.5)?, 97, 63)?;
    expect_pixel(&scaled, 45, 15, [0, 255, 0, 255])?;
    expect_pixel(&scaled, 75, 45, [0, 255, 255, 255])?;
    let tinted = DisplayList::build(|p| {
        p.quad(white)?;
        p.image(Image {
            tint: [0.5, 1.0, 1.0, 0.5],
            ..Image::new(id, bounds)
        })
    })?;
    let image = renderer.capture_list(&tinted, ScaleFactor::default(), 67, 63)?;
    expect_pixel(&image, 10, 10, [225, 188, 188, 255])?;
    expect_pixel(&image, 50, 10, [225, 225, 255, 255])?;
    let clipped = DisplayList::build(|p| {
        p.quad(white)?;
        p.with_clip(Rect::from_xywh(20.0, 0.0, 20.0, 40.0), |p| {
            p.image(Image::new(id, bounds))
        })
    })?;
    let image = renderer.capture_list(&clipped, ScaleFactor::default(), 67, 63)?;
    for x in [21, 39] {
        expect_pixel(&image, x, 10, [0, 255, 0, 255])?;
    }
    for x in [19, 40] {
        expect_pixel(&image, x, 10, [255; 4])?;
    }
    let cropped = DisplayList::build(|p| {
        p.quad(white)?;
        p.image(Image {
            uv: Rect::<Uv>::from_xywh(2.0 / 3.0, 0.0, 1.0 / 3.0, 1.0),
            ..Image::new(id, bounds)
        })
    })?;
    let image = renderer.capture_list(&cropped, ScaleFactor::default(), 67, 63)?;
    expect_pixel(&image, 10, 10, [187, 187, 255, 255])?;
    expect_pixel(&image, 10, 30, [0, 255, 255, 255])?;
    let mixed = DisplayList::build(|p| {
        p.quad(white)?;
        p.image(Image::new(id, bounds))?;
        p.quad(Quad {
            bounds,
            color: [1.0, 0.0, 0.0, 0.5],
        })
    })?;
    let image = renderer.capture_list(&mixed, ScaleFactor::default(), 67, 63)?;
    expect_pixel(&image, 30, 10, [188, 188, 0, 255])?;
    if renderer.last_frame_stats().draw_calls != 3 {
        return Err("interleaved painter order was merged across an image".into());
    }
    let adjacent = DisplayList::build(|p| {
        p.image(Image::new(id, bounds))?;
        p.image(Image::new(id, bounds))
    })?;
    renderer.capture_list(&adjacent, ScaleFactor::default(), 67, 63)?;
    if renderer.last_frame_stats().draw_calls != 1 {
        return Err("adjacent image material was not batched".into());
    }

    if !renderer.remove_image(id)?
        || renderer.remove_image(id)?
        || renderer.image_stats().retired == 0
    {
        return Err("image retirement did not invalidate its handle".into());
    }
    let before = renderer.last_frame_stats();
    if renderer
        .capture_list(&list, ScaleFactor::default(), 67, 63)
        .is_ok()
        || renderer.last_frame_stats() != before
    {
        return Err("stale image handle reached GPU submission".into());
    }
    let replacement = renderer.upload_rgba8(1, 1, &[255, 0, 255, 255])?;
    if replacement == id
        || renderer
            .capture_list(&list, ScaleFactor::default(), 67, 63)
            .is_ok()
    {
        return Err("reused image slot aliased a stale generation".into());
    }
    let fresh = DisplayList::build(|p| p.image(Image::new(replacement, bounds)))?;
    let image = renderer.capture_list(&fresh, ScaleFactor::default(), 67, 63)?;
    expect_pixel(&image, 10, 10, [255, 0, 255, 255])?;
    let mut foreign = ResourceArena::<ImageResource, _>::default();
    let foreign_id = foreign.insert(());
    let other = DisplayList::build(|p| p.image(Image::new(foreign_id, bounds)))?;
    if renderer
        .capture_list(&other, ScaleFactor::default(), 67, 63)
        .is_ok()
    {
        return Err("foreign arena image was accepted".into());
    }
    renderer.remove_image(replacement)?;
    // Exercise retirement of uploads that have not been waited for explicitly.
    for _ in 0..16 {
        let id = renderer.upload_rgba8(3, 2, &data)?;
        renderer.remove_image(id)?;
    }
    renderer.wait_idle()?;
    if renderer.image_stats().live != baseline || renderer.image_stats().retired != 0 {
        return Err("image retirement failed to drain after GPU idle".into());
    }
    println!(
        "GPU image checks passed: padded RGBA upload, UV/orientation, clipping, sRGB alpha, ordered materials, stale/foreign handles and fence retirement"
    );
    Ok(())
}
