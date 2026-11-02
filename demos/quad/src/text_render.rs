use super::text_input::{KeyResult, TextInput};
use super::*;
use winit::{
    event::Ime,
    keyboard::{Key, ModifiersState, PhysicalKey},
};
use zest::{
    paint::Painter,
    text::{
        Selection,
        raster::{Paragraph, RasterOptions, Rasterizer, TextRect},
    },
};

const FAMILIES: &[&str] = &["Noto Sans", "Noto Sans Arabic"];
const SAMPLE: &str = "Zest • Привет, мир!\noffice e\u{301} • العربية";
pub(super) const FIELD: Rect<Logical> = Rect::from_xywh(288.0, 272.0, 344.0, 84.0);
const VIEW_WIDTH: f64 = 320.0;
const VIEW_HEIGHT: f64 = 68.0;

pub(super) struct TextDemo {
    raster: Rasterizer,
    image: Option<ImageId>,
    paragraph: Paragraph,
    uploaded: bool,
    pub input: TextInput,
    scroll_x: f64,
    scroll_y: f64,
}
impl TextDemo {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut raster = Rasterizer::new();
        raster.register_font(include_bytes!("../../../assets/fonts/NotoSans.ttf").to_vec())?;
        raster
            .register_font(include_bytes!("../../../assets/fonts/NotoSansArabic.ttf").to_vec())?;
        let paragraph = raster.paragraph(SAMPLE, FAMILIES, Self::options(1.0))?;
        Ok(Self {
            raster,
            image: None,
            paragraph,
            uploaded: false,
            input: TextInput::new(SAMPLE),
            scroll_x: 0.0,
            scroll_y: 0.0,
        })
    }
    pub fn reset(&mut self) {
        self.image = None;
        self.uploaded = false;
        self.input.focus(false);
    }
    fn options(scale: f32) -> RasterOptions {
        RasterOptions {
            font_size: 18.0,
            width: VIEW_WIDTH as f32,
            scale,
        }
    }
    fn refresh(&mut self, scale: f32) -> Result<(), Box<dyn std::error::Error>> {
        let display = self.input.buffer.display_text();
        if self.paragraph.text() != display || self.paragraph.bitmap().scale != scale {
            self.paragraph = self
                .raster
                .paragraph(&display, FAMILIES, Self::options(scale))?;
            self.uploaded = false;
        }
        Ok(())
    }
    // Stage model/history and layout before accepting an edit. A paragraph
    // budget error retains the last accepted document and its undo history.
    fn edit<T>(
        &mut self,
        action: impl FnOnce(&mut TextInput) -> Result<T, zest::text::EditError>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let mut next = self.input.clone();
        let result = action(&mut next)?;
        let display = next.buffer.display_text();
        if self.paragraph.text() != display {
            self.paragraph = self.raster.paragraph(
                &display,
                FAMILIES,
                Self::options(self.paragraph.bitmap().scale),
            )?;
            self.uploaded = false;
        }
        self.input = next;
        Ok(result)
    }
    pub fn key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        pressed: bool,
        synthetic: bool,
    ) -> Result<KeyResult, Box<dyn std::error::Error>> {
        self.edit(|input| input.key(key, text, pressed, synthetic))
    }
    pub fn key_physical(
        &mut self,
        key: &Key,
        physical: PhysicalKey,
        text: Option<&str>,
        pressed: bool,
        synthetic: bool,
    ) -> Result<KeyResult, Box<dyn std::error::Error>> {
        self.edit(|input| input.key_physical(key, physical, text, pressed, synthetic))
    }
    pub fn ime(&mut self, ime: &Ime) -> Result<bool, Box<dyn std::error::Error>> {
        self.edit(|input| input.ime(ime))
    }
    pub fn focus(&mut self, focused: bool) -> bool {
        self.input.focus(focused)
    }
    pub fn modifiers(&mut self, modifiers: ModifiersState) {
        self.input.modifiers = modifiers;
    }
    pub fn cancel_composition(&mut self) {
        self.input.cancel_composition();
    }
    pub fn click(
        &mut self,
        point: Point<Logical>,
        extend: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.input.cancel_composition();
        self.input.focus(true);
        self.refresh(self.paragraph.bitmap().scale)?;
        let offset = self.paragraph.hit_test(
            point.x - 300.0 + self.scroll_x,
            point.y - 280.0 + self.scroll_y,
        )?;
        self.edit(|input| {
            input.buffer.set_selection(Selection::new(
                if extend {
                    input.buffer.selection().anchor
                } else {
                    offset
                },
                offset,
            ))
        })?;
        Ok(())
    }
    pub fn prepare(
        &mut self,
        renderer: &mut Renderer,
        scale: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.refresh(scale)?;
        if !self.uploaded {
            let bitmap = self.paragraph.bitmap();
            let id = renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)?;
            if let Some(old) = self.image.replace(id) {
                renderer.remove_image(old)?;
            }
            self.uploaded = true;
        }
        if let Some(selection) = self.input.buffer.display_selection() {
            let caret = self.paragraph.caret(selection.head)?;
            if caret.x < self.scroll_x {
                self.scroll_x = caret.x;
            }
            if caret.x + caret.width > self.scroll_x + VIEW_WIDTH {
                self.scroll_x = caret.x + caret.width - VIEW_WIDTH;
            }
            if caret.y < self.scroll_y {
                self.scroll_y = caret.y;
            }
            if caret.y + caret.height > self.scroll_y + VIEW_HEIGHT {
                self.scroll_y = caret.y + caret.height - VIEW_HEIGHT;
            }
        }
        self.scroll_x = self.scroll_x.max(0.0);
        self.scroll_y = self.scroll_y.max(0.0);
        Ok(())
    }
    fn rect(&self, rect: TextRect) -> Rect<Logical> {
        Rect::from_xywh(
            300.0 + rect.x - self.scroll_x,
            280.0 + rect.y - self.scroll_y,
            rect.width,
            rect.height,
        )
    }
    pub fn ime_area(&self) -> Option<Rect<Logical>> {
        let offset = self
            .input
            .buffer
            .display_selection()
            .map(|s| s.head)
            .unwrap_or_else(|| {
                self.input
                    .buffer
                    .preedit_range()
                    .map_or(self.input.buffer.selection().head, |r| r.end)
            });
        let caret = self.paragraph.caret(offset).ok()?;
        let rect = self.rect(caret);
        let height = rect.size.height.clamp(1.0, VIEW_HEIGHT);
        Some(Rect::from_xywh(
            rect.origin.x.clamp(300.0, 619.0),
            rect.origin.y.clamp(280.0, 348.0 - height),
            1.0,
            height,
        ))
    }
    pub fn paint(&self, painter: &mut Painter<'_>) -> Result<(), PaintError> {
        let bitmap = self.paragraph.bitmap();
        let scale = f64::from(bitmap.scale);
        let mut image = Image::new(
            self.image.unwrap(),
            Rect::from_xywh(
                300.0 + f64::from(bitmap.origin_x) / scale - self.scroll_x,
                280.0 + f64::from(bitmap.origin_y) / scale - self.scroll_y,
                f64::from(bitmap.width) / scale,
                f64::from(bitmap.height) / scale,
            ),
        );
        image.tint = [0.82, 0.9, 1.0, 1.0];
        painter.quad(Quad {
            bounds: FIELD,
            color: if self.input.focused {
                [0.12, 0.42, 0.75, 1.0]
            } else {
                [0.1, 0.14, 0.2, 1.0]
            },
        })?;
        painter.quad(Quad {
            bounds: Rect::from_xywh(289.0, 273.0, 342.0, 82.0),
            color: [0.015, 0.025, 0.04, 1.0],
        })?;
        painter.with_clip(
            Rect::from_xywh(300.0, 280.0, VIEW_WIDTH, VIEW_HEIGHT),
            |p| {
                if self.input.focused
                    && let Some(selection) = self.input.buffer.display_selection()
                {
                    for rect in self
                        .paragraph
                        .selection_rects(selection)
                        .expect("validated display selection")
                    {
                        let bounds = self.rect(rect);
                        if !bounds.is_empty() {
                            p.quad(Quad {
                                bounds,
                                color: [0.05, 0.2, 0.45, 1.0],
                            })?;
                        }
                    }
                }
                p.image(image)?;
                if let Some(range) = self.input.buffer.preedit_range() {
                    for rect in self
                        .paragraph
                        .selection_rects(Selection::new(range.start, range.end))
                        .expect("validated preedit range")
                    {
                        if rect.width > 0.0 {
                            p.quad(Quad {
                                bounds: self.rect(TextRect {
                                    y: rect.y + rect.height - 2.0,
                                    height: 1.0,
                                    ..rect
                                }),
                                color: [0.3, 0.7, 1.0, 1.0],
                            })?;
                        }
                    }
                }
                if self.input.focused
                    && let Some(selection) = self.input.buffer.display_selection()
                    && selection.is_caret()
                {
                    p.quad(Quad {
                        bounds: self.rect(
                            self.paragraph
                                .caret(selection.head)
                                .expect("validated display caret"),
                        ),
                        color: [1.0; 4],
                    })?;
                }
                Ok(())
            },
        )
    }

    pub fn check(&mut self, renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
        let live = renderer.image_stats().live;
        for scale in [1.0, 1.25, 1.5] {
            let bitmap = self.raster.rasterize(
                "office Привет\nلا العربية e\u{301}",
                FAMILIES,
                RasterOptions {
                    width: 190.0,
                    scale,
                    ..Default::default()
                },
            )?;
            if bitmap.metrics.missing_glyphs != 0
                || bitmap.metrics.rtl_runs == 0
                || bitmap.metrics.fonts != 2
            {
                return Err(format!("text shaping/fallback failed: {:?}", bitmap.metrics).into());
            }
            let id = renderer.upload_rgba8(bitmap.width, bitmap.height, &bitmap.rgba8)?;
            // An opaque black quad gives a known linear background. The white
            // texture's coverage must be blended in linear space then sRGB encoded.
            let clip = (scale == 1.25).then(|| {
                Rect::<Logical>::from_xywh(
                    7.0 / f64::from(scale),
                    3.0 / f64::from(scale),
                    f64::from(bitmap.width - 16) / f64::from(scale),
                    f64::from(bitmap.height - 8) / f64::from(scale),
                )
            });
            let list = DisplayList::build(|p| {
                p.quad(Quad {
                    bounds: Rect::from_xywh(
                        0.0,
                        0.0,
                        f64::from(bitmap.width) / f64::from(scale),
                        f64::from(bitmap.height) / f64::from(scale),
                    ),
                    color: [0.0, 0.0, 0.0, 1.0],
                })?;
                let image = Image::new(
                    id,
                    Rect::from_xywh(
                        0.0,
                        0.0,
                        f64::from(bitmap.width) / f64::from(scale),
                        f64::from(bitmap.height) / f64::from(scale),
                    ),
                );
                match clip {
                    Some(clip) => p.with_clip(clip, |p| p.image(image)),
                    None => p.image(image),
                }
            })?;
            let capture = renderer.capture_list(
                &list,
                ScaleFactor::new(f64::from(scale))?,
                bitmap.width,
                bitmap.height,
            )?;
            let mut ink = 0;
            for (index, source) in bitmap.rgba8.chunks_exact(4).enumerate() {
                let x = index as u32 % bitmap.width;
                let y = index as u32 / bitmap.width;
                let clipped = clip.is_some()
                    && (x < 7 || y < 3 || x >= bitmap.width - 9 || y >= bitmap.height - 5);
                let linear = if clipped {
                    0.0
                } else {
                    f64::from(source[3]) / 255.0
                };
                let srgb = if linear <= 0.0031308 {
                    12.92 * linear
                } else {
                    1.055 * linear.powf(1.0 / 2.4) - 0.055
                };
                let expected = (srgb * 255.0).round() as u8;
                expect_pixel(&capture, x, y, [expected, expected, expected, 255])?;
                ink += usize::from(source[3] != 0);
            }
            if ink < 100 {
                return Err("text capture had no meaningful glyph coverage".into());
            }
            renderer.remove_image(id)?;
        }
        renderer.wait_idle()?;
        if renderer.image_stats().live != live {
            return Err("text checks leaked images".into());
        }
        println!(
            "GPU text checks passed: Latin/Cyrillic/Arabic fallback, ligatures, every coverage pixel at DPI 1/1.25/1.5, clipping and image retirement"
        );
        Ok(())
    }
}

pub(super) fn check_editor(renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let live = renderer.image_stats().live;
    let mut editor = TextDemo::new()?;
    editor.focus(true);
    let command = if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        ModifiersState::CONTROL
    };
    editor.modifiers(command);
    editor.key(&Key::Character("a".into()), None, true, false)?;
    editor.modifiers(ModifiersState::empty());
    editor.key(&Key::Character(" ".into()), Some("     "), true, false)?;
    editor.prepare(renderer, 1.25)?;
    let before = renderer.upload_stats().staged_bytes;
    let image = editor.image;
    editor.modifiers(command);
    editor.key(&Key::Character("a".into()), None, true, false)?;
    editor.modifiers(ModifiersState::empty());
    editor.prepare(renderer, 1.25)?;
    if editor.image != image || renderer.upload_stats().staged_bytes != before {
        return Err("selection change rerasterized or uploaded text".into());
    }
    let capture_editor = |editor: &TextDemo,
                          renderer: &mut Renderer|
     -> Result<Capture, Box<dyn std::error::Error>> {
        let list = DisplayList::build(|p| editor.paint(p))?;
        for primitive in list.primitives() {
            if let Primitive::Image(image) = primitive
                && let Err(error) = image.validate()
            {
                return Err(format!("editor image after clipping: {image:?}: {error}").into());
            }
        }
        Ok(renderer.capture_list(&list, ScaleFactor::new(1.25)?, 800, 450)?)
    };
    let capture = capture_editor(&editor, renderer)?;
    expect_pixel(&capture, 378, 354, [63, 124, 179, 255])?;
    editor.key(
        &Key::Named(winit::keyboard::NamedKey::ArrowRight),
        None,
        true,
        false,
    )?;
    editor.prepare(renderer, 1.25)?;
    let selection = editor.input.buffer.selection();
    let caret = editor.rect(editor.paragraph.caret(selection.head)?);
    let capture = capture_editor(&editor, renderer)?;
    let x = (caret.origin.x * 1.25 - 0.5).ceil() as u32;
    let y = ((caret.origin.y + caret.size.height * 0.5) * 1.25).floor() as u32;
    expect_pixel(&capture, x, y, [255; 4])?;
    editor.ime(&Ime::Enabled)?;
    editor.ime(&Ime::Preedit("é العربية".into(), Some((0, 0))))?;
    editor.prepare(renderer, 1.25)?;
    let range = editor.input.buffer.preedit_range().unwrap();
    let underline = editor
        .paragraph
        .selection_rects(Selection::new(range.start, range.end))?[0];
    let underline = editor.rect(TextRect {
        y: underline.y + underline.height - 2.0,
        height: 1.0,
        ..underline
    });
    let capture = capture_editor(&editor, renderer)?;
    expect_pixel(
        &capture,
        ((underline.origin.x + 4.0) * 1.25).floor() as u32,
        (underline.origin.y * 1.25 - 0.5).ceil() as u32,
        [149, 218, 255, 255],
    )?;
    editor.ime(&Ime::Preedit("".into(), None))?;
    editor.ime(&Ime::Commit("é".into()))?;
    if editor.input.buffer.text() != "     é" {
        return Err("IME clear/commit lost replacement text".into());
    }
    editor.modifiers(command);
    editor.key(&Key::Character("z".into()), None, true, false)?;
    if editor.input.buffer.text() != "     " {
        return Err("IME commit did not undo as one edit".into());
    }
    editor.modifiers(ModifiersState::empty());
    editor.edit(|input| {
        input.buffer.select_all()?;
        input.buffer.insert(&"one\n".repeat(10))
    })?;
    editor.prepare(renderer, 1.25)?;
    let area = editor.ime_area().ok_or("missing IME candidate area")?;
    if editor.scroll_y <= 0.0 || area.origin.y < 280.0 || area.origin.y + area.size.height > 348.0 {
        return Err("caret scrolling/candidate area escaped the field".into());
    }
    editor.ime(&Ime::Preedit("cancel".into(), None))?;
    let committed = editor.input.buffer.text().to_owned();
    editor.focus(false);
    editor.ime(&Ime::Commit("late".into()))?;
    editor.prepare(renderer, 1.25)?;
    if editor.input.buffer.text() != committed || editor.input.buffer.preedit().is_some() {
        return Err("focus loss committed stale IME text".into());
    }
    capture_editor(&editor, renderer)?;
    renderer.remove_image(editor.image.unwrap())?;
    renderer.wait_idle()?;
    if renderer.image_stats().live != live || renderer.image_stats().retired != 0 {
        return Err("editor image replacement leaked textures".into());
    }
    println!(
        "GPU editor checks passed: selection/caret/preedit pixels, no upload on selection, IME commit/undo, caret scrolling, focus cancellation and texture retirement"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paragraph_rejection_preserves_document_selection_history_and_snapshot() {
        let mut editor = TextDemo::new().unwrap();
        editor.focus(true);
        editor.edit(|input| input.buffer.insert("!")).unwrap();
        let before = editor.input.buffer.text().to_owned();
        let selection = editor.input.buffer.selection();
        let history = editor.input.buffer.history_stats();
        assert!(
            editor
                .edit(|input| input.buffer.insert(&"\n".repeat(5000)))
                .is_err()
        );
        assert_eq!(editor.input.buffer.text(), before);
        assert_eq!(editor.paragraph.text(), before);
        assert_eq!(editor.input.buffer.selection(), selection);
        assert_eq!(editor.input.buffer.history_stats(), history);
        editor.edit(|input| input.buffer.undo()).unwrap();
        assert_eq!(editor.input.buffer.text(), SAMPLE);
    }
}
