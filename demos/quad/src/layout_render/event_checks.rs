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
    demo.focus_window(true);
    let footer = snapshot.bounds(&demo.footer).unwrap();
    let point = Point::new(
        footer.origin.x + 10.0,
        footer.origin.y + footer.size.height * 0.5,
    );
    demo.pointer_moved(point);
    demo.pointer_press();
    demo.pointer_release();
    if demo.selected.as_ref() != Some(&demo.footer) {
        return Err("unhandled routed activation lost its default action".into());
    }
    expect_pixel(
        &capture(demo, renderer)?,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    demo.pointer_press();
    demo.pointer_release();
    demo.key(&Key::Named(NamedKey::F6), true, false);
    expect_pixel(&capture(demo, renderer)?, 12, 1, [255, 170, 63, 255])?;
    demo.pointer_press();
    demo.pointer_release();
    activate(demo);
    if demo.selected.is_some() || demo.scene.focused() != Some(&demo.footer) {
        return Err("parent event consumption failed to prevent pointer/keyboard defaults".into());
    }
    expect_pixel(
        &capture(demo, renderer)?,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [124, 203, 255, 255],
    )?;
    demo.key(&Key::Named(NamedKey::F3), true, false);
    activate(demo);
    let body = &demo.labels[1].id;
    if demo.scene.modal_root() != Some(body) || demo.selected.as_ref() != Some(body) {
        return Err("modal event route reached a blocked outer interceptor".into());
    }
    let bounds = snapshot.bounds(body).unwrap();
    expect_pixel(
        &capture(demo, renderer)?,
        ((bounds.origin.x + bounds.size.width - 2.0) * 1.25) as u32,
        ((bounds.origin.y + bounds.size.height - 1.0) * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    activate(demo);
    demo.key(&Key::Named(NamedKey::Escape), true, false);
    demo.key(&Key::Named(NamedKey::F6), true, false);
    demo.activation_handlers.set(
        &demo.scene,
        &demo.footer,
        EventPhase::Target,
        |_, source, _| {
            if matches!(source, ActivationSource::Keyboard(_)) {
                EventControl::PreventDefault
            } else {
                EventControl::Continue
            }
        },
    );
    demo.pointer_moved(point);
    demo.pointer_press();
    demo.pointer_release();
    activate(demo);
    if demo.selected.as_ref() != Some(&demo.footer) {
        return Err("registered target filter did not preserve pointer selection during keyboard activation".into());
    }
    expect_pixel(
        &capture(demo, renderer)?,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [89, 225, 149, 255],
    )?;
    demo.activation_handlers
        .remove(&demo.footer, EventPhase::Target);
    activate(demo);
    if demo.selected.is_some() {
        return Err(
            "removing the target handler did not restore default keyboard activation".into(),
        );
    }
    demo.focus_window(false);
    demo.focus_window(true);
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    expect_pixel(&capture(demo, renderer)?, 12, 1, [0, 0, 0, 255])?;
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if !snapshot.same_snapshot(demo.snapshot.as_ref().unwrap())
        || renderer.upload_stats().staged_bytes != staged
        || demo.scene.stats().archetypes != archetypes
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            != images
    {
        return Err("event routing changed layout, shapes, image uploads or archetypes".into());
    }
    println!(
        "GPU event checks passed: registered parent/target handlers, keyboard filtering/removal, pointer defaults, modal boundary, status/selection pixels and retained geometry/text/images"
    );
    Ok(())
}
fn activate(demo: &mut LayoutDemo) {
    demo.key(&Key::Named(NamedKey::Enter), true, false);
    demo.key(&Key::Named(NamedKey::Enter), false, false);
}
fn capture(
    demo: &LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    Ok(renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?)
}
