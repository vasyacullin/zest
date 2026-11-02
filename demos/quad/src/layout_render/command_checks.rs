use super::*;
use winit::keyboard::{Key, NamedKey};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let staged = renderer.upload_stats().staged_bytes;
    let images: Vec<_> = demo
        .labels
        .iter()
        .map(|label| label.prepared.as_ref().unwrap().image)
        .collect();
    let guard = demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .layout_selection(zest::text::Selection::caret(0))?;
    let archetypes = demo.scene.stats().archetypes;
    let title = demo.labels[0].id.clone();
    let body = demo.labels[1].id.clone();
    let title_bounds = snapshot.bounds(&title).unwrap();
    let body_bounds = snapshot.bounds(&body).unwrap();
    let footer = snapshot.bounds(&demo.footer).unwrap();
    let root = snapshot.bounds(&demo.root).unwrap();
    demo.key(&Key::Named(NamedKey::F7), true, false);
    click(demo, title_bounds);
    if demo.scene.focused() != Some(&demo.footer) || demo.selected.as_ref() != Some(&title) {
        return Err(
            "deferred focus changed the activation target or failed after default action".into(),
        );
    }
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        12,
        ((root.origin.y + root.size.height - 1.0) * 1.25) as u32,
        [89, 231, 255, 255],
    )?;
    expect_pixel(
        &image,
        ((footer.origin.x + 2.0) * 1.25) as u32,
        ((footer.origin.y + 2.0) * 1.25) as u32,
        [124, 203, 255, 255],
    )?;
    demo.key(&Key::Named(NamedKey::F3), true, false);
    activate(demo);
    if demo.scene.focused() != Some(&body) || demo.selected.as_ref() != Some(&body) {
        return Err("modal event boundary leaked a deferred outer focus command".into());
    }
    capture(demo, renderer)?;
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    demo.key(&Key::Named(NamedKey::F6), true, false);
    click(demo, title_bounds);
    if demo.scene.focused() != Some(&title) || demo.selected.as_ref() != Some(&body) {
        return Err("consumed capture still queued a bubble/default focus action".into());
    }
    capture(demo, renderer)?;
    demo.key(&Key::Named(NamedKey::F6), true, false);
    demo.key(&Key::Named(NamedKey::F7), true, false);
    click(demo, body_bounds);
    demo.focus_window(false);
    demo.focus_window(true);
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        12,
        ((root.origin.y + root.size.height - 1.0) * 1.25) as u32,
        [0, 0, 0, 255],
    )?;
    demo.finish_frame(RenderStatus::Presented, true);
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if demo.selected.is_some()
        || demo.scene.needs_paint()
        || !snapshot.same_snapshot(demo.snapshot.as_ref().unwrap())
        || renderer.upload_stats().staged_bytes != staged
        || demo.scene.stats().archetypes != archetypes
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            != images
    {
        return Err(
            "deferred commands invalidated layout, shapes, image uploads or archetypes".into(),
        );
    }
    println!(
        "GPU deferred command checks passed: focus after original activation/default, prevention/modal boundaries, cached status/focus pixels and retained geometry/text/images"
    );
    Ok(())
}
fn click(demo: &mut LayoutDemo, bounds: Rect<Logical>) {
    demo.pointer_moved(Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0));
    demo.pointer_press();
    demo.pointer_release();
}
fn activate(demo: &mut LayoutDemo) {
    demo.key(&Key::Named(NamedKey::Enter), true, false);
    demo.key(&Key::Named(NamedKey::Enter), false, false);
}
fn capture(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    let cached = demo.cached_paint()?;
    if *cached != demo.paint()? {
        return Err("deferred effects reused stale cached paint".into());
    }
    Ok(renderer.capture_list(&cached, ScaleFactor::new(1.25)?, 300, 563)?)
}
