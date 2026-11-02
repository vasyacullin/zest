use std::{fmt, sync::Arc};
use zest_text::raster::ShapedText;

/// Shared immutable shaped text for a measured leaf. Cloning retains the same
/// content identity and layout caches; replacing it invalidates measurement.
/// Shape again after text/font/fallback/font-size/DPI changes. Color is not part
/// of the content and does not require a new shape.
#[derive(Clone)]
pub struct TextContent(pub(crate) Arc<ShapedText>);
impl TextContent {
    pub fn new(text: ShapedText) -> Self {
        Self(Arc::new(text))
    }
    pub fn shaped(&self) -> &ShapedText {
        &self.0
    }
}
impl PartialEq for TextContent {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl fmt::Debug for TextContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextContent")
            .field("source_bytes", &self.0.text().len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
