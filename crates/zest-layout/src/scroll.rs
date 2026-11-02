use super::*;

/// Logical content offsets for one clipping node. Ranges include positive
/// descendant overflow and trailing padding. Clipping descendants bound their
/// own overflow; their internal scrolling never expands an ancestor's range.
///
/// ```
/// use zest_core::{Scene, geometry::{Point, Size}};
/// use zest_layout::{LayoutTree, Length, Style};
/// let mut scene = Scene::default();
/// scene.update(|ui| { ui.container("scroll", |ui| { ui.button("content", "Content"); }); })?;
/// let root = scene.roots()[0].clone();
/// let child = scene.children(&root).unwrap()[0].clone();
/// let mut tree = LayoutTree::default();
/// tree.sync(&scene, |id| if id == &root {
///     Style { width: Length::Px(100.0), height: Length::Px(40.0), clip_children: true,
///         scroll_offset: Point::new(0.0, 30.0), ..Default::default() }
/// } else {
///     Style { height: Length::Px(120.0), shrink: 0.0, ..Default::default() }
/// })?;
/// let frame = tree.compute(Size::new(200.0, 200.0))?.snapshot;
/// assert_eq!(frame.scroll(&root).unwrap().max_offset.y, 80.0);
/// assert_eq!(frame.bounds(&child).unwrap().origin.y, -30.0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollMetrics {
    pub offset: Point<Logical>,
    pub max_offset: Point<Logical>,
}
impl Default for ScrollMetrics {
    fn default() -> Self {
        Self {
            offset: Point::new(0.0, 0.0),
            max_offset: Point::new(0.0, 0.0),
        }
    }
}

impl LayoutTree {
    pub(super) fn scroll_metrics(&self) -> Result<HashMap<WidgetId, ScrollMetrics>, LayoutError> {
        let mut extents: HashMap<WidgetId, Size<Logical>> =
            HashMap::with_capacity(self.order.len());
        let mut metrics = HashMap::with_capacity(self.order.len());
        // Taffy geometry stays unscrolled. Accumulate overflow bottom-up before
        // projecting nodes top-down, so nested scroll offsets cannot feed back
        // into range calculation or text measurement.
        for id in self.order.iter().rev() {
            let entry = &self.entries[id];
            let layout = self
                .tree
                .layout(entry.node)
                .expect("owned computed layout node");
            let mut extent = Size::new(f64::from(layout.size.width), f64::from(layout.size.height));
            for child in &entry.children {
                let child_entry = &self.entries[child];
                let child_layout = self
                    .tree
                    .layout(child_entry.node)
                    .expect("owned computed child");
                let child_extent = if child_entry.style.clip_children {
                    Size::new(
                        f64::from(child_layout.size.width),
                        f64::from(child_layout.size.height),
                    )
                } else {
                    extents[child]
                };
                extent.width = extent.width.max(
                    f64::from(child_layout.location.x)
                        + child_extent.width
                        + f64::from(entry.style.padding.right),
                );
                extent.height = extent.height.max(
                    f64::from(child_layout.location.y)
                        + child_extent.height
                        + f64::from(entry.style.padding.bottom),
                );
            }
            if !extent.width.is_finite() || !extent.height.is_finite() {
                return Err(LayoutError::InvalidGeometry(id.clone()));
            }
            let max_offset = if entry.style.clip_children {
                Point::new(
                    (extent.width - f64::from(layout.size.width)).max(0.0),
                    (extent.height - f64::from(layout.size.height)).max(0.0),
                )
            } else {
                Point::new(0.0, 0.0)
            };
            metrics.insert(
                id.clone(),
                ScrollMetrics {
                    offset: Point::new(
                        entry.style.scroll_offset.x.min(max_offset.x),
                        entry.style.scroll_offset.y.min(max_offset.y),
                    ),
                    max_offset,
                },
            );
            extents.insert(id.clone(), extent);
        }
        Ok(metrics)
    }
}

#[cfg(test)]
mod tests;
