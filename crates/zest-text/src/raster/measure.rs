use super::*;
use parley::{FontFamilyName, StyleProperty};

/// Logical font size and physical pixels per logical unit, independent of width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeOptions {
    pub font_size: f32,
    pub scale: f32,
}
impl Default for ShapeOptions {
    fn default() -> Self {
        Self {
            font_size: 20.0,
            scale: 1.0,
        }
    }
}
impl ShapeOptions {
    pub(super) fn validate(self) -> Result<(), RasterError> {
        if !self.font_size.is_finite()
            || !(0.0..=256.0).contains(&self.font_size)
            || self.font_size == 0.0
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || self.scale > 8.0
            || self.font_size * self.scale > 512.0
            || self.font_size * self.scale < 1.0
        {
            return Err(RasterError::InvalidOptions);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeasureWidth {
    /// Break at every permitted line break, preserving unbreakable segments.
    MinContent,
    /// Only explicit hard line breaks constrain line width.
    MaxContent,
    /// A finite nonnegative logical width; zero is a valid measurement constraint.
    Definite(f32),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMeasurement {
    /// Logical typographic width including trailing whitespace, excluding ink overhangs.
    pub width: f32,
    pub height: f32,
    pub lines: usize,
}
/// Immutable font-resolved glyph shaping. Measurement reflows a copy of the
/// layout without reshaping or producing glyph masks/bitmaps. Retains its font
/// data; registering more fallback fonts does not alter an existing shape.
pub struct ShapedText {
    pub(super) text: String,
    pub(super) layout: Layout<()>,
    pub(super) scale: f32,
}
impl ShapedText {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn scale(&self) -> f32 {
        self.scale
    }
    pub fn measure(&self, width: MeasureWidth) -> Result<TextMeasurement, RasterError> {
        let width = match width {
            MeasureWidth::MinContent => Some(0.0),
            MeasureWidth::MaxContent => None,
            MeasureWidth::Definite(width) => {
                if !width.is_finite() || width < 0.0 || !(width * self.scale).is_finite() {
                    return Err(RasterError::InvalidOptions);
                }
                Some(width * self.scale)
            }
        };
        let mut layout = self.layout.clone();
        layout.break_all_lines(width);
        layout.align(Alignment::Start, AlignmentOptions::default());
        let measurement = TextMeasurement {
            width: layout.full_width() / self.scale,
            height: layout.height() / self.scale,
            lines: layout.len(),
        };
        if !measurement.width.is_finite()
            || !measurement.height.is_finite()
            || measurement.width < 0.0
            || measurement.height < 0.0
        {
            return Err(RasterError::LayoutTooLarge);
        }
        Ok(measurement)
    }
}
impl Rasterizer {
    /// Shapes at most 64 KiB of UTF-8 using the same explicit fonts and options
    /// as paragraph rasterization. No bitmap dimension/pixel budget is consumed.
    pub fn shape(
        &mut self,
        text: &str,
        families: &[&str],
        options: ShapeOptions,
    ) -> Result<ShapedText, RasterError> {
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
        Ok(ShapedText {
            text: text.to_owned(),
            layout: builder.build(text),
            scale: options.scale,
        })
    }
}

#[cfg(test)]
mod tests;
