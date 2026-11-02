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
    let tint = demo.tint;
    let first = compare(demo, renderer)?;
    demo.scene.invalidate_paint(&demo.footer);
    for status in [
        RenderStatus::Skipped,
        RenderStatus::Suspended,
        RenderStatus::NeedsResize,
    ] {
        demo.finish_frame(status, true);
        if !demo.scene.needs_paint() || !Arc::ptr_eq(&first, &demo.cached_paint()?) {
            return Err("retry lost pending scene work or rebuilt identical paint".into());
        }
    }
    demo.finish_frame(RenderStatus::Presented, true);
    if demo.scene.needs_paint() || !Arc::ptr_eq(&first, &demo.cached_paint()?) {
        return Err("paint acknowledgement changed the cached command inputs".into());
    }
    demo.tint[0] = f32::NAN;
    if demo.cached_paint() != Err(PaintError::InvalidColor) {
        return Err("paint cache hid an invalid new tint behind stale commands".into());
    }
    demo.tint = tint;
    if !Arc::ptr_eq(&first, &demo.cached_paint()?) {
        return Err("failed paint build discarded its previous valid entry".into());
    }
    demo.tint = [1.0, 0.6, 0.2, 1.0];
    compare(demo, renderer)?;
    demo.tint = tint;
    let footer = snapshot.bounds(&demo.footer).unwrap();
    demo.pointer_moved(Point::new(footer.origin.x + 10.0, footer.origin.y + 2.0));
    compare(demo, renderer)?;
    demo.pointer_press();
    compare(demo, renderer)?;
    demo.pointer_release();
    compare(demo, renderer)?;
    demo.key(&Key::Named(NamedKey::Enter), true, false);
    demo.key(&Key::Named(NamedKey::Enter), false, false);
    demo.key(&Key::Named(NamedKey::F6), true, false);
    compare(demo, renderer)?;
    demo.key(&Key::Named(NamedKey::F3), true, false);
    compare(demo, renderer)?;
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    demo.key(&Key::Named(NamedKey::F6), true, false);
    demo.focus_window(false);
    demo.focus_window(true);
    let f4 = Key::Named(NamedKey::F4);
    demo.key(&f4, true, false);
    compare(demo, renderer)?;
    let thumb = demo.scrollbar().unwrap().thumb();
    demo.pointer_moved(Point::new(
        thumb.origin.x + thumb.size.width * 0.5,
        thumb.origin.y + thumb.size.height * 0.5,
    ));
    compare(demo, renderer)?;
    demo.pointer_press();
    compare(demo, renderer)?;
    demo.pointer_moved(Point::new(thumb.origin.x + 4.0, 1000.0));
    compare(demo, renderer)?;
    demo.pointer_release();
    demo.key(&f4, true, false);
    demo.focus_window(false);
    demo.focus_window(true);
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    compare(demo, renderer)?;
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if renderer.upload_stats().staged_bytes != staged
        || demo.scene.stats().archetypes != archetypes
        || !snapshot
            .changes_since(demo.snapshot.as_ref().unwrap())
            .is_empty()
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            != images
    {
        return Err(
            "paint cache interactions changed geometry, shapes, images or archetypes".into(),
        );
    }
    let before = demo.cached_paint()?;
    demo.remove_images(renderer)?;
    demo.reset_gpu();
    let empty_images = compare(demo, renderer)?;
    if Arc::ptr_eq(&before, &empty_images) {
        return Err("GPU image removal reused stale paint".into());
    }
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    let restored = compare(demo, renderer)?;
    if Arc::ptr_eq(&empty_images, &restored)
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            == images
    {
        return Err("restored GPU images did not replace cached image identities".into());
    }
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    demo.finish_frame(RenderStatus::Presented, true);
    println!(
        "GPU paint cache checks passed: shared retry/completed lists, failed build retention, cached/uncached pixel equality across input/clip/scroll states, and fresh image handles after restoration"
    );
    Ok(())
}
fn compare(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Arc<DisplayList>, Box<dyn std::error::Error>> {
    let cached = demo.cached_paint()?;
    let direct = demo.paint()?;
    if *cached != direct {
        return Err("cached and direct paint commands differ".into());
    }
    let scale = ScaleFactor::new(1.25)?;
    let a = renderer.capture_list(&cached, scale, 300, 563)?;
    let b = renderer.capture_list(&direct, scale, 300, 563)?;
    if a.rgba8 != b.rgba8 {
        return Err("cached and direct paint pixels differ".into());
    }
    if !Arc::ptr_eq(&cached, &demo.cached_paint()?) {
        return Err("unchanged paint did not reuse its Arc".into());
    }
    Ok(cached)
}
