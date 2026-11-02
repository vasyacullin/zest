use super::*;
use winit::keyboard::{Key, NamedKey};
use zest::semantics::SemanticQuery;

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let initial = demo
        .snapshot
        .as_ref()
        .unwrap()
        .semantic_view(&demo.scene.semantic_snapshot());
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
    let clipped = demo
        .snapshot
        .as_ref()
        .unwrap()
        .semantic_view(&demo.scene.semantic_snapshot());
    let target = clipped
        .semantics()
        .find_target(SemanticQuery::new().test_id("layout.footer"))?;
    let plan = clipped.reveal_target(&target).unwrap();
    if clipped.node(target.id()).unwrap().is_visible()
        || !plan.can_apply(&demo.scene, demo.snapshot.as_ref().unwrap())
        || plan.adjustments().len() != 1
        || plan.adjustments()[0].owner().id() != &demo.root
    {
        return Err("semantic reveal did not bind the hidden target and its scroll owner".into());
    }
    demo.scene.push_modal(target.id());
    if demo.semantic_reveal_and_activate(&target)?
        || demo.scroll_offset != Point::new(0.0, 0.0)
        || demo.selected.is_some()
    {
        return Err("semantic reveal edited a scroll owner outside the modal input scope".into());
    }
    demo.scene.pop_modal();
    demo.scene.push_modal(&demo.root);
    if !demo.semantic_reveal_and_activate(&target)?
        || demo.selected.as_ref() != Some(target.id())
        || demo.scene.focused() != Some(target.id())
        || demo.scroll_offset != plan.adjustments()[0].offset()
        || plan.can_apply(&demo.scene, demo.snapshot.as_ref().unwrap())
        || clipped.node(target.id()).unwrap().is_visible()
    {
        return Err("semantic reveal failed to publish fresh geometry before activation".into());
    }
    let visible = demo
        .snapshot
        .as_ref()
        .unwrap()
        .semantic_view(&demo.scene.semantic_snapshot());
    let bounds = visible.node(target.id()).unwrap().visible_bounds().unwrap();
    let point = Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0);
    if visible
        .hit_test(point, |node| {
            node.state.input_allowed && node.id == *target.id()
        })
        .map(|node| &node.semantic.id)
        != Some(target.id())
    {
        return Err("revealed semantic target missed its rendered hit bounds".into());
    }
    let list = demo.cached_paint()?;
    let image = renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    let offset = demo.scroll_offset;
    demo.semantic_reveal_and_activate(&target)?;
    if demo.selected.is_some() || demo.scroll_offset != offset {
        return Err(
            "already visible semantic activation changed scroll or missed its default".into(),
        );
    }
    demo.scene.pop_modal();
    demo.key(&f4, true, false);
    demo.focus_window(false);
    demo.focus_window(true);
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    let list = demo.cached_paint()?;
    renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?;
    demo.finish_frame(RenderStatus::Presented, true);
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    let restored = demo
        .snapshot
        .as_ref()
        .unwrap()
        .semantic_view(&demo.scene.semantic_snapshot());
    if !restored.layout().changes_since(initial.layout()).is_empty()
        || !restored
            .semantics()
            .changes_since(initial.semantics())
            .is_empty()
        || renderer.upload_stats().staged_bytes != staged
        || demo.scene.stats().archetypes != archetypes
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            != images
    {
        return Err("semantic reveal changed retained text/images or restored observations".into());
    }
    println!(
        "GPU semantic reveal checks passed: bound nearest-edge scroll, modal owner limits, fresh geometry before activation, immutable old views, selected pixels and retained text/images"
    );
    Ok(())
}
