use crate::{LayoutBox, LayoutSnapshot};
use zest_core::{
    WidgetId,
    geometry::{Logical, Point, Rect},
    semantics::{FindError, SemanticNode, SemanticQuery, SemanticSnapshot, SemanticTarget},
};

/// A semantic observation with optional geometry from the paired layout.
/// Missing geometry differs from a known box whose visible bounds are empty.
#[derive(Clone, Copy, Debug)]
pub struct SemanticViewNode<'a> {
    pub semantic: &'a SemanticNode,
    pub geometry: Option<&'a LayoutBox>,
}
impl SemanticViewNode<'_> {
    pub fn visible_bounds(&self) -> Option<Rect<Logical>> {
        self.geometry.and_then(LayoutBox::visible_bounds)
    }
    /// Positive rectangular area after viewport/ancestor clipping. This does not
    /// test occlusion, input eligibility or a minimum physical-pixel extent.
    pub fn is_visible(&self) -> bool {
        self.visible_bounds().is_some()
    }
}

/// An immutable join by WidgetId, retaining both published snapshots cheaply.
/// Pair compatible, current observations from the same synchronized scene before
/// input. This join cannot certify layout/style freshness or snapshot provenance.
/// Missing geometry is preserved for inspection. Bound targets retain the
/// semantic snapshot's generations and check current scene eligibility on use.
/// No layout computation, state mutation or rendering runs here.
#[derive(Clone, Debug)]
pub struct SemanticView {
    layout: LayoutSnapshot,
    semantics: SemanticSnapshot,
}
impl LayoutSnapshot {
    /// Join semantic observations to projected bounds/clips and scroll metrics.
    ///
    /// ```
    /// use zest_core::{Scene, geometry::Size, semantics::SemanticQuery};
    /// use zest_layout::{LayoutTree, Length, Style};
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.button("save", "Save"); })?;
    /// let mut tree = LayoutTree::default();
    /// tree.sync(&scene, |_| Style { width: Length::Px(80.0), height: Length::Px(20.0), ..Default::default() })?;
    /// let layout = tree.compute(Size::new(200.0, 100.0))?.snapshot;
    /// let view = layout.semantic_view(&scene.semantic_snapshot());
    /// let save = view.find_visible(SemanticQuery::new().named("Save"))?;
    /// assert_eq!(save.visible_bounds().unwrap().size, Size::new(80.0, 20.0));
    /// let target = view.find_visible_target(SemanticQuery::new().named("Save"))?;
    /// assert!(target.focus(&mut scene));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn semantic_view(&self, semantics: &SemanticSnapshot) -> SemanticView {
        SemanticView {
            layout: self.clone(),
            semantics: semantics.clone(),
        }
    }
}
impl SemanticView {
    pub fn layout(&self) -> &LayoutSnapshot {
        &self.layout
    }
    pub fn semantics(&self) -> &SemanticSnapshot {
        &self.semantics
    }
    fn project<'a>(&'a self, node: &'a SemanticNode) -> SemanticViewNode<'a> {
        SemanticViewNode {
            semantic: node,
            geometry: self
                .layout
                .0
                .index
                .get(&node.id)
                .map(|&index| &self.layout.0.boxes[index]),
        }
    }
    pub fn node(&self, id: &WidgetId) -> Option<SemanticViewNode<'_>> {
        self.semantics.node(id).map(|node| self.project(node))
    }
    /// All semantic nodes in semantic declaration order, including missing/hidden
    /// geometry. Hit testing instead follows the layout's reverse paint order.
    pub fn nodes(
        &self,
    ) -> impl ExactSizeIterator<Item = SemanticViewNode<'_>> + DoubleEndedIterator {
        self.semantics.nodes().iter().map(|node| self.project(node))
    }
    pub fn find_all(&self, query: SemanticQuery<'_>) -> Vec<SemanticViewNode<'_>> {
        self.semantics
            .find_all(query)
            .into_iter()
            .map(|node| self.project(node))
            .collect()
    }
    pub fn find(&self, query: SemanticQuery<'_>) -> Result<SemanticViewNode<'_>, FindError> {
        self.semantics.find(query).map(|node| self.project(node))
    }
    /// Apply logical rectangular visibility before requiring a unique match.
    /// Disabled/blocked nodes remain unless the semantic query filters them out.
    pub fn find_visible(
        &self,
        query: SemanticQuery<'_>,
    ) -> Result<SemanticViewNode<'_>, FindError> {
        let matches: Vec<_> = self
            .find_all(query)
            .into_iter()
            .filter(SemanticViewNode::is_visible)
            .collect();
        match matches.as_slice() {
            [] => Err(FindError::NotFound),
            [node] => Ok(*node),
            _ => Err(FindError::Ambiguous(
                matches
                    .iter()
                    .map(|node| node.semantic.id.clone())
                    .collect(),
            )),
        }
    }
    /// Bind the visible match's observed semantic generation. This does not
    /// upgrade a stale semantic snapshot to a recreated node's current instance.
    pub fn find_visible_target(
        &self,
        query: SemanticQuery<'_>,
    ) -> Result<SemanticTarget, FindError> {
        let node = self.find_visible(query)?;
        Ok(self.semantics.target(&node.semantic.id).unwrap())
    }
    /// Rectangular hit test through the paired layout, applying semantic policy
    /// in the predicate. Layout-only nodes are skipped. Viewport/clip edges are
    /// half-open; custom overlays/transforms and current scene validation remain
    /// caller concerns, just as for LayoutSnapshot::hit_test.
    pub fn hit_test(
        &self,
        point: Point<Logical>,
        mut accepts: impl FnMut(&SemanticNode) -> bool,
    ) -> Option<SemanticViewNode<'_>> {
        let id = self.layout.hit_test(point, |id| {
            self.semantics.node(id).is_some_and(&mut accepts)
        })?;
        self.node(id)
    }
}

#[cfg(test)]
mod tests;
