use super::text_input::{KeyResult, TextInput};
use super::*;
use winit::{
    event::{Ime, MouseScrollDelta},
    keyboard::{Key, ModifiersState, NamedKey, PhysicalKey},
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
const VIEW: Rect<Logical> = Rect::from_xywh(300.0, 280.0, VIEW_WIDTH, VIEW_HEIGHT);

#[derive(Clone, Copy)]
struct Drag {
    anchor: usize,
    pointer: Point<Logical>,
}

pub(super) struct TextDemo {
    raster: Rasterizer,
    image: Option<ImageId>,
    paragraph: Paragraph,
    uploaded: bool,
    pub input: TextInput,
    scroll_x: f64,
    scroll_y: f64,
    follow_caret: bool,
    drag: Option<Drag>,
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
            follow_caret: true,
            drag: None,
        })
    }
    pub fn reset(&mut self) {
        self.image = None;
        self.uploaded = false;
        self.input.focus(false);
        self.end_drag();
        self.follow_caret = true;
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
        let follow_caret = self.input.buffer.text() != next.buffer.text()
            || self.input.buffer.display_selection() != next.buffer.display_selection()
            || self.input.buffer.preedit_range() != next.buffer.preedit_range()
            || self.paragraph.text() != display;
        if self.paragraph.text() != display {
            self.paragraph = self.raster.paragraph(
                &display,
                FAMILIES,
                Self::options(self.paragraph.bitmap().scale),
            )?;
            self.uploaded = false;
        }
        self.input = next;
        self.follow_caret |= follow_caret;
        Ok(result)
    }
    pub fn key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        pressed: bool,
        synthetic: bool,
    ) -> Result<KeyResult, Box<dyn std::error::Error>> {
        let result = self.edit(|input| input.key(key, text, pressed, synthetic))?;
        self.finish_key(key, pressed, synthetic, result)
    }
    pub fn key_physical(
        &mut self,
        key: &Key,
        physical: PhysicalKey,
        text: Option<&str>,
        pressed: bool,
        synthetic: bool,
    ) -> Result<KeyResult, Box<dyn std::error::Error>> {
        let result =
            self.edit(|input| input.key_physical(key, physical, text, pressed, synthetic))?;
        self.finish_key(key, pressed, synthetic, result)
    }
    fn finish_key(
        &mut self,
        key: &Key,
        pressed: bool,
        synthetic: bool,
        mut result: KeyResult,
    ) -> Result<KeyResult, Box<dyn std::error::Error>> {
        // End at the document end (or Backspace at its start) still needs to
        // reveal an offscreen caret after manual scrolling, without a text edit.
        let navigation = matches!(
            key,
            Key::Named(
                NamedKey::Home
                    | NamedKey::End
                    | NamedKey::ArrowLeft
                    | NamedKey::ArrowRight
                    | NamedKey::Backspace
                    | NamedKey::Delete
            )
        );
        let blocked = self.input.modifiers.super_key()
            || (self.input.modifiers.control_key() && !self.input.modifiers.alt_key());
        if !result.changed
            && navigation
            && pressed
            && !synthetic
            && !blocked
            && self.input.focused
            && self.input.buffer.preedit().is_none()
        {
            let before = (self.scroll_x, self.scroll_y);
            self.follow_caret = true;
            self.update_view()?;
            result.changed = before != (self.scroll_x, self.scroll_y);
        }
        if result.changed || result.close {
            self.end_drag();
        }
        Ok(result)
    }
    pub fn ime(&mut self, ime: &Ime) -> Result<bool, Box<dyn std::error::Error>> {
        let changed = self.edit(|input| input.ime(ime))?;
        if matches!(ime, Ime::Preedit(text, _) if !text.is_empty()) || matches!(ime, Ime::Commit(_))
        {
            self.end_drag();
        }
        Ok(changed)
    }
    pub fn focus(&mut self, focused: bool) -> bool {
        let composing = self.input.buffer.preedit().is_some();
        let changed = self.input.focus(focused);
        if !focused {
            self.end_drag();
        }
        if (focused && changed) || composing {
            self.follow_caret = true;
        }
        changed
    }
    pub fn modifiers(&mut self, modifiers: ModifiersState) {
        self.input.modifiers = modifiers;
    }
    pub fn cancel_composition(&mut self) {
        self.follow_caret |= self.input.cancel_composition();
        self.end_drag();
    }
    pub fn begin_drag(
        &mut self,
        point: Point<Logical>,
        extend: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(zest::text::raster::RasterError::InvalidOptions.into());
        }
        self.input.cancel_composition();
        self.input.focus(true);
        self.refresh(self.paragraph.bitmap().scale)?;
        self.clamp_scroll();
        let offset = self.pointer_offset(point)?;
        self.input.buffer.set_selection(Selection::new(
            if extend {
                self.input.buffer.selection().anchor
            } else {
                offset
            },
            offset,
        ))?;
        self.drag = Some(Drag {
            anchor: self.input.buffer.selection().anchor,
            pointer: point,
        });
        self.follow_caret = false;
        Ok(())
    }
    fn pointer_offset(
        &self,
        point: Point<Logical>,
    ) -> Result<usize, zest::text::raster::RasterError> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(zest::text::raster::RasterError::InvalidOptions);
        }
        self.paragraph.hit_test(
            point
                .x
                .clamp(VIEW.origin.x, VIEW.origin.x + VIEW_WIDTH - 0.5)
                - VIEW.origin.x
                + self.scroll_x,
            point
                .y
                .clamp(VIEW.origin.y, VIEW.origin.y + VIEW_HEIGHT - 0.5)
                - VIEW.origin.y
                + self.scroll_y,
        )
    }
    pub fn drag_to(&mut self, point: Point<Logical>) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(drag) = self.drag else {
            return Ok(false);
        };
        let offset = self.pointer_offset(point)?;
        let changed = self
            .input
            .buffer
            .set_selection(Selection::new(drag.anchor, offset))?;
        self.drag = Some(Drag {
            pointer: point,
            ..drag
        });
        self.follow_caret = false;
        Ok(changed)
    }
    pub fn end_drag(&mut self) {
        self.drag = None;
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    fn scroll_limit(&self) -> (f64, f64) {
        let (width, height) = self.paragraph.content_size();
        (
            (width - VIEW_WIDTH).max(0.0),
            (height - VIEW_HEIGHT).max(0.0),
        )
    }
    fn clamp_scroll(&mut self) {
        let (x, y) = self.scroll_limit();
        self.scroll_x = self.scroll_x.clamp(0.0, x);
        self.scroll_y = self.scroll_y.clamp(0.0, y);
    }
    fn scroll_by(&mut self, dx: f64, dy: f64) -> bool {
        let before = (self.scroll_x, self.scroll_y);
        self.scroll_x += dx;
        self.scroll_y += dy;
        self.clamp_scroll();
        before != (self.scroll_x, self.scroll_y)
    }
    /// Wheel deltas describe movement of the content, opposite the scroll offset.
    /// Line deltas use 36 logical pixels; touchpad deltas use physical pixels.
    pub fn wheel(
        &mut self,
        delta: MouseScrollDelta,
        scale: ScaleFactor,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let (mut x, mut y) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (f64::from(x) * 36.0, f64::from(y) * 36.0),
            MouseScrollDelta::PixelDelta(point) => (point.x / scale.get(), point.y / scale.get()),
        };
        if !x.is_finite() || !y.is_finite() || (x == 0.0 && y == 0.0) {
            return Ok(false);
        }
        if self.input.modifiers.shift_key() && x == 0.0 {
            x = y;
            y = 0.0;
        }
        self.follow_caret = false;
        let scrolled = self.scroll_by(-x, -y);
        let selected = if let Some(drag) = self.drag {
            self.drag_to(drag.pointer)?
        } else {
            false
        };
        Ok(scrolled || selected)
    }
    fn drag_velocity(&self) -> (f64, f64) {
        let Some(drag) = self.drag else {
            return (0.0, 0.0);
        };
        let axis = |point: f64, start: f64, end: f64| {
            if point < start {
                ((point - start) * 12.0).max(-720.0)
            } else if point >= end {
                ((point - end + 1.0) * 12.0).min(720.0)
            } else {
                0.0
            }
        };
        let (max_x, max_y) = self.scroll_limit();
        let x = axis(drag.pointer.x, VIEW.origin.x, VIEW.origin.x + VIEW_WIDTH);
        let y = axis(drag.pointer.y, VIEW.origin.y, VIEW.origin.y + VIEW_HEIGHT);
        let movable = |velocity: f64, offset: f64, max: f64| {
            if (velocity < 0.0 && offset > 0.0) || (velocity > 0.0 && offset < max) {
                velocity
            } else {
                0.0
            }
        };
        (
            movable(x, self.scroll_x, max_x),
            movable(y, self.scroll_y, max_y),
        )
    }
    pub fn autoscroll_active(&self) -> bool {
        self.drag_velocity() != (0.0, 0.0)
    }
    /// Deterministic elapsed-time step, bounded after stalls. The event loop
    /// schedules ticks only while a drag can scroll beyond a viewport edge.
    pub fn drag_scroll(&mut self, seconds: f64) -> Result<bool, Box<dyn std::error::Error>> {
        if !seconds.is_finite() || seconds <= 0.0 {
            return Ok(false);
        }
        let (x, y) = self.drag_velocity();
        let scrolled = self.scroll_by(x * seconds.min(0.1), y * seconds.min(0.1));
        let selected = if let Some(drag) = self.drag {
            self.drag_to(drag.pointer)?
        } else {
            false
        };
        Ok(scrolled || selected)
    }
    fn display_head(&self) -> usize {
        self.input
            .buffer
            .display_selection()
            .map(|s| s.head)
            .unwrap_or_else(|| {
                self.input
                    .buffer
                    .preedit_range()
                    .map_or(self.input.buffer.selection().head, |r| r.end)
            })
    }
    fn update_view(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.clamp_scroll();
        if std::mem::take(&mut self.follow_caret) {
            let caret = self.paragraph.caret(self.display_head())?;
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
            self.clamp_scroll();
        }
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
        self.update_view()
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
        let caret = self.paragraph.caret(self.display_head()).ok()?;
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
    editor.focus(true);
    editor.edit(|input| {
        input.buffer.select_all()?;
        input.buffer.insert(&"     \n".repeat(10))
    })?;
    editor.prepare(renderer, 1.25)?;
    let image = editor.image;
    let staged = renderer.upload_stats().staged_bytes;
    let history = editor.input.buffer.history_stats();
    editor.wheel(
        MouseScrollDelta::LineDelta(0.0, 1e6),
        ScaleFactor::new(1.25)?,
    )?;
    editor.prepare(renderer, 1.25)?;
    if editor.scroll_y != 0.0 {
        return Err("manual scroll snapped back to the caret on redraw".into());
    }
    let caret = editor.paragraph.caret(0)?;
    editor.begin_drag(Point::new(300.1, 280.0 + caret.height * 0.5), false)?;
    editor.drag_to(Point::new(310.0, 355.0))?;
    for _ in 0..10 {
        editor.drag_scroll(0.05)?;
    }
    editor.end_drag();
    let selection = editor.input.buffer.selection();
    if selection.anchor != 0
        || selection.is_caret()
        || editor.scroll_y <= 0.0
        || editor.autoscroll_active()
    {
        return Err(
            "drag selection/autoscroll did not retain its anchor or stop on release".into(),
        );
    }
    editor.prepare(renderer, 1.25)?;
    let capture = capture_editor(&editor, renderer)?;
    let selected = editor
        .paragraph
        .selection_rects(selection)?
        .into_iter()
        .map(|rect| editor.rect(rect))
        .filter_map(|rect| rect.intersection(&VIEW))
        .find(|rect| rect.size.width > 2.0 && rect.size.height > 2.0)
        .ok_or("drag selection has no visible highlight")?;
    expect_pixel(
        &capture,
        ((selected.origin.x + selected.size.width * 0.5) * 1.25).floor() as u32,
        ((selected.origin.y + selected.size.height * 0.5) * 1.25).floor() as u32,
        [63, 124, 179, 255],
    )?;
    expect_pixel(&capture, 370, 355, [33, 44, 56, 255])?; // padding outside the clipped viewport
    let before = editor.scroll_y;
    editor.wheel(
        MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, 20.0)),
        ScaleFactor::new(1.25)?,
    )?;
    editor.prepare(renderer, 1.25)?;
    capture_editor(&editor, renderer)?;
    if (editor.scroll_y - (before - 16.0).max(0.0)).abs() > 1e-9
        || editor.input.buffer.selection() != selection
        || editor.input.buffer.history_stats() != history
        || editor.image != image
        || renderer.upload_stats().staged_bytes != staged
    {
        return Err("wheel/drag changed model history, uploaded text again or lost fractional-DPI scrolling".into());
    }
    renderer.remove_image(editor.image.unwrap())?;
    renderer.wait_idle()?;
    if renderer.image_stats().live != live || renderer.image_stats().retired != 0 {
        return Err("editor image replacement leaked textures".into());
    }
    println!(
        "GPU editor checks passed: selection/caret/preedit pixels, drag/wheel at fractional DPI without texture uploads, IME commit/undo, caret/drag scrolling, focus cancellation and texture retirement"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
