use super::*;
use winit::{
    event::MouseScrollDelta,
    keyboard::{Key, NamedKey},
};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let before = demo.snapshot.as_ref().unwrap().clone();
    let staged = renderer.upload_stats().staged_bytes;
    let images: Vec<_> = demo
        .labels
        .iter()
        .map(|label| label.prepared.as_ref().unwrap().image)
        .collect();
    let guards: Vec<_> = demo
        .labels
        .iter()
        .map(|label| {
            label
                .prepared
                .as_ref()
                .unwrap()
                .paragraph
                .layout_selection(zest::text::Selection::caret(0))
        })
        .collect::<Result<_, _>>()?;
    let title = demo.labels[0].id.clone();
    let footer = demo.footer.clone();
    let scale = ScaleFactor::new(1.25)?;
    let f4 = Key::Named(NamedKey::F4);
    demo.key(&f4, true, false);
    demo.pointer_moved(Point::new(34.0, 40.0));
    demo.pointer_press();
    if !demo.wheel(MouseScrollDelta::LineDelta(0.0, -1e6), scale)?
        || demo.pointer_release()
        || demo.scene.state(&title).unwrap().pressed()
        || demo.scene.focused().is_some()
    {
        return Err("scrolling did not cancel a captured/hidden title press".into());
    }
    let frame = demo.snapshot.as_ref().unwrap().clone();
    let metrics = frame.scroll(&demo.root).unwrap();
    if metrics.offset != metrics.max_offset
        || metrics.offset.y <= 0.0
        || frame.visible_bounds(&title).is_some()
    {
        return Err("content scroll range did not clamp at the bottom".into());
    }
    for node in frame.boxes() {
        if node.layout_bounds != before.bounds(&node.id).unwrap() {
            return Err("scrolling changed unscrolled layout bounds".into());
        }
    }
    let bounds = frame.bounds(&footer).unwrap();
    let point = Point::new(
        bounds.origin.x + 10.0,
        bounds.origin.y + bounds.size.height * 0.5,
    );
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [255; 4],
    )?;
    if demo.hit(point) != Some(footer.clone()) {
        return Err("hit testing did not follow the scrolled marker".into());
    }
    demo.pointer_moved(point);
    demo.pointer_press();
    demo.pointer_release();
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    demo.pointer_press();
    demo.pointer_release();
    if demo.wheel(MouseScrollDelta::LineDelta(0.0, -1e6), scale)?
        || !frame.same_snapshot(demo.snapshot.as_ref().unwrap())
    {
        return Err("scrolling past the boundary rebuilt a snapshot".into());
    }
    if !demo.wheel(MouseScrollDelta::LineDelta(0.0, 1e6), scale)? || demo.scroll_offset.y != 0.0 {
        return Err("wheel did not restore the top scroll position".into());
    }
    demo.focus_window(false);
    demo.focus_window(true);
    demo.key(&f4, true, false);
    demo.prepare(renderer, before.viewport(), 1.25)?;
    for (label, guard) in demo.labels.iter().zip(&guards) {
        label
            .prepared
            .as_ref()
            .unwrap()
            .paragraph
            .selection_caret(guard)?;
    }
    if staged != renderer.upload_stats().staged_bytes
        || images
            != demo
                .labels
                .iter()
                .map(|label| label.prepared.as_ref().unwrap().image)
                .collect::<Vec<_>>()
    {
        return Err("scrolling replaced paragraphs/images or uploaded text textures".into());
    }
    println!(
        "GPU layout scroll checks passed: bounded wheel, projected marker/selection pixels, matching hits, hidden focus/capture cancellation, stable layout bounds and retained paragraphs/images"
    );
    Ok(())
}
fn capture(
    demo: &LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    Ok(renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?)
}
