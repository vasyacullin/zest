use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};

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
    let footer = snapshot.bounds(&demo.footer).unwrap();
    let point = Point::new(
        footer.origin.x + 10.0,
        footer.origin.y + footer.size.height * 0.5,
    );
    demo.focus_window(true);
    if !demo.pointer_moved(point) || demo.hovered.as_ref() != Some(&demo.footer) {
        return Err("layout hover did not use the current snapshot".into());
    }
    let capture = capture(demo, renderer)?;
    expect_pixel(
        &capture,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [170, 231, 255, 255],
    )?;
    if !demo.pointer_press() || demo.scene.focused() != Some(&demo.footer) {
        return Err("layout click did not focus the hit node".into());
    }
    let image = self::capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [124, 203, 255, 255],
    )?;
    let tab = Key::Named(NamedKey::Tab);
    if !demo.key(&tab, true, false) || demo.scene.focused() != Some(&demo.labels[0].id) {
        return Err("layout Tab did not wrap in declaration order".into());
    }
    let title = snapshot.bounds(&demo.labels[0].id).unwrap();
    let image = self::capture(demo, renderer)?;
    expect_pixel(
        &image,
        ((title.origin.x + title.size.width - 2.0) * 1.25) as u32,
        ((title.origin.y + title.size.height * 0.5) * 1.25) as u32,
        [80, 118, 149, 255],
    )?;
    demo.modifiers(ModifiersState::SHIFT);
    if !demo.key(&tab, true, false) || demo.scene.focused() != Some(&demo.footer) {
        return Err("layout Shift+Tab did not retain keyed focus order".into());
    }
    demo.focus_window(false);
    if demo.hovered.is_some()
        || demo.scene.focused().is_some()
        || demo.key(&tab, true, false)
        || demo.pointer_press()
    {
        return Err("unfocused layout retained input state or accepted input".into());
    }
    let image = self::capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [255; 4],
    )?;
    demo.focus_window(true);
    demo.prepare(renderer, snapshot.viewport(), 1.25)?;
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if !snapshot.same_snapshot(demo.snapshot.as_ref().unwrap())
        || renderer.upload_stats().staged_bytes != staged
        || images
            != demo
                .labels
                .iter()
                .map(|label| label.prepared.as_ref().unwrap().image)
                .collect::<Vec<_>>()
        || demo.scene.stats().archetypes != archetypes
    {
        return Err("hover/focus changed layout, text images or ECS archetypes".into());
    }
    println!(
        "GPU layout input checks passed: hover/click/Tab/Shift+Tab/focus-loss pixels, shared snapshot/paragraph/images and stable ECS archetypes"
    );
    Ok(())
}
fn capture(
    demo: &LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    Ok(renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?)
}
