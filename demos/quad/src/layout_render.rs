//! Responsive scene/layout/text preview. Retains one paragraph/image per label.
use super::*;
use zest::{
    Scene, WidgetId,
    geometry::Size,
    layout::{Edges, LayoutSnapshot, LayoutTree, Length, Style, TextContent},
    text::raster::{Paragraph, Rasterizer, ShapeOptions},
};

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
        if self.scale != Some(scale) {
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
        for label in &mut self.labels {
            let width = snapshot.bounds(&label.id).unwrap().size.width as f32;
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
        self.snapshot = Some(snapshot);
        Ok(())
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
                    for label in &self.labels {
                        let Some(prepared) = &label.prepared else {
                            continue;
                        };
                        let Some(id) = prepared.image else {
                            continue;
                        };
                        let bounds = snapshot.bounds(&label.id).unwrap();
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
                    let bounds = snapshot.bounds(&self.footer).unwrap();
                    if bounds.is_empty() {
                        Ok(())
                    } else {
                        painter.quad(Quad {
                            bounds,
                            color: [1.0; 4],
                        })
                    }
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
mod tests {
    use super::*;
    #[test]
    fn empty_viewports_and_empty_footer_bounds_emit_only_valid_geometry() {
        let mut demo = LayoutDemo::new().unwrap();
        demo.tree.sync(&demo.scene, |_| Style::default()).unwrap();
        demo.snapshot = Some(demo.tree.compute(Size::new(0.0, 0.0)).unwrap().snapshot);
        assert!(demo.paint().unwrap().is_empty());
        demo.snapshot = Some(demo.tree.compute(Size::new(40.0, 40.0)).unwrap().snapshot);
        assert_eq!(demo.paint().unwrap().len(), 1); // background, zero-height footer omitted
    }
}
