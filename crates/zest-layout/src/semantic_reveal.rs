use crate::{LayoutSnapshot, SemanticView};
use zest_core::{
    Scene,
    geometry::{Logical, Point, Rect},
    semantics::SemanticTarget,
};

/// One absolute offset bound to its observed scroll-owner instance.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticScrollAdjustment {
    owner: SemanticTarget,
    offset: Point<Logical>,
}
impl SemanticScrollAdjustment {
    pub fn owner(&self) -> &SemanticTarget {
        &self.owner
    }
    pub fn offset(&self) -> Point<Logical> {
        self.offset
    }
}
/// Read-only nearest-edge reveal advice with observed instance bindings. Apply
/// only after can_apply with current scene and freshly computed geometry. Offset
/// edits, layout recomputation, visibility checks and action dispatch are caller
/// responsibilities. This plan never changes focus, captures input or runs layout.
#[derive(Clone, Debug)]
pub struct SemanticRevealPlan {
    target: SemanticTarget,
    source: LayoutSnapshot,
    adjustments: Vec<SemanticScrollAdjustment>,
    visible_bounds: Rect<Logical>,
}
impl SemanticRevealPlan {
    pub fn target(&self) -> &SemanticTarget {
        &self.target
    }
    pub fn source_layout(&self) -> &LayoutSnapshot {
        &self.source
    }
    /// Changed owners in inner-to-outer order; the offsets are already clamped.
    pub fn adjustments(&self) -> &[SemanticScrollAdjustment] {
        &self.adjustments
    }
    pub fn visible_bounds(&self) -> Rect<Logical> {
        self.visible_bounds
    }
    /// Require this exact source layout and live, currently input-eligible target
    /// and changed owners. In particular, offsets outside an active modal scope
    /// are not allowed. A nonempty applied plan requires fresh geometry/replanning
    /// afterward; an equal-looking independently computed layout also invalidates
    /// this check. This cannot certify that callers synchronized their scene/styles
    /// before supplying current geometry. Physical extent/occlusion/window policy
    /// remain caller concerns.
    pub fn can_apply(&self, scene: &Scene, current: &LayoutSnapshot) -> bool {
        self.source.same_snapshot(current)
            && self.target.input_allowed(scene)
            && self
                .adjustments
                .iter()
                .all(|change| change.owner.input_allowed(scene))
    }
}
impl SemanticView {
    /// Bind LayoutSnapshot::reveal_plan advice to the target/owners observed by
    /// this semantic view. Returns None for mismatched target instances, missing
    /// semantic owner data or unknown/empty/unreachable geometry. Planning remains
    /// available for disabled/blocked observations; can_apply checks current rules.
    /// Pair compatible/current snapshots as for all SemanticView operations.
    ///
    /// ```
    /// use zest_core::{Scene, geometry::Size, semantics::SemanticQuery};
    /// use zest_layout::{LayoutTree, Length, Style};
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.container("port", |ui| { ui.button("prefix", "Prefix"); ui.button("save", "Save"); }); })?;
    /// let root = scene.roots()[0].clone();
    /// let mut tree = LayoutTree::default();
    /// tree.sync(&scene, |id| if id == &root {
    ///     Style { height: Length::Px(40.0), clip_children: true, ..Default::default() }
    /// } else { Style { height: Length::Px(60.0), shrink: 0.0, ..Default::default() } })?;
    /// let frame = tree.compute(Size::new(200.0, 100.0))?.snapshot;
    /// let view = frame.semantic_view(&scene.semantic_snapshot());
    /// let target = view.semantics().find_target(SemanticQuery::new().named("Save"))?;
    /// let plan = view.reveal_target(&target).unwrap();
    /// assert!(plan.can_apply(&scene, &frame));
    /// assert_eq!(plan.adjustments()[0].owner().id(), &root);
    /// assert!(view.node(target.id()).unwrap().visible_bounds().is_none());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn reveal_target(&self, target: &SemanticTarget) -> Option<SemanticRevealPlan> {
        let observed = self.semantics().target(target.id())?;
        if &observed != target {
            return None;
        }
        let plan = self.layout().reveal_plan(target.id())?;
        let adjustments = plan
            .adjustments
            .into_iter()
            .map(|change| {
                Some(SemanticScrollAdjustment {
                    owner: self.semantics().target(&change.widget)?,
                    offset: change.offset,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(SemanticRevealPlan {
            target: observed,
            source: self.layout().clone(),
            adjustments,
            visible_bounds: plan.visible_bounds,
        })
    }
}

#[cfg(test)]
mod tests;
