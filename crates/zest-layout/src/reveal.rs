use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct ScrollAdjustment {
    pub widget: WidgetId,
    /// Absolute requested offset, already clamped to this snapshot's range.
    pub offset: Point<Logical>,
}

/// Read-only advice computed from one snapshot. Apply offsets to the matching
/// styles, sync/compute fresh geometry, and then update input/focus. Plans contain
/// no scene eligibility or entity-generation checks; use the current snapshot.
///
/// ```
/// use zest_core::{Scene, geometry::{Point, Size}};
/// use zest_layout::{LayoutTree, Length, Style};
/// let mut scene = Scene::default();
/// scene.update(|ui| { ui.container("scroll", |ui| {
///     ui.button("prefix", "Prefix"); ui.button("target", "Target");
/// }); })?;
/// let root = scene.roots()[0].clone();
/// let target = scene.children(&root).unwrap()[1].clone();
/// let styles = |offset| {
///     let (root, target) = (&root, &target);
///     move |id: &_| if id == root {
///         Style { width: Length::Px(100.0), height: Length::Px(40.0), clip_children: true,
///             scroll_offset: offset, ..Default::default() }
///     } else {
///         Style { height: Length::Px(if id == target { 20.0 } else { 80.0 }), shrink: 0.0, ..Default::default() }
///     }
/// };
/// let mut tree = LayoutTree::default();
/// tree.sync(&scene, styles(Point::new(0.0, 0.0)))?;
/// let viewport = Size::new(100.0, 100.0);
/// let old = tree.compute(viewport)?.snapshot;
/// let plan = old.reveal_plan(&target).unwrap();
/// tree.sync(&scene, styles(plan.adjustments[0].offset))?;
/// let revealed = tree.compute(viewport)?;
/// assert!(!revealed.stats.performed);
/// assert_eq!(revealed.snapshot.visible_bounds(&target), Some(plan.visible_bounds));
/// assert!(old.visible_bounds(&target).is_none());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct RevealPlan {
    /// Changed clipping ancestors, ordered from innermost to outermost.
    pub adjustments: Vec<ScrollAdjustment>,
    /// Predicted visible part after all adjustments and viewport clipping.
    /// Oversized targets may be partially visible.
    pub visible_bounds: Rect<Logical>,
}

impl LayoutSnapshot {
    /// Plan the nearest scroll positions that reveal this node through all
    /// clipping ancestors. Small targets fit fully when ranges allow it. An
    /// oversized target fills its scrollport, retaining its current position if
    /// it already spans that port. Nested clips propagate only the visible part
    /// outward, so clipped-away content cannot pull an outer scroller farther.
    ///
    /// Returns None for unknown/empty nodes or targets that remain completely
    /// clipped/unreachable. A visible target can return an empty adjustment list.
    /// This never mutates the snapshot, runs Taffy or measures text. Applications
    /// decide enabled/modal eligibility and minimum physical visible dimensions.
    pub fn reveal_plan(&self, id: &WidgetId) -> Option<RevealPlan> {
        let index = *self.0.index.get(id)?;
        let mut visible = self.0.boxes[index].bounds;
        if visible.is_empty() {
            return None;
        }
        let mut parent = self.0.parents[index];
        let mut ancestors = Vec::new();
        while let Some(index) = parent {
            ancestors.push(index);
            parent = self.0.parents[index];
        }
        // Pull the viewport's reachable area through ancestor scroll ranges.
        // Unlike a node's full border box, this also accounts for scrollports
        // that can only ever be partially visible through an outer boundary.
        let mut reachable =
            Rect::from_xywh(0.0, 0.0, self.viewport().width, self.viewport().height);
        let mut ports = Vec::new();
        for index in ancestors.into_iter().rev() {
            if self.0.clipping[index] {
                let node = &self.0.boxes[index];
                let port = node.bounds.intersection(&reachable)?;
                ports.push((index, port));
                reachable = Rect::from_xywh(
                    port.origin.x - node.scroll.offset.x,
                    port.origin.y - node.scroll.offset.y,
                    port.size.width + node.scroll.max_offset.x,
                    port.size.height + node.scroll.max_offset.y,
                );
            }
        }
        let mut adjustments = Vec::new();
        for (index, port) in ports.into_iter().rev() {
            let node = &self.0.boxes[index];
            let current = node.scroll.offset;
            let offset = Point::new(
                nearest(
                    visible.origin.x,
                    visible.size.width,
                    port.origin.x,
                    port.size.width,
                    current.x,
                    node.scroll.max_offset.x,
                ),
                nearest(
                    visible.origin.y,
                    visible.size.height,
                    port.origin.y,
                    port.size.height,
                    current.y,
                    node.scroll.max_offset.y,
                ),
            );
            if offset != current {
                visible.origin.x -= offset.x - current.x;
                visible.origin.y -= offset.y - current.y;
                adjustments.push(ScrollAdjustment {
                    widget: node.id.clone(),
                    offset,
                });
            }
            visible = visible.intersection(&port)?;
        }
        visible = visible.intersection(&Rect::from_xywh(
            0.0,
            0.0,
            self.viewport().width,
            self.viewport().height,
        ))?;
        Some(RevealPlan {
            adjustments,
            visible_bounds: visible,
        })
    }
}

fn nearest(start: f64, size: f64, port_start: f64, port_size: f64, current: f64, max: f64) -> f64 {
    let end = start + size;
    let port_end = port_start + port_size;
    let delta = if size <= port_size {
        if start < port_start {
            start - port_start
        } else if end > port_end {
            end - port_end
        } else {
            0.0
        }
    } else if start > port_start {
        start - port_start
    } else if end < port_end {
        end - port_end
    } else {
        0.0
    };
    (current + delta).clamp(0.0, max)
}

#[cfg(test)]
mod tests;
