use super::*;
use winit::keyboard::{Key, NamedKey};
use zest::semantics::{FindError, SemanticQuery, SemanticRole};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = demo.snapshot.as_ref().unwrap().clone();
    let before_paint = demo.scene.paint_invalidation();
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
    let initial = demo.scene.semantic_snapshot();
    let buttons = SemanticQuery::new().role(SemanticRole::Button);
    let footer = initial
        .find(
            buttons
                .test_id("layout.footer")
                .named("Layout status")
                .within(&demo.root)
                .input_allowed(true),
        )?
        .id
        .clone();
    if footer != demo.footer
        || initial.nodes().len() != layout.boxes().len()
        || !matches!(initial.find(buttons), Err(FindError::Ambiguous(ids)) if ids.len() == 3)
        || demo.scene.paint_invalidation() != before_paint
    {
        return Err(
            "semantic query changed paint state, lost nodes or silently resolved ambiguity".into(),
        );
    }
    let bounds = layout.visible_bounds(&footer).unwrap();
    let point = Point::new(bounds.origin.x + 2.0, bounds.origin.y + 2.0);
    demo.pointer_moved(point);
    demo.pointer_press();
    let pressed = demo.scene.semantic_snapshot();
    let state = pressed.node(&footer).unwrap().state;
    if !state.pressed
        || !state.focused
        || !state.hovered
        || !state.input_allowed
        || initial.node(&footer).unwrap().state.pressed
    {
        return Err(
            "semantic press/focus/hover observation disagreed with the input controller".into(),
        );
    }
    let list = demo.cached_paint()?;
    let image = renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [63, 149, 203, 255],
    )?;
    demo.pointer_release();
    if demo
        .scene
        .semantic_snapshot()
        .node(&footer)
        .unwrap()
        .state
        .pressed
        || demo.selected.as_ref() != Some(&footer)
    {
        return Err(
            "semantic release retained pressed state or changed the original activation".into(),
        );
    }
    let body = demo.labels[1].id.clone();
    demo.key(&Key::Named(NamedKey::F2), true, false);
    let disabled = demo.scene.semantic_snapshot();
    if disabled.find(buttons.enabled(false))?.id != body
        || disabled.node(&body).unwrap().state.input_allowed
    {
        return Err("semantic availability filter disagreed with disabled body input".into());
    }
    demo.key(&Key::Named(NamedKey::F2), true, false);
    demo.key(&Key::Named(NamedKey::F3), true, false);
    let modal = demo.scene.semantic_snapshot();
    if modal.modal_root() != Some(&body)
        || modal.find(buttons.input_allowed(true))?.id != body
        || modal.find_all(buttons).len() != 3
        || !modal.node(&footer).unwrap().state.enabled
        || modal.node(&footer).unwrap().state.input_allowed
    {
        return Err(
            "semantic modal observation removed blocked nodes or confused availability with scope"
                .into(),
        );
    }
    let body_bounds = layout.visible_bounds(&body).unwrap();
    if demo.hit(Point::new(
        body_bounds.origin.x + 2.0,
        body_bounds.origin.y + 2.0,
    )) != Some(body)
        || demo.hit(point).is_some()
    {
        return Err("semantic input eligibility and current layout hit testing disagreed".into());
    }
    let list = demo.cached_paint()?;
    renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?;
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    demo.key(&Key::Named(NamedKey::Enter), true, false);
    demo.key(&Key::Named(NamedKey::Enter), false, false);
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
    if initial.nodes() != demo.scene.semantic_snapshot().nodes()
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
            "semantic observation changed geometry, paragraphs, GPU images or restored scene state"
                .into(),
        );
    }
    println!(
        "GPU semantic checks passed: role/name/scope queries, ambiguity, shared layout IDs, press/focus pixels, disabled/modal eligibility, immutable snapshots and retained text/images"
    );
    Ok(())
}
