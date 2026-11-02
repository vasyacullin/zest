use super::*;
use winit::keyboard::{Key, NamedKey};
use zest::semantics::SemanticQuery;

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = demo.snapshot.as_ref().unwrap().clone();
    let snapshot = demo.scene.semantic_snapshot();
    let title = snapshot.find_target(SemanticQuery::new().test_id("layout.title"))?;
    let body = snapshot.find_target(SemanticQuery::new().test_id("layout.body"))?;
    let stale = snapshot.find_target(SemanticQuery::new().test_id("layout.footer"))?;
    let title_bounds = layout.visible_bounds(title.id()).unwrap();
    let footer_bounds = layout.visible_bounds(stale.id()).unwrap();
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
    demo.pointer_moved(Point::new(
        title_bounds.origin.x + 2.0,
        title_bounds.origin.y + 2.0,
    ));
    demo.pointer_press();
    metadata_checks::without_footer(demo)?;
    metadata_checks::redeclare(demo, "Layout status", "layout.footer")?;
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    if demo.semantic_activate(&stale) || !demo.scene.state(title.id()).unwrap().pressed() {
        return Err("stale semantic activation disturbed a live pointer gesture".into());
    }
    let footer = demo
        .scene
        .semantic_snapshot()
        .find_target(SemanticQuery::new().test_id("layout.footer"))?;
    demo.activation_handlers.set(
        &demo.scene,
        footer.id(),
        EventPhase::Target,
        |_, source, _| {
            assert_eq!(*source, ActivationSource::Semantic);
            EventControl::Continue
        },
    );
    if !demo.semantic_activate(&footer)
        || demo.selected.as_ref() != Some(footer.id())
        || demo.scene.state(title.id()).unwrap().pressed()
        || demo.scene.state(footer.id()).unwrap().pressed()
    {
        return Err(
            "logical semantic activation lost its default or synthesized press state".into(),
        );
    }
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        ((footer_bounds.origin.x + 2.0) * 1.25) as u32,
        ((footer_bounds.origin.y + 2.0) * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    demo.pointer_release();
    if demo.selected.as_ref() != Some(footer.id()) {
        return Err("cancelled pointer release activated again after semantic action".into());
    }
    demo.semantic_activate(&footer);
    demo.key(&Key::Named(NamedKey::F6), true, false);
    demo.semantic_activate(&title);
    if demo.selected.is_some() {
        return Err("parent interception failed for semantic activation".into());
    }
    demo.key(&Key::Named(NamedKey::F6), true, false);
    demo.key(&Key::Named(NamedKey::F7), true, false);
    demo.semantic_activate(&title);
    if demo.selected.as_ref() != Some(title.id()) || demo.scene.focused() != Some(footer.id()) {
        return Err("semantic activation bypassed the deferred focus/default order".into());
    }
    capture(demo, renderer)?;
    demo.key(&Key::Named(NamedKey::F3), true, false);
    demo.semantic_activate(&body);
    if demo.scene.focused() != Some(body.id())
        || demo.semantic_activate(&footer)
        || demo.selected.as_ref() != Some(body.id())
    {
        return Err("semantic activation escaped its modal event/input boundary".into());
    }
    capture(demo, renderer)?;
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    demo.key(&Key::Named(NamedKey::F7), true, false);
    demo.semantic_activate(&body);
    demo.activation_handlers
        .remove(footer.id(), EventPhase::Target);
    demo.focus_window(false);
    demo.focus_window(true);
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    capture(demo, renderer)?;
    demo.finish_frame(RenderStatus::Presented, true);
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if demo.selected.is_some()
        || demo.scene.needs_paint()
        || stale.is_live(&demo.scene)
        || !layout.same_snapshot(demo.snapshot.as_ref().unwrap())
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
            "semantic activation changed retained layout/text/images or revived a stale target"
                .into(),
        );
    }
    println!(
        "GPU semantic target checks passed: stale instance rejection, logical source/default pixels, pending gesture cancellation, shared interception/modal/deferred focus and retained layout/text/images"
    );
    Ok(())
}
fn capture(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    let list = demo.cached_paint()?;
    if *list != demo.paint()? {
        return Err("semantic action reused stale paint commands".into());
    }
    Ok(renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?)
}
