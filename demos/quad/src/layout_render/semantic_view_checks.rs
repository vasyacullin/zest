use super::*;
use winit::keyboard::{Key, NamedKey};
use zest::semantics::{FindError, SemanticQuery, SemanticRole};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let initial = demo
        .snapshot
        .as_ref()
        .unwrap()
        .semantic_view(&demo.scene.semantic_snapshot());
    let query = SemanticQuery::new()
        .test_id("layout.footer")
        .input_allowed(true);
    let footer = initial.find_visible(query)?;
    let bounds = footer.visible_bounds().unwrap();
    let point = Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0);
    let target = initial.find_visible_target(query)?;
    if initial
        .hit_test(point, |node| {
            node.role == SemanticRole::Button && node.state.input_allowed
        })
        .map(|node| &node.semantic.id)
        != Some(target.id())
        || demo.hit(point).as_ref() != Some(target.id())
    {
        return Err("joined semantic hit testing diverged from current preview input".into());
    }
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
    demo.semantic_activate(&target);
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    if initial.node(target.id()).unwrap().semantic.state.focused {
        return Err("semantic view mutated its old state after activation".into());
    }
    demo.semantic_activate(&target);
    let f4 = Key::Named(NamedKey::F4);
    demo.key(&f4, true, false);
    let clipped = demo
        .snapshot
        .as_ref()
        .unwrap()
        .semantic_view(&demo.scene.semantic_snapshot());
    if clipped.find(query)?.geometry.is_none()
        || clipped.find(query)?.is_visible()
        || clipped.find_visible(query).unwrap_err() != FindError::NotFound
        || clipped.find_visible_target(query).is_ok()
        || clipped
            .hit_test(point, |node| {
                node.role == SemanticRole::Button && node.state.input_allowed
            })
            .is_some()
        || !initial.find_visible(query)?.is_visible()
        || demo.semantic_activate(&target)
    {
        return Err(
            "joined semantic visibility accepted a clipped target or mutated old geometry".into(),
        );
    }
    let max_offset = demo
        .snapshot
        .as_ref()
        .unwrap()
        .scroll(&demo.root)
        .unwrap()
        .max_offset
        .y;
    demo.scroll_to(Point::new(0.0, max_offset), false)?;
    let scrolled = demo
        .snapshot
        .as_ref()
        .unwrap()
        .semantic_view(&demo.scene.semantic_snapshot());
    let revealed = scrolled.find_visible_target(query)?;
    let bounds = scrolled.find_visible(query)?.visible_bounds().unwrap();
    let point = Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0);
    if revealed != target
        || scrolled
            .hit_test(point, |node| {
                node.role == SemanticRole::Button && node.state.input_allowed
            })
            .map(|node| &node.semantic.id)
            != Some(target.id())
    {
        return Err(
            "scrolled semantic view changed instance identity or missed projected hits".into(),
        );
    }
    demo.semantic_activate(&revealed);
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    demo.semantic_activate(&revealed);
    demo.key(&f4, true, false);
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
        return Err(
            "semantic view projection changed layout/text/images or restored observations".into(),
        );
    }
    println!(
        "GPU semantic view checks passed: visible query/target/hit joins, clip rejection, scroll projection, immutable old observations, selection pixels and retained text/images"
    );
    Ok(())
}
fn capture(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    let list = demo.cached_paint()?;
    Ok(renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?)
}
