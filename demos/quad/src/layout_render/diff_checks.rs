use super::*;
use winit::keyboard::{Key, NamedKey};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let original = demo.snapshot.as_ref().unwrap().clone();
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
    let f4 = Key::Named(NamedKey::F4);
    demo.key(&f4, true, false);
    let clipped = demo.snapshot.as_ref().unwrap().clone();
    let clipped_changes = clipped.changes_since(&original);
    if clipped_changes.is_empty()
        || !clipped_changes.changed.contains(&demo.root)
        || clipped_changes.order_changed
        || clipped_changes.viewport_changed
    {
        return Err(
            "clip-only snapshot diff missed its owner or changed hierarchy/viewport".into(),
        );
    }
    demo.scene.take_paint_invalidation();
    demo.scroll_to(Point::new(0.0, 30.0), false)?;
    let scrolled = demo.snapshot.as_ref().unwrap();
    let changes = scrolled.changes_since(&clipped);
    let work = demo.scene.paint_invalidation();
    if changes.changed.len() != scrolled.boxes().len()
        || changes.changed.iter().any(|id| !work.widgets.contains(id))
        || work.structure_changed
        || changes.order_changed
        || changes.viewport_changed
    {
        return Err("scroll diff did not invalidate projected nodes and owner metrics".into());
    }
    let image = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?;
    let root = scrolled.bounds(&demo.root).unwrap();
    expect_pixel(
        &image,
        100,
        ((root.origin.y + root.size.height + 2.0) * 1.25) as u32,
        [0, 0, 0, 255],
    )?;
    if scrolled
        .boxes()
        .iter()
        .zip(clipped.boxes())
        .any(|(new, old)| new.layout_bounds != old.layout_bounds)
        || clipped.scroll(&demo.root).unwrap().offset.y != 0.0
    {
        return Err("scroll diff mutated unscrolled layout or its previous snapshot".into());
    }
    demo.finish_frame(RenderStatus::Presented, true);
    demo.key(&f4, true, false);
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?;
    demo.finish_frame(RenderStatus::Presented, true);
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if !demo
        .snapshot
        .as_ref()
        .unwrap()
        .changes_since(&original)
        .is_empty()
        || renderer.upload_stats().staged_bytes != staged
        || demo.scene.needs_paint()
        || demo.scene.stats().archetypes != archetypes
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            != images
    {
        return Err(
            "restored layout diff changed geometry, paragraphs, images or paint state".into(),
        );
    }
    println!(
        "GPU layout diff checks passed: clip/scroll changes, precise geometry invalidation, immutable snapshots, restored equality and retained text/images/archetypes"
    );
    Ok(())
}
