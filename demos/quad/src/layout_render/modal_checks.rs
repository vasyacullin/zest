use super::*;
use winit::keyboard::{Key, ModifiersState, NamedKey};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
    outside: Point<Logical>,
) -> Result<(), Box<dyn std::error::Error>> {
    let footer = demo.footer.clone();
    let body = demo.labels[1].id.clone();
    let f3 = Key::Named(NamedKey::F3);
    let escape = Key::Named(NamedKey::Escape);
    let tab = Key::Named(NamedKey::Tab);
    demo.set_focus(Some(footer.clone()));
    if !demo.key(&f3, true, false)
        || demo.scene.modal_root() != Some(&body)
        || demo.scene.focused() != Some(&body)
    {
        return Err("F3 did not restrict input and focus to the body".into());
    }
    demo.pointer_moved(outside);
    if demo.pointer_press()
        || demo.pointer_release()
        || demo.hovered.is_some()
        || demo.scene.focused() != Some(&body)
        || demo.key(&tab, true, false)
    {
        return Err("outside click/Tab escaped the modal input scope".into());
    }
    demo.modifiers(ModifiersState::SHIFT);
    if demo.key(&tab, true, false)
        || demo.key_event(&f3, true, true, false)
        || demo.key(&escape, true, true)
    {
        return Err("Shift+Tab, repeat or synthetic key escaped the modal input scope".into());
    }
    demo.modifiers(ModifiersState::empty());
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (outside.x * 1.25) as u32,
        (outside.y * 1.25) as u32,
        [97, 97, 97, 255],
    )?;
    let bounds = demo.snapshot.as_ref().unwrap().bounds(&body).unwrap();
    expect_pixel(
        &image,
        ((bounds.origin.x + 10.0) * 1.25) as u32,
        ((bounds.origin.y + 1.0) * 1.25) as u32,
        [255, 203, 89, 255],
    )?;
    let (x, y) = opaque_title_pixel(demo);
    expect_pixel(&image, x, y, [116, 144, 160, 255])?;
    if !demo.scene.state(&footer).unwrap().enabled() || demo.scene.input_allowed(&footer) {
        return Err("modal blocking changed availability instead of input membership".into());
    }
    let enter = Key::Named(NamedKey::Enter);
    demo.key(&enter, true, false);
    if !demo.key(&escape, true, false)
        || demo.modal_active()
        || demo.scene.focused() != Some(&footer)
        || demo.key(&enter, false, false)
        || demo.selected.is_some()
    {
        return Err("Esc did not restore focus or cancel the in-flight modal key".into());
    }
    let image = capture(demo, renderer)?;
    expect_pixel(
        &image,
        (outside.x * 1.25) as u32,
        (outside.y * 1.25) as u32,
        [124, 203, 255, 255],
    )?;
    expect_pixel(&image, x, y, [188, 231, 255, 255])?;
    demo.focus_window(false);
    demo.focus_window(true);
    Ok(())
}
fn capture(
    demo: &LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    Ok(renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?)
}
