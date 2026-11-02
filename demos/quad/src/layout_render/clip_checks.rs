use super::*;
use winit::keyboard::{Key, NamedKey};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let before = demo.snapshot.as_ref().unwrap().clone();
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
    let staged = renderer.upload_stats().staged_bytes;
    let footer = before.bounds(&demo.footer).unwrap();
    let point = Point::new(
        footer.origin.x + 10.0,
        footer.origin.y + footer.size.height * 0.5,
    );
    demo.pointer_moved(point);
    demo.pointer_press();
    let f4 = Key::Named(NamedKey::F4);
    if !demo.key(&f4, true, false)
        || !demo.clipped
        || demo.pointer_release()
        || demo.hovered.is_some()
        || demo.scene.focused().is_some()
    {
        return Err("ancestor clipping did not cancel a hidden footer gesture".into());
    }
    let clipped = demo.snapshot.as_ref().unwrap();
    for node in before.boxes() {
        if clipped.bounds(&node.id) != Some(node.bounds) {
            return Err("clip-only update changed raw layout geometry".into());
        }
    }
    if clipped.visible_bounds(&demo.footer).is_some()
        || demo.hit(point).is_some()
        || demo.hit(Point::new(34.0, CLIP_HEIGHT)).is_some()
        || demo.hit(Point::new(34.0, CLIP_HEIGHT - 0.001)) != Some(demo.labels[1].id.clone())
    {
        return Err("hit testing did not match the fractional ancestor clip".into());
    }
    let image = capture(demo, renderer)?;
    let (x, y) = opaque_title_pixel(demo);
    expect_pixel(&image, x, y, [188, 231, 255, 255])?;
    // Only compare pixel centers inside the viewport. At fractional DPI the
    // padded capture's final row can lie exactly on/outside its maximum edge.
    let first_hidden_row = (CLIP_HEIGHT * 1.25 - 0.5).ceil() as u32;
    let viewport_end = (before.viewport().height * 1.25 - 0.5).ceil().max(0.0) as u32;
    for row in first_hidden_row..viewport_end.min(image.height) {
        let start = (row * image.width * 4) as usize;
        if let Some((x, pixel)) = image.rgba8[start..start + image.width as usize * 4]
            .chunks_exact(4)
            .enumerate()
            .find(|(_, pixel)| *pixel != [0, 0, 0, 255])
        {
            return Err(
                format!("unexpected pixel under ancestor clip at ({x}, {row}): {pixel:?}").into(),
            );
        }
    }
    let tab = Key::Named(NamedKey::Tab);
    for expected in [
        demo.labels[0].id.clone(),
        demo.labels[1].id.clone(),
        demo.labels[0].id.clone(),
    ] {
        demo.key(&tab, true, false);
        if demo.scene.focused() != Some(&expected) {
            return Err("Tab did not skip the ancestor-clipped footer".into());
        }
    }
    let f3 = Key::Named(NamedKey::F3);
    demo.key(&f3, true, false);
    let image = capture(demo, renderer)?;
    let body = demo
        .snapshot
        .as_ref()
        .unwrap()
        .bounds(&demo.labels[1].id)
        .unwrap();
    expect_pixel(
        &image,
        ((body.origin.x + 10.0) * 1.25) as u32,
        ((body.origin.y + 1.0) * 1.25) as u32,
        [255, 203, 89, 255],
    )?;
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    demo.focus_window(false);
    demo.focus_window(true);
    if !demo.key(&f4, true, false) || demo.clipped {
        return Err("F4 did not restore overflow visibility".into());
    }
    demo.prepare(renderer, before.viewport(), 1.25)?;
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [255; 4],
    )?;
    for (label, guard) in demo.labels.iter().zip(&guards) {
        label
            .prepared
            .as_ref()
            .unwrap()
            .paragraph
            .selection_caret(guard)?;
    }
    if renderer.upload_stats().staged_bytes != staged
        || images
            != demo
                .labels
                .iter()
                .map(|label| label.prepared.as_ref().unwrap().image)
                .collect::<Vec<_>>()
    {
        return Err("clip-only updates changed text paragraphs/images or uploaded textures".into());
    }
    println!(
        "GPU ancestor clip checks passed: fractional text/marker pixels, matching hit bounds, hidden-focus/gesture cancellation, modal scope, Tab filtering and paragraph/image reuse"
    );
    Ok(())
}
fn capture(
    demo: &LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    Ok(renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?)
}
