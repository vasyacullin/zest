//! Offscreen page-navigation replay; no native input or clipboard access.
use super::*;

pub(super) fn check(
    editor: &mut TextDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    editor.modifiers(ModifiersState::empty());
    editor.edit(|input| {
        input.buffer.select_all()?;
        input.buffer.insert(&"abcdefghij\n".repeat(16))?;
        input.buffer.set_selection(Selection::caret(8))
    })?;
    editor.prepare(renderer, 1.25)?;
    let history = editor.input.buffer.history_stats();
    let image = editor.image;
    let staged = renderer.upload_stats().staged_bytes;
    let before = editor.rect(editor.current_caret()?);
    editor.key(&Key::Named(NamedKey::PageDown), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    let caret = editor.rect(editor.current_caret()?);
    if (caret.origin.y - before.origin.y).abs() > 1e-4
        || (editor.scroll_y - VIEW_HEIGHT).abs() > caret.size.height * 0.5 + 1e-4
    {
        return Err("page movement lost the caret's viewport-relative height".into());
    }
    let capture = capture_editor(editor, renderer)?;
    expect_pixel(
        &capture,
        (caret.origin.x * 1.25 - 0.5).ceil() as u32,
        ((caret.origin.y + caret.size.height * 0.5) * 1.25).floor() as u32,
        [255; 4],
    )?;
    let anchor = editor.input.buffer.selection().head;
    editor.modifiers(ModifiersState::SHIFT);
    editor.key(&Key::Named(NamedKey::PageDown), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    if editor.input.buffer.selection().anchor != anchor
        || editor.input.buffer.selection().head <= anchor
    {
        return Err("Shift+PageDown did not retain its selection anchor".into());
    }
    let capture = capture_editor(editor, renderer)?;
    let selected = editor
        .paragraph
        .selection_geometry(editor.current_navigation().unwrap())?
        .into_iter()
        .map(|rect| editor.rect(rect))
        .filter_map(|rect| rect.intersection(&VIEW))
        .find(|rect| rect.size.width > 3.0 && rect.size.height > 3.0)
        .ok_or("page selection has no visible highlight")?;
    expect_pixel(
        &capture,
        ((selected.origin.x + selected.size.width * 0.5) * 1.25).floor() as u32,
        ((selected.origin.y + selected.size.height - 1.5) * 1.25).floor() as u32,
        [63, 124, 179, 255],
    )?;
    editor.key(&Key::Named(NamedKey::PageUp), None, true, false)?;
    if editor.input.buffer.selection() != Selection::caret(anchor) {
        return Err("Shift+PageUp did not return to the anchor".into());
    }
    editor.modifiers(ModifiersState::empty());
    editor.key(&Key::Named(NamedKey::PageUp), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    if editor.input.buffer.selection() != Selection::caret(8) || editor.scroll_y != 0.0 {
        return Err("PageUp did not restore the initial column and viewport".into());
    }
    for _ in 0..12 {
        editor.key(&Key::Named(NamedKey::PageDown), None, true, false)?;
    }
    editor.wheel(
        MouseScrollDelta::LineDelta(0.0, 1e6),
        ScaleFactor::new(1.25)?,
    )?;
    if editor.scroll_y != 0.0
        || editor.input.buffer.selection() != Selection::caret(editor.input.buffer.text().len())
        || !editor
            .key(&Key::Named(NamedKey::PageDown), None, true, false)?
            .changed
    {
        return Err("page boundary did not reveal the manually hidden final caret".into());
    }
    editor.prepare(renderer, 1.25)?;
    capture_editor(editor, renderer)?;
    let area = editor
        .ime_area()
        .ok_or("missing page-navigation candidate area")?;
    if area.origin.y < VIEW.origin.y
        || area.origin.y + area.size.height > VIEW.origin.y + VIEW_HEIGHT
        || editor.input.buffer.history_stats() != history
        || editor.image != image
        || renderer.upload_stats().staged_bytes != staged
    {
        return Err("paging changed history/texture uploads or misplaced the IME area".into());
    }
    println!(
        "GPU page checks passed: viewport-height movement, stable caret column/height, Shift selection pixels, document edges, manual scrolling and no text uploads"
    );
    Ok(())
}

fn capture_editor(
    editor: &TextDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    let list = DisplayList::build(|painter| editor.paint(painter))?;
    Ok(renderer.capture_list(&list, ScaleFactor::new(1.25)?, 800, 450)?)
}
