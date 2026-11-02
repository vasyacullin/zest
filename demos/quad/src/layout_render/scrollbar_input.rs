use super::*;
use zest::{
    layout::{Axis, ScrollbarGeometry, ScrollbarPart},
    paint::Painter,
};

#[derive(Clone, Copy)]
pub(super) struct Drag {
    grab: f64,
    track: Rect<Logical>,
    thumb_size: Size<Logical>,
    viewport: f64,
    max_offset: f64,
}
impl Drag {
    fn matches(self, bar: ScrollbarGeometry) -> bool {
        self.track == bar.track()
            && self.thumb_size == bar.thumb().size
            && self.viewport == bar.viewport_extent()
            && self.max_offset == bar.max_offset()
    }
}
impl LayoutDemo {
    pub fn scrollbar_dragging(&self) -> bool {
        self.scrollbar_drag.is_some()
    }
    pub(super) fn scrollbar(&self) -> Option<ScrollbarGeometry> {
        if !self.clipped
            || self.modal_active()
            || self
                .scene
                .state(&self.root)
                .is_none_or(|state| !state.enabled())
        {
            return None;
        }
        let snapshot = self.snapshot.as_ref()?;
        let port = snapshot.visible_bounds(&self.root)?;
        let bounds = snapshot.bounds(&self.root)?;
        if bounds.size.width <= 48.0 || port.size.width < 16.0 || port.size.height <= 8.0 {
            return None;
        }
        ScrollbarGeometry::new(
            Rect::from_xywh(
                port.origin.x + port.size.width - 12.0,
                port.origin.y + 4.0,
                8.0,
                port.size.height - 8.0,
            ),
            Axis::Column,
            bounds.size.height,
            snapshot.scroll(&self.root)?,
            16.0,
        )
    }
    pub(super) fn scrollbar_hovered(&self) -> bool {
        self.pointer.is_some_and(|point| {
            self.scrollbar()
                .is_some_and(|bar| bar.hit_test(point) == Some(ScrollbarPart::Thumb))
        })
    }
    pub(super) fn clear_scrollbar_drag(&mut self) -> bool {
        if self.scrollbar_drag.take().is_none() {
            return false;
        }
        self.activation.cancel(&mut self.scene);
        true
    }
    pub(super) fn validate_scrollbar_drag(&mut self) -> bool {
        if self
            .scrollbar_drag
            .is_some_and(|drag| self.scrollbar().is_none_or(|bar| !drag.matches(bar)))
        {
            return self.clear_scrollbar_drag();
        }
        false
    }
    pub(super) fn scroll_to(
        &mut self,
        next: Point<Logical>,
        keep_drag: bool,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(snapshot) = &self.snapshot else {
            return Ok(false);
        };
        let metrics = snapshot.scroll(&self.root).unwrap_or_default();
        if !next.x.is_finite() || !next.y.is_finite() {
            return Ok(false);
        }
        let next = Point::new(
            next.x.clamp(0.0, metrics.max_offset.x),
            next.y.clamp(0.0, metrics.max_offset.y),
        );
        if next == metrics.offset {
            return Ok(false);
        }
        let viewport = snapshot.viewport();
        let previous = self.scroll_offset;
        self.scroll_offset = next;
        if let Err(error) = self.update_geometry_impl(viewport, self.scale.unwrap(), keep_drag) {
            self.scroll_offset = previous;
            return Err(error);
        }
        Ok(true)
    }
    pub(super) fn press_scrollbar(&mut self) -> Option<bool> {
        let point = self.pointer?;
        let bar = self.scrollbar()?;
        let part = bar.hit_test(point)?;
        let changed = self.activation.pointer_press(&mut self.scene, None).changed;
        if let Some(grab) = bar.grab_offset(point) {
            self.scrollbar_drag = Some(Drag {
                grab,
                track: bar.track(),
                thumb_size: bar.thumb().size,
                viewport: bar.viewport_extent(),
                max_offset: bar.max_offset(),
            });
            return Some(true);
        }
        let next = Point::new(self.scroll_offset.x, bar.page_offset(part));
        Some(match self.scroll_to(next, false) {
            Ok(scrolled) => scrolled || changed,
            Err(error) => {
                eprintln!("Scrollbar paging rejected: {error}");
                changed
            }
        })
    }
    pub(super) fn drag_scrollbar(&mut self, point: Point<Logical>) -> Option<bool> {
        let drag = self.scrollbar_drag?;
        let Some(bar) = self.scrollbar().filter(|bar| drag.matches(*bar)) else {
            return Some(self.clear_scrollbar_drag());
        };
        let offset = bar.drag_offset(point, drag.grab)?;
        Some(
            match self.scroll_to(Point::new(self.scroll_offset.x, offset), true) {
                Ok(changed) => changed,
                Err(error) => {
                    eprintln!("Scrollbar drag rejected: {error}");
                    self.clear_scrollbar_drag()
                }
            },
        )
    }
    pub(super) fn paint_scrollbar(&self, painter: &mut Painter<'_>) -> Result<(), PaintError> {
        let Some(bar) = self.scrollbar() else {
            return Ok(());
        };
        painter.with_clip(bar.track(), |painter| {
            painter.quad(Quad {
                bounds: bar.track(),
                color: [0.025, 0.035, 0.05, 1.0],
            })?;
            painter.quad(Quad {
                bounds: bar.thumb(),
                color: if self.scrollbar_dragging() {
                    [0.2, 0.6, 1.0, 1.0]
                } else if self.scrollbar_hovered() {
                    [0.3, 0.5, 0.7, 1.0]
                } else {
                    [0.2, 0.35, 0.5, 1.0]
                },
            })
        })
    }
}
