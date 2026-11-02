use super::*;

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = demo.snapshot.as_ref().unwrap().clone();
    let old = demo.scene.semantic_snapshot();
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
    let list = demo.cached_paint()?;
    let before = renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?;
    demo.scene.take_paint_invalidation();
    metadata_checks::without_footer(demo)?;
    let removed = demo.scene.semantic_snapshot();
    let changes = removed.changes_since(&old);
    if changes.removed != std::slice::from_ref(&demo.footer)
        || changes.changed != std::slice::from_ref(&demo.root)
        || !changes.order_changed
        || changes.roots_changed
        || changes.modal_changed
    {
        return Err("semantic removal diff missed the changed parent child list".into());
    }
    metadata_checks::redeclare(demo, "Layout status", "layout.footer")?;
    let current = demo.scene.semantic_snapshot();
    let changes = current.changes_since(&old);
    if old.nodes() != current.nodes()
        || changes.replaced != std::slice::from_ref(&demo.footer)
        || !changes.added.is_empty()
        || !changes.removed.is_empty()
        || !changes.changed.is_empty()
        || changes.order_changed
        || changes.roots_changed
        || changes.modal_changed
        || !demo.scene.paint_invalidation().structure_changed
    {
        return Err(
            "semantic diff did not distinguish a recreated instance with identical data".into(),
        );
    }
    // No intermediate layout was published, so identical current geometry and
    // paint may remain cached while semantic consumers retire the old instance.
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    let current_list = demo.cached_paint()?;
    let after = renderer.capture_list(&current_list, ScaleFactor::new(1.25)?, 300, 563)?;
    if !Arc::ptr_eq(&list, &current_list) || before.rgba8 != after.rgba8 {
        return Err(
            "semantic instance replacement changed identical paint commands or pixels".into(),
        );
    }
    demo.finish_frame(RenderStatus::Presented, true);
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if demo.scene.needs_paint()
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
            "semantic diff comparison changed layout, shaping, images or archetypes".into(),
        );
    }
    println!(
        "GPU semantic diff checks passed: removed child/parent deltas, recreated instance with identical data, independent structural paint work and shared layout/paint/pixels/images"
    );
    Ok(())
}
