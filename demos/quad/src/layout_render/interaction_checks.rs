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
        [63, 149, 203, 255],
    )?;
    if !demo.pointer_release()
        || demo.selected.as_ref() != Some(&demo.footer)
        || demo.pointer_release()
    {
        return Err("layout pointer release did not toggle selection exactly once".into());
    }
    let image = self::capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    for named in [NamedKey::Enter, NamedKey::Space, NamedKey::Space] {
        let key = Key::Named(named);
        if !demo.key(&key, true, false)
            || demo.key_event(&key, true, true, false)
            || demo.key(&key, false, true)
            || !demo.key(&key, false, false)
        {
            return Err("layout keyboard activation accepted repeats/synthetic release or lost the real pair".into());
        }
    }
    if demo.selected.is_some() {
        return Err("layout Enter/Space did not toggle pointer selection".into());
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
    let enter = Key::Named(NamedKey::Enter);
    demo.key(&enter, true, false);
    let image = self::capture(demo, renderer)?;
    expect_pixel(
        &image,
        ((title.origin.x + title.size.width - 2.0) * 1.25) as u32,
        ((title.origin.y + title.size.height * 0.5) * 1.25) as u32,
        [108, 170, 211, 255],
    )?;
    demo.key(&enter, false, false);
    let image = self::capture(demo, renderer)?;
    expect_pixel(
        &image,
        ((title.origin.x + title.size.width - 2.0) * 1.25) as u32,
        ((title.origin.y + title.size.height - 1.0) * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    // Toggle off so later focus-loss checks keep the preview's original colors.
    demo.key(&enter, true, false);
    demo.key(&enter, false, false);
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
    availability_checks(demo, renderer, point)?;
    super::modal_checks::check(demo, renderer, point)?;
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
        return Err("hover/focus/activation changed layout, text images or ECS archetypes".into());
    }
    println!(
        "GPU layout input checks passed: hover/pressed/selection/disabled/modal pixels, Enter/Space/F2/F3/Tab/Esc, focus trapping/restoration/loss, repeat filtering, shared snapshot/paragraph/images and stable ECS archetypes"
    );
    Ok(())
}
fn availability_checks(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
    point: Point<Logical>,
) -> Result<(), Box<dyn std::error::Error>> {
    let footer = demo.footer.clone();
    let title = demo.labels[0].id.clone();
    let tab = Key::Named(NamedKey::Tab);
    let f2 = Key::Named(NamedKey::F2);
    demo.pointer_moved(point);
    demo.pointer_press();
    demo.set_enabled(&footer, false);
    if demo.pointer_release()
        || demo.scene.focused().is_some()
        || demo.hovered.is_some()
        || demo.selected.is_some()
        || demo.activation.captured().is_some()
    {
        return Err("disabling a layout node retained or completed its gesture".into());
    }
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [97, 97, 97, 255],
    )?;
    demo.pointer_press();
    if demo.pointer_release() || demo.scene.focused().is_some() {
        return Err("disabled layout node accepted another click".into());
    }
    demo.key(&tab, true, false);
    demo.key(&tab, true, false);
    if demo.scene.focused() != Some(&demo.labels[1].id)
        || !demo.key(&f2, true, false)
        || demo.scene.focused().is_some()
    {
        return Err("F2 did not disable and unfocus the body label".into());
    }
    demo.key(&tab, true, false);
    if demo.scene.focused() != Some(&title) || demo.key(&tab, true, false) {
        return Err("focus traversal did not skip disabled layout nodes".into());
    }
    demo.key(&f2, true, false);
    demo.set_enabled(&footer, true);
    demo.focus_window(false);
    demo.focus_window(true);
    demo.set_enabled(&title, false);
    let image = capture(demo, renderer)?;
    let (x, y) = opaque_title_pixel(demo);
    expect_pixel(&image, x, y, [116, 144, 160, 255])?;
    demo.set_enabled(&title, true);
    let image = capture(demo, renderer)?;
    expect_pixel(&image, x, y, [188, 231, 255, 255])?;
    Ok(())
}
fn capture(
    demo: &LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    Ok(renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?)
}
