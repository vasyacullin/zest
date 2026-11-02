use super::{SemanticSnapshot, WidgetId};

/// Semantic observations changed between snapshots. Added/changed/replaced IDs
/// follow new declaration preorder; removed IDs follow the old order. Lists are
/// disjoint: a replaced path appears only in replaced, even if its data changed.
/// Read old/new node data from their respective snapshots to publish updates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticChanges {
    pub added: Vec<WidgetId>,
    pub removed: Vec<WidgetId>,
    /// Retained instances with different role, metadata, parent/children or state.
    pub changed: Vec<WidgetId>,
    /// The path exists in both snapshots but belongs to a different entity
    /// generation or scene. Identical published data still counts as replacement.
    pub replaced: Vec<WidgetId>,
    /// The full node sequence differs, including additions/removals.
    pub order_changed: bool,
    pub roots_changed: bool,
    /// Innermost modal root path or its instance changed, including an empty scope.
    pub modal_changed: bool,
}
impl SemanticChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.replaced.is_empty()
            && !self.order_changed
            && !self.roots_changed
            && !self.modal_changed
    }
}
impl SemanticSnapshot {
    fn same_instance(&self, index: usize, previous: &Self, old_index: usize) -> bool {
        self.0.world == previous.0.world && self.0.entities[index] == previous.0.entities[old_index]
    }
    /// Compare observed semantics and private node generations without mutating
    /// either snapshot or the scene. Shared snapshots return empty immediately;
    /// other comparisons walk both node lists using their existing indexes.
    /// Independent observations of an unchanged scene compare empty too.
    ///
    /// Private instance stamps distinguish deletion/recreation and foreign scenes
    /// without exposing ECS handles. Returned IDs are still paths, not safe action
    /// targets. This reports differences, not a transaction/event log: transient
    /// state changes between observations are omitted. Nested modal stack depth is
    /// not published; only the innermost root instance and node eligibility matter.
    /// Geometry and paint bookkeeping are outside the comparison. Consumers choose
    /// update ordering and join with current layout; no platform updates run here.
    ///
    /// ```
    /// use zest_core::Scene;
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.button("save", "Save"); })?;
    /// let id = scene.roots()[0].clone();
    /// let old = scene.semantic_snapshot();
    /// scene.focus(&id);
    /// let focused = scene.semantic_snapshot();
    /// assert_eq!(focused.changes_since(&old).changed, std::slice::from_ref(&id));
    /// scene.update(|_| {})?;
    /// scene.update(|ui| { ui.button("save", "Save"); })?;
    /// let recreated = scene.semantic_snapshot();
    /// let changes = recreated.changes_since(&old);
    /// assert_eq!(changes.replaced, [id]);
    /// assert!(changes.changed.is_empty() && !changes.order_changed);
    /// # Ok::<(), zest_core::ReconcileError>(())
    /// ```
    pub fn changes_since(&self, previous: &Self) -> SemanticChanges {
        if self.same_snapshot(previous) {
            return SemanticChanges::default();
        }
        let modal_changed = self.modal_root() != previous.modal_root()
            || self
                .modal_root()
                .zip(previous.modal_root())
                .is_some_and(|(id, old)| {
                    !self.same_instance(self.0.index[id], previous, previous.0.index[old])
                });
        let mut changes = SemanticChanges {
            order_changed: self
                .nodes()
                .iter()
                .map(|node| &node.id)
                .ne(previous.nodes().iter().map(|node| &node.id)),
            roots_changed: self.roots() != previous.roots(),
            modal_changed,
            ..Default::default()
        };
        for (index, node) in self.nodes().iter().enumerate() {
            match previous.0.index.get(&node.id) {
                None => changes.added.push(node.id.clone()),
                Some(&old_index) if !self.same_instance(index, previous, old_index) => {
                    changes.replaced.push(node.id.clone())
                }
                Some(&old_index) if node != &previous.0.nodes[old_index] => {
                    changes.changed.push(node.id.clone())
                }
                _ => {}
            }
        }
        changes.removed.extend(
            previous
                .nodes()
                .iter()
                .filter(|node| !self.0.index.contains_key(&node.id))
                .map(|node| node.id.clone()),
        );
        changes
    }
}

#[cfg(test)]
mod tests;
