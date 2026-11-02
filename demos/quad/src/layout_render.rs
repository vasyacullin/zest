//! Responsive scene/layout/text preview. Retains one paragraph/image per label.
use super::*;
use zest::{
    Scene, WidgetId,
    geometry::Size,
    layout::{Edges, LayoutSnapshot, LayoutTree, Length, Style, TextContent},
    text::raster::{Paragraph, Rasterizer, ShapeOptions},
};
mod interaction_checks;

struct Prepared {
    content: TextContent,
    width: f32,
    paragraph: Paragraph,
    image: Option<ImageId>,
}
struct Label {
    id: WidgetId,
    source: &'static str,
    font_size: f32,
    content: Option<TextContent>,
    prepared: Option<Prepared>,
}
pub(super) struct LayoutDemo {
    scene: Scene,
    tree: LayoutTree,
    raster: Rasterizer,
    root: WidgetId,
    footer: WidgetId,
    labels: Vec<Label>,
    scale: Option<f32>,
    snapshot: Option<LayoutSnapshot>,
    pointer: Option<Point<Logical>>,
    hovered: Option<WidgetId>,
    window_focused: bool,
    modifiers: winit::keyboard::ModifiersState,
    pub tint: [f32; 4],
}
impl LayoutDemo {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut raster = Rasterizer::new();
        raster.register_font(include_bytes!("../../../assets/fonts/NotoSans.ttf").to_vec())?;
        raster
            .register_font(include_bytes!("../../../assets/fonts/NotoSansArabic.ttf").to_vec())?;
        let mut scene = Scene::default();
        let (mut root, mut footer) = (None, None);
        let mut labels = Vec::new();
        scene.update(|ui| {
            root = Some(ui.container("preview", |ui| {
                for (key, source, font_size) in [
                    ("title", "Zest • Layout", 24.0),
                    (
                        "body",
                        "Resize this window to reflow text. Привет, мир! العربية office e\u{301}.",
                        18.0,
                    ),
                ] {
                    labels.push(Label {
                        id: ui.button(key, source),
                        source,
                        font_size,
                        content: None,
                        prepared: None,
                    });
                }
                footer = Some(ui.button("footer", "Layout follows text"));
            }));
        })?;
        Ok(Self {
            scene,
            tree: LayoutTree::default(),
            raster,
            root: root.unwrap(),
            footer: footer.unwrap(),
            labels,
            scale: None,
            snapshot: None,
            pointer: None,
            hovered: None,
            window_focused: true,
            modifiers: winit::keyboard::ModifiersState::empty(),
            tint: [1.0; 4],
        })
    }
    /// The renderer destroys its images during suspension; CPU paragraphs stay reusable.
    pub fn reset_gpu(&mut self) {
        for label in &mut self.labels {
            if let Some(prepared) = &mut label.prepared {
                prepared.image = None;
            }
        }
    }
    pub fn prepare(
        &mut self,
        renderer: &mut Renderer,
        viewport: Size<Logical>,
        scale: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update_geometry(viewport, scale)?;
        let snapshot = self.snapshot.as_ref().unwrap();
        for label in &mut self.labels {
            let Some(bounds) = snapshot.bounds(&label.id) else {
                if let Some(old) = label.prepared.take().and_then(|prepared| prepared.image) {
                    renderer.remove_image(old)?;
                }
                continue;
            };
            let width = bounds.size.width as f32;
            if width * scale < 1.0 {
                if let Some(old) = label.prepared.take().and_then(|prepared| prepared.image) {
                    renderer.remove_image(old)?;
                }
                continue;
            }
            let content = label.content.as_ref().unwrap();
            if label
                .prepared
                .as_ref()
                .is_none_or(|prepared| prepared.width != width || &prepared.content != content)
            {
                let paragraph = self.raster.paragraph_from_shape(content.shaped(), width)?;
                let bitmap = paragraph.bitmap();
                let image = renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)?;
                if let Some(old) = label.prepared.take().and_then(|prepared| prepared.image) {
                    renderer.remove_image(old)?;
                }
                label.prepared = Some(Prepared {
                    content: content.clone(),
                    width,
                    paragraph,
                    image: Some(image),
                });
            } else if let Some(prepared) = &mut label.prepared
                && prepared.image.is_none()
            {
                let bitmap = prepared.paragraph.bitmap();
                prepared.image =
                    Some(renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)?);
            }
        }
        Ok(())
    }
    /// Refresh CPU geometry before position-dependent input, independently of
    /// redraw/upload scheduling. Re-hit a stationary pointer after a resize.
    pub fn update_geometry(
        &mut self,
        viewport: Size<Logical>,
        scale: f32,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if self.scale != Some(scale) {
            if self.scale.is_some() {
                self.pointer = None;
            }
            for label in &mut self.labels {
                label.content = Some(TextContent::new(self.raster.shape(
                    label.source,
                    &["Noto Sans", "Noto Sans Arabic"],
                    ShapeOptions {
                        font_size: label.font_size,
                        scale,
                    },
                )?));
            }
            self.scale = Some(scale);
        }
        // Bound the demonstration's readable column and each bitmap's physical width.
        let column = viewport
            .width
            .min(800.0)
            .min(4090.0 / f64::from(scale) + 48.0);
        self.tree.sync_with_text(&self.scene, |id| {
            if id == &self.root {
                (
                    Style {
                        width: Length::Px(column as f32),
                        padding: Edges::all(24.0),
                        gap: 16.0,
                        shrink: 0.0,
                        ..Default::default()
                    },
                    None,
                )
            } else if id == &self.footer {
                (
                    Style {
                        height: Length::Px(12.0),
                        shrink: 0.0,
                        ..Default::default()
                    },
                    None,
                )
            } else {
                (
                    Style {
                        shrink: 0.0,
                        ..Default::default()
                    },
                    self.labels
                        .iter()
                        .find(|label| &label.id == id)
                        .unwrap()
                        .content
                        .clone(),
                )
            }
        })?;
        let snapshot = self.tree.compute(viewport)?.snapshot;
        let mut changed = self
            .snapshot
            .as_ref()
            .is_none_or(|old| !old.same_snapshot(&snapshot));
        self.snapshot = Some(snapshot);
        let target = self
            .pointer
            .filter(|_| self.window_focused)
            .and_then(|point| self.hit(point));
        changed |= self.set_hovered(target);
        if self.scene.focused().is_some_and(|id| !self.visible(id)) {
            changed |= self.set_focus(None);
        }
        Ok(changed)
    }
    fn interactive(&self, id: &WidgetId) -> bool {
        (id == &self.footer || self.labels.iter().any(|label| &label.id == id))
            && self.scene.state(id).is_some()
    }
    fn visible(&self, id: &WidgetId) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| {
            let viewport = snapshot.viewport();
            snapshot.bounds(id).is_some_and(|bounds| {
                bounds.size.width * f64::from(self.scale.unwrap_or(1.0)) >= 1.0
                    && bounds.size.height * f64::from(self.scale.unwrap_or(1.0)) >= 1.0
                    && bounds
                        .intersection(&Rect::from_xywh(0.0, 0.0, viewport.width, viewport.height))
                        .is_some()
            })
        })
    }
    fn hit(&self, point: Point<Logical>) -> Option<WidgetId> {
        self.snapshot
            .as_ref()?
            .hit_test(point, |id| self.interactive(id) && self.visible(id))
            .cloned()
    }
    fn set_hovered(&mut self, target: Option<WidgetId>) -> bool {
        if self.hovered == target {
            return false;
        }
        if let Some(old) = self.hovered.take() {
            self.scene.set_hovered(&old, false);
        }
        if let Some(id) = &target {
            self.scene.set_hovered(id, true);
        }
        self.hovered = target;
        true
    }
    fn set_focus(&mut self, target: Option<WidgetId>) -> bool {
        if self.scene.focused() == target.as_ref() {
            return false;
        }
        match target {
            Some(id) => self.scene.focus(&id),
            None => {
                self.scene.clear_focus();
                true
            }
        }
    }
    pub fn pointer_moved(&mut self, point: Point<Logical>) -> bool {
        if !self.window_focused || !point.x.is_finite() || !point.y.is_finite() {
            return false;
        }
        self.pointer = Some(point);
        self.set_hovered(self.hit(point))
    }
    pub fn pointer_left(&mut self) -> bool {
        self.pointer = None;
        self.set_hovered(None)
    }
    pub fn pointer_press(&mut self) -> bool {
        if !self.window_focused {
            return false;
        }
        self.set_focus(self.pointer.and_then(|point| self.hit(point)))
    }
    pub fn focus_window(&mut self, focused: bool) -> bool {
        self.window_focused = focused;
        if focused {
            return false;
        }
        self.modifiers = winit::keyboard::ModifiersState::empty();
        let changed = self.pointer_left();
        self.set_focus(None) || changed
    }
    pub fn modifiers(&mut self, modifiers: winit::keyboard::ModifiersState) {
        self.modifiers = if self.window_focused {
            modifiers
        } else {
            winit::keyboard::ModifiersState::empty()
        };
    }
    pub fn key(&mut self, key: &winit::keyboard::Key, pressed: bool, synthetic: bool) -> bool {
        if !self.window_focused
            || !pressed
            || synthetic
            || self.modifiers.control_key()
            || self.modifiers.alt_key()
            || self.modifiers.super_key()
            || key != &winit::keyboard::Key::Named(winit::keyboard::NamedKey::Tab)
        {
            return false;
        }
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        let order: Vec<_> = snapshot
            .boxes()
            .iter()
            .filter(|node| self.interactive(&node.id) && self.visible(&node.id))
            .map(|node| node.id.clone())
            .collect();
        if order.is_empty() {
            return self.set_focus(None);
        }
        let current = order.iter().position(|id| Some(id) == self.scene.focused());
        let index = if self.modifiers.shift_key() {
            current.map_or(order.len() - 1, |index| {
                (index + order.len() - 1) % order.len()
            })
        } else {
            current.map_or(0, |index| (index + 1) % order.len())
        };
        self.set_focus(Some(order[index].clone()))
    }
    pub fn paint(&self) -> Result<DisplayList, PaintError> {
        let Some(snapshot) = &self.snapshot else {
            return Ok(DisplayList::default());
        };
        let viewport = snapshot.viewport();
        if viewport.width <= 0.0 || viewport.height <= 0.0 {
            return Ok(DisplayList::default());
        }
        DisplayList::build(|painter| {
            painter.with_clip(
                Rect::from_xywh(0.0, 0.0, viewport.width, viewport.height),
                |painter| {
                    painter.quad(Quad {
                        bounds: Rect::from_xywh(0.0, 0.0, viewport.width, viewport.height),
                        color: [0.0, 0.0, 0.0, 1.0],
                    })?;
                    for node in snapshot.boxes() {
                        let bounds = node.bounds;
                        if node.id == self.footer {
                            if !bounds.is_empty() {
                                let state = self.scene.state(&self.footer).unwrap();
                                painter.quad(Quad {
                                    bounds,
                                    color: if state.focused() {
                                        [0.2, 0.6, 1.0, 1.0]
                                    } else if state.hovered() {
                                        [0.4, 0.8, 1.0, 1.0]
                                    } else {
                                        [1.0; 4]
                                    },
                                })?;
                            }
                            continue;
                        }
                        let Some(label) = self.labels.iter().find(|label| label.id == node.id)
                        else {
                            continue;
                        };
                        let Some(prepared) = &label.prepared else {
                            continue;
                        };
                        let Some(id) = prepared.image else {
                            continue;
                        };
                        let state = self.scene.state(&label.id).unwrap();
                        if !bounds.is_empty() && (state.focused() || state.hovered()) {
                            painter.quad(Quad {
                                bounds,
                                color: if state.focused() {
                                    [0.08, 0.18, 0.3, 1.0]
                                } else {
                                    [0.035, 0.06, 0.09, 1.0]
                                },
                            })?;
                        }
                        let bitmap = prepared.paragraph.bitmap();
                        let scale = f64::from(bitmap.scale);
                        let mut image = Image::new(
                            id,
                            Rect::from_xywh(
                                bounds.origin.x + f64::from(bitmap.origin_x) / scale,
                                bounds.origin.y + f64::from(bitmap.origin_y) / scale,
                                f64::from(bitmap.width) / scale,
                                f64::from(bitmap.height) / scale,
                            ),
                        );
                        image.tint = self.tint;
                        painter.image(image)?;
                    }
                    Ok(())
                },
            )
        })
    }
    fn remove_images(&mut self, renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
        for label in &mut self.labels {
            if let Some(image) = label
                .prepared
                .as_mut()
                .and_then(|prepared| prepared.image.take())
            {
                renderer.remove_image(image)?;
            }
        }
        Ok(())
    }
}

pub(super) fn check(renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let live = renderer.image_stats().live;
    let mut demo = LayoutDemo::new()?;
    let mut previous_height = 0.0;
    for width in [480.0, 240.0] {
        demo.prepare(renderer, Size::new(width, 450.0), 1.25)?;
        let snapshot = demo.snapshot.as_ref().unwrap();
        let height = snapshot.bounds(&demo.labels[1].id).unwrap().size.height;
        if previous_height > 0.0 && height <= previous_height {
            return Err("narrow layout made the wrapped paragraph shorter".into());
        }
        previous_height = height;
        let footer = snapshot.bounds(&demo.footer).unwrap();
        let capture = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 600, 563)?;
        expect_pixel(&capture, 5, 5, [0, 0, 0, 255])?;
        let (x, y) = opaque_title_pixel(&demo);
        expect_pixel(&capture, x, y, [255; 4])?;
        expect_pixel(
            &capture,
            ((footer.origin.x + 2.0) * 1.25) as u32,
            ((footer.origin.y + 2.0) * 1.25) as u32,
            [255; 4],
        )?;
    }
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let staged = renderer.upload_stats().staged_bytes;
    let guard = demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .layout_selection(zest::text::Selection::caret(0))?;
    let images: Vec<_> = demo
        .labels
        .iter()
        .map(|label| label.prepared.as_ref().unwrap().image)
        .collect();
    demo.tint = [0.5, 0.8, 1.0, 1.0];
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    let capture = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?;
    let (x, y) = opaque_title_pixel(&demo);
    expect_pixel(&capture, x, y, [188, 231, 255, 255])?;
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if !snapshot.same_snapshot(demo.snapshot.as_ref().unwrap())
        || renderer.upload_stats().staged_bytes != staged
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            != images
    {
        return Err("tint/unchanged layout rerasterized or reuploaded text".into());
    }
    interaction_checks::check(&mut demo, renderer)?;
    demo.remove_images(renderer)?;
    demo.reset_gpu();
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 300, 563)?;
    demo.prepare(renderer, Size::new(40.0, 40.0), 1.25)?;
    let tiny = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.25)?, 50, 50)?;
    expect_pixel(&tiny, 25, 25, [0, 0, 0, 255])?;
    demo.prepare(renderer, Size::new(0.0, 0.0), 1.25)?;
    demo.paint()?;
    demo.prepare(renderer, Size::new(480.0, 450.0), 1.5)?;
    let capture = renderer.capture_list(&demo.paint()?, ScaleFactor::new(1.5)?, 720, 675)?;
    let (x, y) = opaque_title_pixel(&demo);
    expect_pixel(&capture, x, y, [188, 231, 255, 255])?;
    demo.remove_images(renderer)?;
    renderer.wait_idle()?;
    if renderer.image_stats().live != live || renderer.image_stats().retired != 0 {
        return Err("layout text image replacement leaked textures".into());
    }
    println!(
        "GPU layout checks passed: responsive paragraph/footer bounds, shared-shape rasterization, tint/image reuse, zero viewport, DPI and image restoration/retirement"
    );
    Ok(())
}

fn opaque_title_pixel(demo: &LayoutDemo) -> (u32, u32) {
    let label = &demo.labels[0];
    let bounds = demo.snapshot.as_ref().unwrap().bounds(&label.id).unwrap();
    let bitmap = label.prepared.as_ref().unwrap().paragraph.bitmap();
    let index = bitmap
        .rgba8
        .chunks_exact(4)
        .position(|pixel| pixel[3] == 255)
        .expect("demo title has opaque glyph pixels");
    let x = (bounds.origin.x * f64::from(bitmap.scale)).round() as i32
        + bitmap.origin_x
        + (index % bitmap.width as usize) as i32;
    let y = (bounds.origin.y * f64::from(bitmap.scale)).round() as i32
        + bitmap.origin_y
        + (index / bitmap.width as usize) as i32;
    (
        u32::try_from(x).expect("positive title margin"),
        u32::try_from(y).expect("positive title margin"),
    )
}

#[cfg(test)]
mod tests;
