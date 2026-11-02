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
    let footer = demo.footer.clone();
    demo.set_focus(Some(footer.clone()));
    demo.scene.take_paint_invalidation();
    demo.key(&Key::Named(NamedKey::F6), true, false);
    let work = demo.scene.paint_invalidation();
    if work.widgets != [demo.root.clone()] || work.structure_changed {
        return Err("application-owned decoration did not invalidate only its node".into());
    }
    for status in [
        RenderStatus::Suspended,
        RenderStatus::Skipped,
        RenderStatus::NeedsResize,
    ] {
        demo.finish_frame(status, true);
    }
    demo.finish_frame(RenderStatus::Presented, false);
    if demo.scene.paint_invalidation() != work {
        return Err("unfinished/clear-only frame consumed scene paint work".into());
    }
    let list = demo.paint()?;
    let image = renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?;
    expect_pixel(&image, 12, 1, [255, 170, 63, 255])?;
    // Successful offscreen rendering of the complete scene has the same CPU
    // completion contract as a presented frame; live presentation is checked by App.
    demo.finish_frame(RenderStatus::Presented, true);
    if demo.scene.needs_paint() || demo.scene.focused() != Some(&footer) {
        return Err("completed paint retained work or changed interaction state".into());
    }
    demo.key(&Key::Named(NamedKey::F6), true, false);
    demo.key(&Key::Named(NamedKey::Enter), true, false);
    demo.key(&Key::Named(NamedKey::Enter), false, false);
    let work = demo.scene.paint_invalidation();
    if work.widgets != [demo.root.clone(), footer.clone()] || work.structure_changed {
        return Err("new decoration/selection changes were lost after draining".into());
    }
    let bounds = snapshot.bounds(&footer).unwrap();
    let image = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?;
    expect_pixel(&image, 12, 1, [0, 0, 0, 255])?;
    expect_pixel(
        &image,
        ((bounds.origin.x + 10.0) * 1.25) as u32,
        ((bounds.origin.y + 2.0) * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    demo.finish_frame(RenderStatus::Presented, true);
    demo.key(&Key::Named(NamedKey::Enter), true, false);
    demo.key(&Key::Named(NamedKey::Enter), false, false);
    demo.focus_window(false);
    demo.focus_window(true);
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?;
    demo.finish_frame(RenderStatus::Presented, true);
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if demo.scene.needs_paint()
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
        return Err("paint draining changed layout, paragraphs, uploads or archetypes".into());
    }
    println!(
        "GPU paint invalidation checks passed: explicit model changes, retry/clear-only retention, completion drain, later changes, matching pixels and retained geometry/text/images"
    );
    Ok(())
}
