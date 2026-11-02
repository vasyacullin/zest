//! Controlled-time replay through the same pointer binding as the window.
use super::*;

pub(super) fn check(
    editor: &mut TextDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    editor.modifiers(ModifiersState::empty());
    editor.edit(|input| {
        input.buffer.select_all()?;
        input.buffer.insert("one two three\nالعربية e\u{301}")?;
        input.buffer.set_selection(Selection::caret(0))
    })?;
    editor.prepare(renderer, 1.25)?;
    let image = editor.image;
    let staged = renderer.upload_stats().staged_bytes;
    let history = editor.input.buffer.history_stats();
    let now = Instant::now();
    let point = point(editor, 5, false)?;
    editor.pointer_press(point, false, now)?;
    editor.end_drag();
    editor.ime(&Ime::Disabled)?;
    editor.ime(&Ime::Enabled)?;
    editor.pointer_press(point, false, now + Duration::from_millis(100))?;
    if editor.input.buffer.selection() != Selection::new(4, 7) {
        return Err("double-click did not select its word across native IME toggles".into());
    }
    highlight(editor, renderer)?;
    editor.drag_to(self::point(editor, 9, false)?)?;
    if editor.input.buffer.selection() != Selection::new(4, 13) {
        return Err("double-click drag did not extend by whole words".into());
    }
    highlight(editor, renderer)?;
    editor.drag_to(self::point(editor, 0, false)?)?;
    if editor.input.buffer.selection() != Selection::new(7, 0) {
        return Err("word drag lost its original word while crossing the anchor".into());
    }
    editor.end_drag();
    for tick in [1000, 1100, 1200] {
        editor.pointer_press(point, false, now + Duration::from_millis(tick))?;
        editor.end_drag();
    }
    if editor.input.buffer.selection() != Selection::new(0, 14) {
        return Err("triple-click did not select the displayed line and its newline".into());
    }
    highlight(editor, renderer)?;
    let mut clipboard = crate::clipboard::MemoryClipboard::default();
    editor.clipboard(ClipboardAction::Copy, &mut clipboard)?;
    if clipboard.text.as_deref() != Some("one two three\n") {
        return Err("copy lost the triple-click line break".into());
    }
    let point = self::point(editor, 14, true)?;
    for tick in [2000, 2100] {
        editor.pointer_press(point, false, now + Duration::from_millis(tick))?;
        editor.end_drag();
    }
    if editor.input.buffer.selected_text() != "العربية" {
        return Err("double-click did not select the whole RTL word".into());
    }
    highlight(editor, renderer)?;
    if editor.input.buffer.history_stats() != history
        || editor.image != image
        || renderer.upload_stats().staged_bytes != staged
    {
        return Err("multi-click selection changed history or uploaded another text image".into());
    }
    println!(
        "GPU pointer checks passed: double/triple-click, whole-word drag/crossing, RTL, newline copy and highlight pixels without text uploads"
    );
    Ok(())
}

fn point(
    editor: &TextDemo,
    offset: usize,
    rtl: bool,
) -> Result<Point<Logical>, Box<dyn std::error::Error>> {
    let rect = editor.rect(editor.paragraph.caret(offset)?);
    Ok(Point::new(
        rect.origin.x + if rtl { -0.1 } else { 0.1 },
        rect.origin.y + rect.size.height * 0.5,
    ))
}

fn highlight(
    editor: &mut TextDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    editor.prepare(renderer, 1.25)?;
    let list = DisplayList::build(|painter| editor.paint(painter))?;
    let capture = renderer.capture_list(&list, ScaleFactor::new(1.25)?, 800, 450)?;
    let selected = editor
        .paragraph
        .selection_geometry(editor.current_navigation().unwrap())?
        .into_iter()
        .map(|rect| editor.rect(rect))
        .filter_map(|rect| rect.intersection(&VIEW))
        .find(|rect| rect.size.width > 3.0 && rect.size.height > 3.0)
        .ok_or("grouped pointer selection has no visible highlight")?;
    expect_pixel(
        &capture,
        ((selected.origin.x + selected.size.width * 0.5) * 1.25).floor() as u32,
        ((selected.origin.y + selected.size.height - 1.5) * 1.25).floor() as u32,
        [63, 124, 179, 255],
    )?;
    Ok(())
}
