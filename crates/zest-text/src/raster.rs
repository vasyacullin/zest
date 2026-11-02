//! Explicit-font paragraph shaping and monochrome rasterization. No system
//! fonts are discovered. The white straight-alpha output can be tinted by an
//! image renderer. This initial path rasterizes a whole paragraph, not an atlas.
use parley::{
    Affinity, Alignment, AlignmentOptions, Cursor, FontContext, FontFamilyName, Layout,
    LayoutContext, PositionedLayoutItem, StyleProperty,
    fontique::{Collection, CollectionOptions},
};
use std::{collections::HashSet, fmt, sync::Arc};
mod navigation;
pub use navigation::{LayoutSelection, Navigation};
use swash::{
    FontRef,
    scale::{Render, ScaleContext, Source, image::Content},
    zeno::{Format, Vector},
};

const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_FONT_BYTES: usize = 16 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
const MAX_PIXELS: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterError {
    InvalidOptions,
    InputTooLarge,
    FontBudgetExceeded,
    InvalidFont,
    UnknownFamily(String),
    LayoutTooLarge,
    InvalidRaster,
    InvalidOffset(usize),
    InvalidGraphemeOffset(usize),
    ForeignSelection,
}
impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions => {
                f.write_str("invalid text size, scale, width, coordinates or font list")
            }
            Self::InputTooLarge => f.write_str("text exceeds the 64 KiB paragraph limit"),
            Self::FontBudgetExceeded => {
                f.write_str("registered fonts exceed the 16 MiB input budget")
            }
            Self::InvalidFont => f.write_str("font has no readable faces"),
            Self::UnknownFamily(name) => write!(f, "font family is not registered: {name:?}"),
            Self::LayoutTooLarge => {
                f.write_str("paragraph exceeds the 4096 dimension / 4 Mi pixel or mask budget")
            }
            Self::InvalidRaster => {
                f.write_str("glyph raster has invalid dimensions or mask format")
            }
            Self::InvalidOffset(offset) => {
                write!(f, "invalid paragraph UTF-8 byte offset: {offset}")
            }
            Self::InvalidGraphemeOffset(offset) => {
                write!(f, "not a paragraph grapheme boundary: {offset}")
            }
            Self::ForeignSelection => {
                f.write_str("selection belongs to a different paragraph snapshot")
            }
        }
    }
}
impl std::error::Error for RasterError {}

/// Logical font size/wrapping width and physical pixels per logical unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterOptions {
    pub font_size: f32,
    pub width: f32,
    pub scale: f32,
}
impl Default for RasterOptions {
    fn default() -> Self {
        Self {
            font_size: 20.0,
            width: 480.0,
            scale: 1.0,
        }
    }
}
impl RasterOptions {
    fn validate(self) -> Result<(), RasterError> {
        if !self.font_size.is_finite()
            || !(0.0..=256.0).contains(&self.font_size)
            || self.font_size == 0.0
            || !self.width.is_finite()
            || self.width <= 0.0
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || self.scale > 8.0
            || self.font_size * self.scale > 512.0
            || self.font_size * self.scale < 1.0
            || self.width * self.scale < 1.0
            || self.width * self.scale > MAX_DIMENSION as f32
        {
            return Err(RasterError::InvalidOptions);
        }
        Ok(())
    }
}

/// CPU layout counters; sizes are logical typographic bounds, not ink bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub lines: usize,
    pub glyphs: usize,
    pub missing_glyphs: usize,
    pub rtl_runs: usize,
    pub fonts: usize,
}

/// Top-to-bottom white straight-alpha sRGB RGBA8. Origin is the bitmap's physical
/// offset from the paragraph origin, including negative glyph overhangs. To
/// paint at logical (x, y), use (x + origin_x/scale, y + origin_y/scale) and size
/// (width/scale, height/scale). Retain/reuse it until content, fonts or DPI change.
#[derive(Clone, Debug, PartialEq)]
pub struct RasterizedText {
    pub width: u32,
    pub height: u32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub scale: f32,
    pub rgba8: Vec<u8>,
    pub metrics: TextMetrics,
}

/// Logical rectangle relative to the paragraph origin (not the bitmap origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Immutable layout and raster snapshot for one display string. Geometry uses
/// the same shaped layout as the bitmap, including wrapping and bidi ordering.
/// Scalar offsets inside a cluster resolve to that cluster's caret geometry.
pub struct Paragraph {
    identity: Arc<()>,
    bitmap: RasterizedText,
    layout: Layout<()>,
    text: String,
}
impl Paragraph {
    pub fn bitmap(&self) -> &RasterizedText {
        &self.bitmap
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn into_bitmap(self) -> RasterizedText {
        self.bitmap
    }
    /// Positive scrolling extents from the paragraph origin, including trailing
    /// whitespace, bitmap ink and line-end carets. Negative ink overhangs remain
    /// available separately through bitmap origin/dimensions.
    pub fn content_size(&self) -> (f64, f64) {
        let scale = f64::from(self.bitmap.scale);
        let caret = self
            .caret(self.text.len())
            .expect("paragraph end is a UTF-8 boundary");
        let right = (f64::from(self.bitmap.origin_x) + f64::from(self.bitmap.width)) / scale;
        let bottom = (f64::from(self.bitmap.origin_y) + f64::from(self.bitmap.height)) / scale;
        // Start-aligned RTL lines can place their logical start at the right
        // wrapping edge. Include a caret there even when the final caret lies
        // at the left edge or on an empty following line.
        let line_right = self
            .layout
            .lines()
            .map(|line| {
                let metrics = line.metrics();
                let range = line.text_range();
                let leading =
                    Cursor::from_byte_index(&self.layout, range.start, Affinity::Downstream)
                        .geometry(&self.layout, self.bitmap.scale);
                let trailing = Cursor::from_byte_index(&self.layout, range.end, Affinity::Upstream)
                    .geometry(&self.layout, self.bitmap.scale);
                (f64::from(metrics.offset + metrics.advance) / scale + 1.0)
                    .max(leading.x1 / scale)
                    .max(trailing.x1 / scale)
            })
            .fold(0.0_f64, f64::max);
        (
            (f64::from(self.layout.full_width()) / scale)
                .max(right)
                .max(line_right)
                .max(caret.x + caret.width),
            (f64::from(self.layout.height()) / scale)
                .max(bottom)
                .max(caret.y + caret.height),
        )
    }
    pub fn caret(&self, offset: usize) -> Result<TextRect, RasterError> {
        self.validate_offset(offset)?;
        let cursor = Cursor::from_byte_index(&self.layout, offset, Affinity::Downstream);
        Ok(self.rect(cursor.geometry(&self.layout, self.bitmap.scale)))
    }
    pub fn selection_rects(
        &self,
        selection: crate::Selection,
    ) -> Result<Vec<TextRect>, RasterError> {
        self.validate_offset(selection.anchor)?;
        self.validate_offset(selection.head)?;
        let range = selection.range();
        let selection = parley::Selection::new(
            Cursor::from_byte_index(&self.layout, range.start, Affinity::Downstream),
            Cursor::from_byte_index(&self.layout, range.end, Affinity::Upstream),
        );
        Ok(selection
            .geometry(&self.layout)
            .into_iter()
            .map(|(rect, _)| self.rect(rect))
            .collect())
    }
    /// Hit tests logical coordinates and snaps forward to an extended grapheme
    /// boundary accepted by TextBuffer. Affinity at bidi/soft-wrap boundaries
    /// is canonicalized; visual-arrow navigation is not part of this API yet.
    pub fn hit_test(&self, x: f64, y: f64) -> Result<usize, RasterError> {
        let scale = f64::from(self.bitmap.scale);
        if !x.is_finite()
            || !y.is_finite()
            || (x * scale).abs() > f64::from(f32::MAX)
            || (y * scale).abs() > f64::from(f32::MAX)
        {
            return Err(RasterError::InvalidOptions);
        }
        let offset =
            Cursor::from_point(&self.layout, (x * scale) as f32, (y * scale) as f32).index();
        Ok(crate::snap_forward(&self.text, offset))
    }
    fn validate_offset(&self, offset: usize) -> Result<(), RasterError> {
        if self.text.is_char_boundary(offset) {
            Ok(())
        } else {
            Err(RasterError::InvalidOffset(offset))
        }
    }
    fn rect(&self, rect: parley::BoundingBox) -> TextRect {
        let scale = f64::from(self.bitmap.scale);
        TextRect {
            x: rect.x0 / scale,
            y: rect.y0 / scale,
            width: (rect.x1 - rect.x0) / scale,
            height: (rect.y1 - rect.y0) / scale,
        }
    }
}

/// Owns the explicitly registered fonts and reusable shaping/raster scratch
/// contexts. Input font bytes are limited to 16 MiB in total; text to 64 KiB per
/// call; output to 4096 per dimension and 4 Mi pixels. Retained temporary glyph
/// masks are separately limited to 4 MiB. These are not total process/cache
/// memory limits. Fonts should come from trusted application assets.
pub struct Rasterizer {
    fonts: FontContext,
    layout: LayoutContext<()>,
    scale: ScaleContext,
    font_bytes: usize,
}
impl Default for Rasterizer {
    fn default() -> Self {
        Self::new()
    }
}
impl Rasterizer {
    pub fn new() -> Self {
        Self {
            fonts: FontContext {
                collection: Collection::new(CollectionOptions {
                    shared: false,
                    system_fonts: false,
                }),
                source_cache: Default::default(),
            },
            layout: LayoutContext::new(),
            scale: ScaleContext::with_max_entries(8),
            font_bytes: 0,
        }
    }
    /// Registers owned TTF/OTF/TTC bytes and returns discovered family names.
    /// The library embeds no fonts; applications retain their font licenses.
    pub fn register_font(&mut self, bytes: Vec<u8>) -> Result<Vec<String>, RasterError> {
        if bytes.len() > MAX_FONT_BYTES - self.font_bytes {
            return Err(RasterError::FontBudgetExceeded);
        }
        if FontRef::from_index(&bytes, 0).is_none() {
            return Err(RasterError::InvalidFont);
        }
        let len = bytes.len();
        let families = self.fonts.collection.register_fonts(bytes.into(), None);
        if families.is_empty() {
            return Err(RasterError::InvalidFont);
        }
        self.font_bytes += len;
        Ok(families
            .into_iter()
            .filter_map(|(id, _)| self.fonts.collection.family_name(id).map(str::to_owned))
            .collect())
    }

    /// Families are an ordered explicit fallback list. All must be registered.
    /// Unsupported characters use the font's missing-glyph outline and are
    /// counted in metrics. Only grayscale outlines are rendered; color emoji,
    /// decorations, rich styles and a persistent glyph atlas are future work.
    pub fn rasterize(
        &mut self,
        text: &str,
        families: &[&str],
        options: RasterOptions,
    ) -> Result<RasterizedText, RasterError> {
        self.paragraph(text, families, options)
            .map(Paragraph::into_bitmap)
    }

    /// Retains layout for caret, selection and hit testing without re-shaping
    /// when only the selection changes. Raster limits also apply to snapshots.
    pub fn paragraph(
        &mut self,
        text: &str,
        families: &[&str],
        options: RasterOptions,
    ) -> Result<Paragraph, RasterError> {
        options.validate()?;
        if text.len() > MAX_TEXT_BYTES {
            return Err(RasterError::InputTooLarge);
        }
        if families.is_empty() || families.len() > 32 {
            return Err(RasterError::InvalidOptions);
        }
        for name in families {
            if self.fonts.collection.family_id(name).is_none() {
                return Err(RasterError::UnknownFamily((*name).to_owned()));
            }
        }
        let names: Vec<_> = families
            .iter()
            .map(|name| FontFamilyName::named(name))
            .collect();
        let mut builder = self
            .layout
            .ranged_builder(&mut self.fonts, text, options.scale, false);
        builder.push_default(StyleProperty::FontSize(options.font_size));
        builder.push_default(names.as_slice());
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(Some(options.width * options.scale));
        layout.align(Alignment::Start, AlignmentOptions::default());
        if !layout.width().is_finite() || !layout.height().is_finite() {
            return Err(RasterError::LayoutTooLarge);
        }
        let mut metrics = TextMetrics {
            width: layout.width() / options.scale,
            height: layout.height() / options.scale,
            lines: layout.lines().count(),
            ..TextMetrics::default()
        };
        let mut bounds = Bounds {
            left: 0,
            top: 0,
            right: layout.width().ceil() as i32,
            bottom: layout.height().ceil() as i32,
        };
        bounds.dimensions()?;
        let mut masks = Vec::new();
        let mut mask_bytes = 0;
        let mut fonts = HashSet::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyphs) = item else {
                    continue;
                };
                let run = glyphs.run();
                let font = run.font();
                fonts.insert((font.data.id(), font.index));
                metrics.rtl_runs += usize::from(run.is_rtl());
                let font_ref = FontRef::from_index(font.data.as_ref(), font.index as usize)
                    .ok_or(RasterError::InvalidFont)?;
                let mut scaler = self
                    .scale
                    .builder_with_id(font_ref, [font.data.id(), u64::from(font.index)])
                    .size(run.font_size())
                    .normalized_coords(run.normalized_coords())
                    .hint(true)
                    .build();
                for glyph in glyphs.positioned_glyphs() {
                    metrics.glyphs += 1;
                    metrics.missing_glyphs += usize::from(glyph.id == 0);
                    if !glyph.x.is_finite() || !glyph.y.is_finite() {
                        return Err(RasterError::InvalidRaster);
                    }
                    let Some(image) = Render::new(&[Source::Outline])
                        .format(Format::Alpha)
                        .offset(Vector::new(
                            glyph.x - glyph.x.floor(),
                            glyph.y - glyph.y.floor(),
                        ))
                        .render(
                            &mut scaler,
                            u16::try_from(glyph.id).map_err(|_| RasterError::InvalidRaster)?,
                        )
                    else {
                        continue;
                    }; // spaces and other blank glyphs have no outline
                    let placement = image.placement;
                    let pixels = (placement.width as usize)
                        .checked_mul(placement.height as usize)
                        .ok_or(RasterError::LayoutTooLarge)?;
                    if image.content != Content::Mask || image.data.len() != pixels {
                        return Err(RasterError::InvalidRaster);
                    }
                    if pixels > MAX_PIXELS - mask_bytes {
                        return Err(RasterError::LayoutTooLarge);
                    }
                    mask_bytes += pixels;
                    let x = (glyph.x.floor() as i32)
                        .checked_add(placement.left)
                        .ok_or(RasterError::InvalidRaster)?;
                    let y = (glyph.y.floor() as i32)
                        .checked_sub(placement.top)
                        .ok_or(RasterError::InvalidRaster)?;
                    bounds.include(x, y, placement.width, placement.height)?;
                    masks.push(Mask {
                        x,
                        y,
                        width: placement.width,
                        data: image.data,
                    });
                }
            }
        }
        metrics.fonts = fonts.len();
        let (width, height) = bounds.dimensions()?;
        let mut rgba8 = vec![0; width as usize * height as usize * 4];
        for mask in masks {
            for (i, alpha) in mask.data.into_iter().enumerate() {
                let x = (mask.x - bounds.left) as usize + i % mask.width as usize;
                let y = (mask.y - bounds.top) as usize + i / mask.width as usize;
                let pixel = &mut rgba8[(y * width as usize + x) * 4..][..4];
                // White masks overlap with source-over alpha coverage. RGB is
                // straight white; alpha must not be gamma-encoded or multiplied twice.
                let alpha = u32::from(alpha);
                let combined = alpha + (u32::from(pixel[3]) * (255 - alpha) + 127) / 255;
                pixel.copy_from_slice(&[255, 255, 255, combined as u8]);
            }
        }
        let bitmap = RasterizedText {
            width,
            height,
            origin_x: bounds.left,
            origin_y: bounds.top,
            scale: options.scale,
            rgba8,
            metrics,
        };
        Ok(Paragraph {
            identity: Arc::new(()),
            bitmap,
            layout,
            text: text.to_owned(),
        })
    }
}

struct Mask {
    x: i32,
    y: i32,
    width: u32,
    data: Vec<u8>,
}
struct Bounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
impl Bounds {
    fn include(&mut self, x: i32, y: i32, width: u32, height: u32) -> Result<(), RasterError> {
        let right = x
            .checked_add(i32::try_from(width).map_err(|_| RasterError::LayoutTooLarge)?)
            .ok_or(RasterError::LayoutTooLarge)?;
        let bottom = y
            .checked_add(i32::try_from(height).map_err(|_| RasterError::LayoutTooLarge)?)
            .ok_or(RasterError::LayoutTooLarge)?;
        self.left = self.left.min(x);
        self.top = self.top.min(y);
        self.right = self.right.max(right);
        self.bottom = self.bottom.max(bottom);
        self.dimensions().map(|_| ())
    }
    fn dimensions(&self) -> Result<(u32, u32), RasterError> {
        let width = (i64::from(self.right) - i64::from(self.left)).max(1);
        let height = (i64::from(self.bottom) - i64::from(self.top)).max(1);
        if width > i64::from(MAX_DIMENSION)
            || height > i64::from(MAX_DIMENSION)
            || width * height > MAX_PIXELS as i64
        {
            return Err(RasterError::LayoutTooLarge);
        }
        Ok((width as u32, height as u32))
    }
}

#[cfg(test)]
mod tests;
