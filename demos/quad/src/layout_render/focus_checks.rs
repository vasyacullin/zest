use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let before = demo.snapshot.as_ref().unwrap().clone();
    let staged = renderer.upload_stats().staged_bytes;
    let images: Vec<_> = demo
        .labels
        .iter()
        .map(|label| label.prepared.as_ref().unwrap().image)
        .collect();
    let guards: Vec<_> = demo
        .labels
        .iter()
        .map(|label| {
            label
                .prepared
                .as_ref()
                .unwrap()
                .paragraph
                .layout_selection(zest::text::Selection::caret(0))
        })
        .collect::<Result<_, _>>()?;
    let title = demo.labels[0].id.clone();
    let body = demo.labels[1].id.clone();
    let footer = demo.footer.clone();
    let f4 = Key::Named(NamedKey::F4);
    let tab = Key::Named(NamedKey::Tab);
    demo.key(&f4, true, false);
    for target in [&title, &body, &footer] {
        if !demo.key(&tab, true, false)
            || demo.scene.focused() != Some(target)
            || !demo.visible(target)
        {
            return Err("keyboard traversal did not reveal a hidden layout target".into());
        }
    }
    let bounds = demo.snapshot.as_ref().unwrap().bounds(&footer).unwrap();
    let point = Point::new(bounds.origin.x + 10.0, bounds.origin.y + 6.0);
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [124, 203, 255, 255],
    )?;
    if demo.hit(point) != Some(footer.clone()) {
        return Err("revealed focus pixels and hit coordinates diverged".into());
    }
    demo.key(&tab, true, false); // Wrap back to the hidden title.
    let bounds = demo.snapshot.as_ref().unwrap().bounds(&title).unwrap();
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        ((bounds.origin.x + bounds.size.width - 2.0) * 1.25) as u32,
        ((bounds.origin.y + bounds.size.height * 0.5) * 1.25) as u32,
        [80, 118, 149, 255],
    )?;
    demo.modifiers(ModifiersState::SHIFT);
    demo.key(&tab, true, false);
    if demo.scene.focused() != Some(&footer) || !demo.visible(&footer) {
        return Err("Shift+Tab did not reveal the reverse-wrap target".into());
    }
    demo.modifiers(ModifiersState::empty());
    demo.key(&Key::Named(NamedKey::F3), true, false);
    let enter = Key::Named(NamedKey::Enter);
    demo.key(&enter, true, false);
    demo.key(&tab, true, false); // Same focused modal ID, different scroll position.
    if demo.scene.focused() != Some(&body)
        || demo.key(&enter, false, false)
        || demo.selected.is_some()
    {
        return Err("modal reveal retained a stale keyboard gesture or escaped its scope".into());
    }
    let stable = demo.snapshot.as_ref().unwrap().clone();
    if demo.key(&tab, true, false) || !stable.same_snapshot(demo.snapshot.as_ref().unwrap()) {
        return Err("repeated reveal of an oversized target oscillated".into());
    }
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    demo.focus_window(false);
    demo.focus_window(true);
    demo.key(&f4, true, false);
    demo.prepare(renderer, before.viewport(), 1.25)?;
    for (label, guard) in demo.labels.iter().zip(&guards) {
        label
            .prepared
            .as_ref()
            .unwrap()
            .paragraph
            .selection_caret(guard)?;
    }
    if staged != renderer.upload_stats().staged_bytes
        || images
            != demo
                .labels
                .iter()
                .map(|label| label.prepared.as_ref().unwrap().image)
                .collect::<Vec<_>>()
    {
        return Err(
            "keyboard reveal invalidated text paragraphs/images or uploaded textures".into(),
        );
    }
    println!(
        "GPU focus reveal checks passed: forward/reverse wrap, scrolled focus/hit pixels, modal trapping, gesture cancellation, stable oversized targets and retained paragraphs/images"
    );
    Ok(())
}
fn capture(
    demo: &LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    Ok(renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?)
}
