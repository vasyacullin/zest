use super::clipboard::{
    ClipboardAction, ClipboardError, MAX_PASTE_BYTES, TextClipboard, paste_text,
};
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
        raster::{
            LayoutSelection, Paragraph, RasterOptions, Rasterizer, SelectionGranularity, TextRect,
        },
    },
};

mod clicks;
mod page_checks;
mod pointer_checks;

const FAMILIES: &[&str] = &["Noto Sans", "Noto Sans Arabic"];
const SAMPLE: &str = "Zest • Привет, мир!\noffice e\u{301} • العربية";
pub(super) const FIELD: Rect<Logical> = Rect::from_xywh(288.0, 272.0, 344.0, 84.0);
const VIEW_WIDTH: f64 = 320.0;
const VIEW_HEIGHT: f64 = 68.0;
const VIEW: Rect<Logical> = Rect::from_xywh(300.0, 280.0, VIEW_WIDTH, VIEW_HEIGHT);

#[derive(Clone)]
struct Drag {
    anchor: LayoutSelection,
    pointer: Point<Logical>,
    granularity: SelectionGranularity,
}

struct StagedInput {
    input: TextInput,
    paragraph: Option<Paragraph>,
    follow_caret: bool,
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
    navigation: Option<LayoutSelection>,
    clicks: clicks::ClickSequence,
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
            navigation: None,
            clicks: clicks::ClickSequence::default(),
        })
    }
    pub fn reset(&mut self) {
        self.image = None;
        self.uploaded = false;
        self.input.focus(false);
        self.cancel_pointer();
        self.follow_caret = true;
        self.navigation = None;
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
            self.navigation = None;
            self.drag = None;
            self.clicks.reset();
        }
        Ok(())
    }
    // Stage model/history and layout before accepting an edit. A paragraph
    // budget error retains the last accepted document and its undo history.
    fn edit<T>(
        &mut self,
        action: impl FnOnce(&mut TextInput) -> Result<T, zest::text::EditError>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let (staged, result) = self.stage_edit(action)?;
        self.accept_edit(staged);
        Ok(result)
    }
    fn stage_edit<T>(
        &mut self,
        action: impl FnOnce(&mut TextInput) -> Result<T, zest::text::EditError>,
    ) -> Result<(StagedInput, T), Box<dyn std::error::Error>> {
        let mut next = self.input.clone();
        let result = action(&mut next)?;
        let display = next.buffer.display_text();
        let follow_caret = self.input.buffer.text() != next.buffer.text()
            || self.input.buffer.display_selection() != next.buffer.display_selection()
            || self.input.buffer.preedit_range() != next.buffer.preedit_range()
            || self.paragraph.text() != display;
        let paragraph = if self.paragraph.text() != display {
            Some(self.raster.paragraph(
                &display,
                FAMILIES,
                Self::options(self.paragraph.bitmap().scale),
            )?)
        } else {
            None
        };
        Ok((
            StagedInput {
                input: next,
                paragraph,
                follow_caret,
            },
            result,
        ))
    }
    fn accept_edit(&mut self, staged: StagedInput) {
        if let Some(paragraph) = staged.paragraph {
            self.paragraph = paragraph;
            self.uploaded = false;
            self.drag = None;
            self.clicks.reset();
        }
        self.input = staged.input;
        if staged.follow_caret {
            self.navigation = None;
        }
        self.follow_caret |= staged.follow_caret;
    }
    /// Copy is observational. Cut validates the staged layout before writing and
    /// commits the deletion only after a successful write. Paste is one edit.
    pub fn clipboard(
        &mut self,
        action: ClipboardAction,
        clipboard: &mut impl TextClipboard,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        if !self.input.focused || self.input.buffer.preedit().is_some() {
            return Ok(false);
        }
        match action {
            ClipboardAction::Copy | ClipboardAction::Cut => {
                let text = self.input.buffer.selected_text().to_owned();
                if text.is_empty() {
                    return Ok(false);
                }
                if text.contains('\0') {
                    return Err(ClipboardError(
                        "selected text contains NUL and cannot be copied losslessly".into(),
                    )
                    .into());
                }
                if action == ClipboardAction::Copy {
                    clipboard.set_text(&text)?;
                    return Ok(false);
                }
                let (staged, changed) = self.stage_edit(|input| input.buffer.insert(""))?;
                clipboard.set_text(&text)?;
                self.accept_edit(staged);
                self.end_drag();
                Ok(changed)
            }
            ClipboardAction::Paste => {
                let text = paste_text(&clipboard.get_text()?)?;
                if text.is_empty() {
                    return Ok(false);
                }
                let retained =
                    self.input.buffer.text().len() - self.input.buffer.selected_text().len();
                if text.len() > MAX_PASTE_BYTES.saturating_sub(retained) {
                    return Err(ClipboardError(
                        "paste would exceed the 64 KiB document limit".into(),
                    )
                    .into());
                }
                let changed = self.edit(|input| input.buffer.insert(&text))?;
                if changed {
                    self.end_drag();
                }
                Ok(changed)
            }
        }
    }
    pub fn key(
        &mut self,
        key: &Key,
        text: Option<&str>,
        pressed: bool,
        synthetic: bool,
    ) -> Result<KeyResult, Box<dyn std::error::Error>> {
        if pressed && !synthetic {
            self.clicks.reset();
        }
        let result = self.edit(|input| input.key(key, text, pressed, synthetic))?;
        self.finish_key(result)
    }
    pub fn key_physical(
        &mut self,
        key: &Key,
        physical: PhysicalKey,
        text: Option<&str>,
        pressed: bool,
        synthetic: bool,
    ) -> Result<KeyResult, Box<dyn std::error::Error>> {
        if pressed && !synthetic {
            self.clicks.reset();
        }
        let result =
            self.edit(|input| input.key_physical(key, physical, text, pressed, synthetic))?;
        self.finish_key(result)
    }
    fn finish_key(
        &mut self,
        mut result: KeyResult,
    ) -> Result<KeyResult, Box<dyn std::error::Error>> {
        if result.navigation.is_some() || result.page.is_some() {
            let current = self
                .current_navigation()
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| {
                    self.paragraph
                        .layout_selection(self.input.buffer.selection())
                })?;
            let before = (self.scroll_x, self.scroll_y);
            let next = if let Some(direction) = result.page {
                let next = self.paragraph.move_selection_page(
                    &current,
                    direction,
                    VIEW_HEIGHT,
                    self.input.modifiers.shift_key(),
                )?;
                // Keep the caret's viewport-relative height while paging, then
                // clamp/reveal it at document edges or after manual scrolling.
                let delta = self.paragraph.selection_caret(&next)?.y
                    - self.paragraph.selection_caret(&current)?.y;
                self.scroll_by(0.0, delta);
                next
            } else {
                self.paragraph.move_selection(
                    &current,
                    result.navigation.unwrap(),
                    self.input.modifiers.shift_key(),
                )?
            };
            result.changed |= !next.same_position(&current);
            self.input.buffer.set_selection(next.selection())?;
            self.navigation = Some(next);
            self.follow_caret = true;
            self.update_view()?;
            result.changed |= before != (self.scroll_x, self.scroll_y);
            self.end_drag();
            return Ok(result);
        }
        // A no-op deletion still reveals an offscreen caret after manual
        // scrolling. Layout navigation (including no-ops) is handled above.
        if !result.changed && result.reveal_caret {
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
        // Toggling native IME after a pointer press must not break a multi-click
        // sequence. Actual composition/editing does interrupt it.
        if self.input.buffer.preedit().is_some()
            || matches!(ime, Ime::Commit(_))
            || matches!(ime, Ime::Preedit(text, _) if !text.is_empty())
        {
            self.clicks.reset();
        }
        let changed = self.edit(|input| input.ime(ime))?;
        if matches!(ime, Ime::Preedit(text, _) if !text.is_empty()) || matches!(ime, Ime::Commit(_))
        {
            self.end_drag();
        }
        Ok(changed)
    }
    pub fn focus(&mut self, focused: bool) -> bool {
        self.clicks.reset();
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
        self.clicks.reset();
        let changed = self.input.cancel_composition();
        self.follow_caret |= changed;
        if changed {
            self.navigation = None;
        }
        self.end_drag();
    }
    pub fn begin_drag(
        &mut self,
        point: Point<Logical>,
        extend: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.clicks.reset();
        self.begin_drag_granularity(point, extend, SelectionGranularity::Grapheme)
    }
    pub fn pointer_press(
        &mut self,
        point: Point<Logical>,
        extend: bool,
        now: Instant,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(zest::text::raster::RasterError::InvalidOptions.into());
        }
        let mut clicks = self.clicks.clone();
        if self.input.buffer.preedit().is_some() {
            clicks.reset();
        }
        let granularity = clicks.press(point, now, extend);
        let result = self.begin_drag_granularity(point, extend, granularity);
        if result.is_ok() {
            self.clicks = clicks;
        } else {
            self.clicks.reset();
        }
        result
    }
    fn begin_drag_granularity(
        &mut self,
        point: Point<Logical>,
        extend: bool,
        granularity: SelectionGranularity,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(zest::text::raster::RasterError::InvalidOptions.into());
        }
        self.input.cancel_composition();
        self.input.focus(true);
        self.refresh(self.paragraph.bitmap().scale)?;
        self.clamp_scroll();
        let anchor = if extend {
            Some(
                self.current_navigation()
                    .cloned()
                    .map(Ok)
                    .unwrap_or_else(|| {
                        self.paragraph
                            .layout_selection(self.input.buffer.selection())
                    })?,
            )
        } else {
            None
        };
        let selection = self.pointer_selection(point, anchor.as_ref(), granularity)?;
        self.input.buffer.set_selection(selection.selection())?;
        self.navigation = Some(selection.clone());
        self.drag = Some(Drag {
            anchor: selection,
            pointer: point,
            granularity,
        });
        self.follow_caret = false;
        Ok(())
    }
    fn pointer_selection(
        &self,
        point: Point<Logical>,
        anchor: Option<&LayoutSelection>,
        granularity: SelectionGranularity,
    ) -> Result<LayoutSelection, zest::text::raster::RasterError> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(zest::text::raster::RasterError::InvalidOptions);
        }
        let x = point
            .x
            .clamp(VIEW.origin.x, VIEW.origin.x + VIEW_WIDTH - 0.5)
            - VIEW.origin.x
            + self.scroll_x;
        let y = point
            .y
            .clamp(VIEW.origin.y, VIEW.origin.y + VIEW_HEIGHT - 0.5)
            - VIEW.origin.y
            + self.scroll_y;
        match anchor {
            Some(anchor) => self
                .paragraph
                .extend_selection_granularity(anchor, x, y, granularity),
            None => self.paragraph.selection_at_granularity(x, y, granularity),
        }
    }
    pub fn drag_to(&mut self, point: Point<Logical>) -> Result<bool, Box<dyn std::error::Error>> {
        if point.x.is_finite() && point.y.is_finite() {
            self.clicks.moved(point);
        }
        let Some(drag) = self.drag.clone() else {
            return Ok(false);
        };
        let selection = self.pointer_selection(point, Some(&drag.anchor), drag.granularity)?;
        let visual_changed = self
            .current_navigation()
            .is_none_or(|old| !old.same_position(&selection));
        let changed = self.input.buffer.set_selection(selection.selection())?;
        self.navigation = Some(selection);
        self.drag = Some(Drag {
            pointer: point,
            ..drag
        });
        self.follow_caret = false;
        Ok(changed || visual_changed)
    }
    pub fn end_drag(&mut self) {
        self.drag = None;
    }
    pub fn cancel_pointer(&mut self) {
        self.end_drag();
        self.clicks.reset();
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
        self.clicks.reset();
        if self.input.modifiers.shift_key() && x == 0.0 {
            x = y;
            y = 0.0;
        }
        self.follow_caret = false;
        let scrolled = self.scroll_by(-x, -y);
        let selected = if let Some(drag) = self.drag.clone() {
            self.drag_to(drag.pointer)?
        } else {
            false
        };
        Ok(scrolled || selected)
    }
    fn drag_velocity(&self) -> (f64, f64) {
        let Some(drag) = &self.drag else {
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
        let selected = if let Some(drag) = self.drag.clone() {
            self.drag_to(drag.pointer)?
        } else {
            false
        };
        Ok(scrolled || selected)
    }
    fn current_navigation(&self) -> Option<&LayoutSelection> {
        self.navigation.as_ref().filter(|state| {
            self.input.buffer.preedit().is_none()
                && state.selection() == self.input.buffer.selection()
        })
    }
    fn current_caret(&self) -> Result<TextRect, zest::text::raster::RasterError> {
        match self.current_navigation() {
            Some(state) => self.paragraph.selection_caret(state),
            None => self.paragraph.caret(self.display_head()),
        }
    }
    fn selection_rects(
        &self,
        selection: Selection,
    ) -> Result<Vec<TextRect>, zest::text::raster::RasterError> {
        match self.current_navigation() {
            Some(state) => self.paragraph.selection_geometry(state),
            None => self.paragraph.selection_rects(selection),
        }
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
            let caret = self.current_caret()?;
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
        let caret = self.current_caret().ok()?;
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
                        bounds: self.rect(self.current_caret().expect("validated display caret")),
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
    editor.edit(|input| {
        input.buffer.select_all()?;
        input.buffer.insert("abcdefghij\nx\nabcdefghij")?;
        input.buffer.set_selection(Selection::caret(8))
    })?;
    editor.prepare(renderer, 1.25)?;
    let image = editor.image;
    let staged = renderer.upload_stats().staged_bytes;
    let history = editor.input.buffer.history_stats();
    editor.key(&Key::Named(NamedKey::ArrowDown), None, true, false)?;
    editor.key(&Key::Named(NamedKey::ArrowDown), None, true, false)?;
    if editor.input.buffer.selection().head != 21 {
        return Err("vertical navigation lost its goal column".into());
    }
    editor.prepare(renderer, 1.25)?;
    let capture = capture_editor(&editor, renderer)?;
    let caret = editor.rect(editor.current_caret()?);
    expect_pixel(
        &capture,
        (caret.origin.x * 1.25 - 0.5).ceil() as u32,
        ((caret.origin.y + caret.size.height * 0.5) * 1.25).floor() as u32,
        [255; 4],
    )?;
    editor.modifiers(ModifiersState::SHIFT);
    editor.key(&Key::Named(NamedKey::ArrowUp), None, true, false)?;
    editor.key(&Key::Named(NamedKey::ArrowUp), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    capture_editor(&editor, renderer)?;
    if editor.input.buffer.selection() != Selection::new(21, 8)
        || editor.input.buffer.history_stats() != history
        || editor.image != image
        || renderer.upload_stats().staged_bytes != staged
    {
        return Err("vertical selection changed history or uploaded a new text image".into());
    }
    editor.modifiers(ModifiersState::empty());
    editor.edit(|input| {
        input.buffer.select_all()?;
        input.buffer.insert(&"one two three four five ".repeat(4))?;
        input.buffer.set_selection(Selection::caret(0))
    })?;
    editor.prepare(renderer, 1.25)?;
    let image = editor.image;
    let staged = renderer.upload_stats().staged_bytes;
    editor.key(&Key::Named(NamedKey::End), None, true, false)?;
    let offset = editor.input.buffer.selection().head;
    let line_y = editor.current_caret()?.y;
    let moved = editor.key(&Key::Named(NamedKey::ArrowRight), None, true, false)?;
    if !moved.changed
        || editor.input.buffer.selection().head != offset
        || editor.current_caret()?.y <= line_y
    {
        return Err("wrap-affinity transition did not move the visible caret".into());
    }
    editor.key(&Key::Named(NamedKey::ArrowLeft), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    let capture = capture_editor(&editor, renderer)?;
    let caret = editor.rect(editor.current_caret()?);
    expect_pixel(
        &capture,
        (caret.origin.x * 1.25 - 0.5).ceil() as u32,
        ((caret.origin.y + caret.size.height * 0.5) * 1.25).floor() as u32,
        [255; 4],
    )?;
    if editor.current_caret()?.y != line_y
        || editor.image != image
        || renderer.upload_stats().staged_bytes != staged
    {
        return Err(
            "affinity-only movement rebuilt the bitmap or used the wrong visual line".into(),
        );
    }
    let mut clipboard = super::clipboard::MemoryClipboard {
        text: Some("é\r\nالعربية e\u{301}".into()),
        ..Default::default()
    };
    editor.modifiers(command);
    editor.key(&Key::Character("a".into()), None, true, false)?;
    let paste = editor.key(&Key::Character("v".into()), None, true, false)?;
    if paste.clipboard != Some(ClipboardAction::Paste)
        || !editor.clipboard(paste.clipboard.unwrap(), &mut clipboard)?
        || editor.input.buffer.text() != "é\nالعربية e\u{301}"
    {
        return Err("clipboard shortcut/paste lost Unicode or line endings".into());
    }
    editor.prepare(renderer, 1.25)?;
    capture_editor(&editor, renderer)?;
    editor.key(&Key::Character("a".into()), None, true, false)?;
    let image = editor.image;
    let staged = renderer.upload_stats().staged_bytes;
    let history = editor.input.buffer.history_stats();
    let selection = editor.input.buffer.selection();
    let copy = editor.key(&Key::Character("c".into()), None, true, false)?;
    if copy.clipboard != Some(ClipboardAction::Copy)
        || editor.clipboard(copy.clipboard.unwrap(), &mut clipboard)?
        || clipboard.text.as_deref() != Some("é\nالعربية e\u{301}")
    {
        return Err("clipboard copy changed the model or lost selected text".into());
    }
    let cut = editor.key(&Key::Character("x".into()), None, true, false)?;
    if cut.clipboard != Some(ClipboardAction::Cut) {
        return Err("clipboard cut shortcut was not routed".into());
    }
    clipboard.fail_write = true;
    if editor
        .clipboard(cut.clipboard.unwrap(), &mut clipboard)
        .is_ok()
    {
        return Err("clipboard write failure was accepted".into());
    }
    editor.prepare(renderer, 1.25)?;
    if editor.input.buffer.text() != "é\nالعربية e\u{301}"
        || editor.input.buffer.selection() != selection
        || editor.input.buffer.history_stats() != history
        || editor.image != image
        || renderer.upload_stats().staged_bytes != staged
    {
        return Err("copy or failed cut changed history, selection or GPU image".into());
    }
    clipboard.fail_write = false;
    if !editor.clipboard(ClipboardAction::Cut, &mut clipboard)?
        || !editor.input.buffer.text().is_empty()
    {
        return Err("successful clipboard cut did not delete the selection".into());
    }
    editor.prepare(renderer, 1.25)?;
    let capture = capture_editor(&editor, renderer)?;
    let caret = editor.rect(editor.current_caret()?);
    expect_pixel(
        &capture,
        (caret.origin.x * 1.25 - 0.5).ceil() as u32,
        ((caret.origin.y + caret.size.height * 0.5) * 1.25).floor() as u32,
        [255; 4],
    )?;
    editor.key(&Key::Character("z".into()), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    capture_editor(&editor, renderer)?;
    if editor.input.buffer.text() != "é\nالعربية e\u{301}"
        || editor.input.buffer.selection() != selection
    {
        return Err("clipboard cut did not undo as one edit".into());
    }
    let words = "one e\u{301} two\nواحد اثنان";
    editor.edit(|input| {
        input.buffer.select_all()?;
        input.buffer.insert(words)?;
        input.buffer.set_selection(Selection::caret(0))
    })?;
    editor.prepare(renderer, 1.25)?;
    let image = editor.image;
    let staged = renderer.upload_stats().staged_bytes;
    let history = editor.input.buffer.history_stats();
    let word_modifier = if cfg!(target_os = "macos") {
        ModifiersState::ALT
    } else {
        ModifiersState::CONTROL
    };
    editor.modifiers(word_modifier | ModifiersState::SHIFT);
    editor.key(&Key::Named(NamedKey::ArrowRight), None, true, false)?;
    if editor.input.buffer.selection() != Selection::new(0, 3) {
        return Err("word shortcut did not select the first word".into());
    }
    editor.prepare(renderer, 1.25)?;
    let capture = capture_editor(&editor, renderer)?;
    let selected = editor
        .paragraph
        .selection_geometry(editor.current_navigation().unwrap())?[0];
    let selected = editor.rect(selected);
    expect_pixel(
        &capture,
        ((selected.origin.x + selected.size.width * 0.5) * 1.25).floor() as u32,
        ((selected.origin.y + selected.size.height - 1.5) * 1.25).floor() as u32,
        [63, 124, 179, 255],
    )?;
    editor.modifiers(word_modifier);
    editor.key(&Key::Named(NamedKey::ArrowRight), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    if editor.input.buffer.selection() != Selection::caret(3)
        || editor.input.buffer.history_stats() != history
        || editor.image != image
        || renderer.upload_stats().staged_bytes != staged
    {
        return Err("word selection/collapse changed history or uploaded text".into());
    }
    editor.key(&Key::Named(NamedKey::Backspace), None, true, false)?;
    if editor.input.buffer.text() != " e\u{301} two\nواحد اثنان"
        || editor.input.buffer.history_stats().undo_edits != history.undo_edits + 1
    {
        return Err("word deletion did not produce one history edit".into());
    }
    editor.prepare(renderer, 1.25)?;
    capture_editor(&editor, renderer)?;
    editor.modifiers(command);
    editor.key(&Key::Character("z".into()), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    if editor.input.buffer.text() != words || editor.input.buffer.selection() != Selection::caret(3)
    {
        return Err("word deletion undo lost text or original caret".into());
    }
    let rtl_start = "one e\u{301} two\n".len();
    editor.edit(|input| input.buffer.set_selection(Selection::caret(rtl_start)))?;
    let before = editor.current_caret()?;
    let image = editor.image;
    let staged = renderer.upload_stats().staged_bytes;
    editor.modifiers(word_modifier);
    editor.key(&Key::Named(NamedKey::ArrowLeft), None, true, false)?;
    editor.prepare(renderer, 1.25)?;
    let capture = capture_editor(&editor, renderer)?;
    let caret = editor.current_caret()?;
    if editor.input.buffer.selection().head <= rtl_start
        || caret.x >= before.x
        || editor.image != image
        || renderer.upload_stats().staged_bytes != staged
    {
        return Err("RTL word-left lost visual direction or uploaded text".into());
    }
    let caret = editor.rect(caret);
    expect_pixel(
        &capture,
        (caret.origin.x * 1.25 - 0.5).ceil() as u32,
        ((caret.origin.y + caret.size.height * 0.5) * 1.25).floor() as u32,
        [255; 4],
    )?;
    page_checks::check(&mut editor, renderer)?;
    pointer_checks::check(&mut editor, renderer)?;
    renderer.remove_image(editor.image.unwrap())?;
    renderer.wait_idle()?;
    if renderer.image_stats().live != live || renderer.image_stats().retired != 0 {
        return Err("editor image replacement leaked textures".into());
    }
    println!(
        "GPU editor checks passed: selection/caret/preedit pixels, drag/wheel/visual word navigation without texture uploads, word deletion/undo, vertical goal column, wrap affinity, IME undo, memory clipboard copy/cut/paste and write failure, scrolling and texture retirement"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
