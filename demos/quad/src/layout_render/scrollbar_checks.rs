use super::*;
use winit::keyboard::{Key, NamedKey};
use zest::layout::ScrollbarPart;
fn center(bounds: Rect<Logical>) -> Point<Logical> {
    Point::new(
        bounds.origin.x + bounds.size.width * 0.5,
        bounds.origin.y + bounds.size.height * 0.5,
    )
}
pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let before = demo.snapshot.as_ref().unwrap().clone();
    let staged = renderer.upload_stats().staged_bytes;
    let archetypes = demo.scene.stats().archetypes;
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
    let f4 = Key::Named(NamedKey::F4);
    demo.key(&f4, true, false);
    let bar = demo
        .scrollbar()
        .ok_or("overflowing column has no scrollbar")?;
    let point = center(bar.thumb());
    expect_pixel(
        &capture(demo, renderer)?,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [124, 160, 188, 255],
    )?;
    demo.pointer_moved(point);
    expect_pixel(
        &capture(demo, renderer)?,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [149, 188, 218, 255],
    )?;
    demo.pointer_press();
    expect_pixel(
        &capture(demo, renderer)?,
        (point.x * 1.25) as u32,
        (point.y * 1.25) as u32,
        [124, 203, 255, 255],
    )?;
    if !demo.scrollbar_dragging() || demo.scroll_offset.y != 0.0 || demo.hit(point).is_some() {
        return Err("scrollbar press jumped or activated a scene target".into());
    }
    demo.pointer_moved(Point::new(point.x, 1000.0));
    let end = demo.scrollbar().unwrap();
    let end_point = center(end.thumb());
    if !demo.scrollbar_dragging() || demo.scroll_offset.y != end.max_offset() {
        return Err("scrollbar did not retain capture and clamp outside its track".into());
    }
    expect_pixel(
        &capture(demo, renderer)?,
        (end_point.x * 1.25) as u32,
        (end_point.y * 1.25) as u32,
        [124, 203, 255, 255],
    )?;
    if !demo.pointer_release() || demo.scrollbar_dragging() || demo.selected.is_some() {
        return Err("scrollbar release retained capture or emitted a widget activation".into());
    }
    expect_pixel(
        &capture(demo, renderer)?,
        (end_point.x * 1.25) as u32,
        (end_point.y * 1.25) as u32,
        [124, 160, 188, 255],
    )?;
    let before_thumb = Point::new(center(end.track()).x, end.track().origin.y + 0.1);
    demo.pointer_moved(before_thumb);
    demo.pointer_press();
    demo.pointer_release();
    if demo.scroll_offset.y != end.page_offset(ScrollbarPart::BeforeThumb)
        || demo.selected.is_some()
    {
        return Err("scrollbar track did not page exactly one viewport".into());
    }
    demo.pointer_moved(center(demo.scrollbar().unwrap().thumb()));
    demo.pointer_press();
    demo.focus_window(false);
    if demo.scrollbar_dragging() {
        return Err("blur retained scrollbar capture".into());
    }
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
        || archetypes != demo.scene.stats().archetypes
        || images
            != demo
                .labels
                .iter()
                .map(|label| label.prepared.as_ref().unwrap().image)
                .collect::<Vec<_>>()
    {
        return Err(
            "scrollbar gestures invalidated paragraphs/images or migrated ECS archetypes".into(),
        );
    }
    println!(
        "GPU scrollbar checks passed: idle/hover/pressed/drag/release pixels, track paging, bounded capture, blur cancellation and retained text images/archetypes"
    );
    Ok(())
}
fn capture(
    demo: &LayoutDemo,
    renderer: &mut Renderer,
) -> Result<Capture, Box<dyn std::error::Error>> {
    Ok(renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?)
}
