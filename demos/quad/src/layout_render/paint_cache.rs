use super::*;
use zest::layout::ScrollbarGeometry;

struct Geometry(Option<LayoutSnapshot>);
impl PartialEq for Geometry {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => a.same_snapshot(b),
            (None, None) => true,
            _ => false,
        }
    }
}
/// The inputs actually read by paint, rather than transient paint-dirty/capture
/// flags. A failed or skipped frame can therefore reuse the prepared commands.
#[derive(PartialEq)]
pub(super) struct PaintKey {
    geometry: Geometry,
    states: Vec<Option<[bool; 4]>>,
    images: Vec<Option<(ImageId, Rect<Logical>)>>,
    tint: [f32; 4],
    selected: Option<WidgetId>,
    modal_root: Option<WidgetId>,
    intercept: bool,
    scrollbar: Option<(ScrollbarGeometry, bool, bool)>,
}
impl LayoutDemo {
    fn paint_key(&self) -> PaintKey {
        PaintKey {
            geometry: Geometry(self.snapshot.clone()),
            states: self
                .snapshot
                .iter()
                .flat_map(|snapshot| snapshot.boxes())
                .map(|node| {
                    self.scene.state(&node.id).map(|state| {
                        [
                            state.hovered(),
                            state.focused(),
                            state.pressed(),
                            self.scene.input_allowed(&node.id),
                        ]
                    })
                })
                .collect(),
            images: self
                .labels
                .iter()
                .map(|label| {
                    let prepared = label.prepared.as_ref()?;
                    let id = prepared.image?;
                    let bitmap = prepared.paragraph.bitmap();
                    let scale = f64::from(bitmap.scale);
                    Some((
                        id,
                        Rect::from_xywh(
                            f64::from(bitmap.origin_x) / scale,
                            f64::from(bitmap.origin_y) / scale,
                            f64::from(bitmap.width) / scale,
                            f64::from(bitmap.height) / scale,
                        ),
                    ))
                })
                .collect(),
            tint: self.tint,
            selected: self.selected.clone(),
            modal_root: self.scene.modal_root().cloned(),
            intercept: self.intercept_activation,
            scrollbar: self.scrollbar().map(|bar| {
                let dragging = self.scrollbar_dragging();
                (bar, !dragging && self.scrollbar_hovered(), dragging)
            }),
        }
    }
    pub fn cached_paint(&mut self) -> Result<Arc<DisplayList>, PaintError> {
        let key = self.paint_key();
        if let Some(list) = self.paint_cache.get(&key) {
            return Ok(list);
        }
        // Build while borrowing the scene immutably, then publish atomically.
        // Failure leaves the previous cache entry available only under its key.
        let list = self.paint()?;
        self.paint_cache.get_or_try_build(key, || Ok(list))
    }
}
