use super::{LayoutSnapshot, WidgetId};

/// A comparison of computed layout data, independent of scene paint flags.
/// Added/changed nodes follow the new declaration order; removed nodes follow
/// the old order. Added and changed sets are disjoint. This contains identities,
/// not entity generations, damage rectangles or cached paint commands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutChanges {
    pub added: Vec<WidgetId>,
    pub removed: Vec<WidgetId>,
    /// Retained nodes whose bounds, layout bounds, effective clip, scroll
    /// metrics, parent or clip-children policy changed. Includes invisible nodes.
    pub changed: Vec<WidgetId>,
    /// The full declaration sequence differs, including additions/removals.
    pub order_changed: bool,
    pub viewport_changed: bool,
}
impl LayoutChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && !self.order_changed
            && !self.viewport_changed
    }
}

impl LayoutSnapshot {
    /// Compare this snapshot with an earlier one without mutating either, the
    /// layout engine or scene. Shared snapshots return empty immediately; other
    /// comparisons walk both snapshots using their existing identity indexes.
    /// Independent trees with equal computed data compare equal too.
    ///
    /// This compares published layout, not style inputs, text content, widget
    /// appearance or entity generations. Combine with Scene paint invalidation.
    /// Old/new bounds remain accessible on their respective snapshots. A changed
    /// viewport/order/removal may require clearing old pixels even with no changed
    /// surviving nodes. Callers choose redraw bounds and schedule rendering.
    ///
    /// ```
    /// use zest_core::{Scene, geometry::Size};
    /// use zest_layout::{LayoutTree, Length, Style};
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.button("fill", "Fill"); })?;
    /// let id = scene.roots()[0].clone();
    /// let mut tree = LayoutTree::default();
    /// tree.sync(&scene, |_| Style { width: Length::Percent(1.0), height: Length::Px(20.0), ..Default::default() })?;
    /// let old = tree.compute(Size::new(100.0, 80.0))?.snapshot;
    /// let new = tree.compute(Size::new(200.0, 80.0))?.snapshot;
    /// let changes = new.changes_since(&old);
    /// assert_eq!(changes.changed, [id]);
    /// assert!(changes.viewport_changed && !changes.order_changed);
    /// assert_eq!(old.boxes()[0].bounds.size.width, 100.0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn changes_since(&self, previous: &Self) -> LayoutChanges {
        if self.same_snapshot(previous) {
            return LayoutChanges::default();
        }
        let mut changes = LayoutChanges {
            viewport_changed: self.viewport() != previous.viewport(),
            order_changed: self
                .boxes()
                .iter()
                .map(|node| &node.id)
                .ne(previous.boxes().iter().map(|node| &node.id)),
            ..Default::default()
        };
        for (index, node) in self.boxes().iter().enumerate() {
            let Some(&old_index) = previous.0.index.get(&node.id) else {
                changes.added.push(node.id.clone());
                continue;
            };
            let parent = self.0.parents[index].map(|index| &self.0.boxes[index].id);
            let old_parent = previous.0.parents[old_index].map(|index| &previous.0.boxes[index].id);
            if node != &previous.0.boxes[old_index]
                || self.0.clipping[index] != previous.0.clipping[old_index]
                || parent != old_parent
            {
                changes.changed.push(node.id.clone());
            }
        }
        changes.removed.extend(
            previous
                .boxes()
                .iter()
                .filter(|node| !self.0.index.contains_key(&node.id))
                .map(|node| node.id.clone()),
        );
        changes
    }
}

#[cfg(test)]
mod tests;
