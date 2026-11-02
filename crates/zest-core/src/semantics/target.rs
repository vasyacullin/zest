use super::{FindError, SemanticQuery, SemanticSnapshot};
use crate::{
    Scene, WidgetId,
    event::{DispatchResult, EventContext, EventControl},
};
use bevy_ecs::{entity::Entity, world::WorldId};
use std::fmt;

/// An opaque instance binding from a semantic snapshot. Reorders, metadata and
/// state changes retain it; deletion/recreation or another scene invalidate it.
/// Current enabled/modal rules are checked when acting, not taken from old state.
/// Query filters describe the observation and are not rerun by this target.
/// Geometry/visibility and platform focus ownership remain application policy.
/// No scene borrow or native ECS handle is exposed or kept alive.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SemanticTarget {
    id: WidgetId,
    entity: Entity,
    world: WorldId,
}
impl fmt::Debug for SemanticTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticTarget")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl SemanticTarget {
    pub fn id(&self) -> &WidgetId {
        &self.id
    }
    /// Instance existence only; disabled/modal-blocked nodes remain live.
    pub fn is_live(&self, scene: &Scene) -> bool {
        self.world == scene.world.id() && scene.entities.get(&self.id) == Some(&self.entity)
    }
    pub fn input_allowed(&self, scene: &Scene) -> bool {
        self.is_live(scene) && scene.input_allowed(&self.id)
    }
    /// Focus using current scene rules. Invalid targets leave focus unchanged.
    /// Visibility is a caller policy, just as for Scene::focus.
    pub fn focus(&self, scene: &mut Scene) -> bool {
        self.is_live(scene) && scene.focus(&self.id)
    }
    /// Route through the current hierarchy only if the original instance is
    /// still live and input eligible. Rejected targets never call handlers or
    /// change the payload. This does not synthesize pointer/key gestures or apply
    /// defaults. Use DispatchResult::default_prevented as for Scene dispatch;
    /// revalidate this binding before applying any later deferred action.
    pub fn dispatch_event<E>(
        &self,
        scene: &Scene,
        event: &mut E,
        handler: impl FnMut(EventContext<'_>, &mut E) -> EventControl,
    ) -> Option<DispatchResult> {
        if !self.is_live(scene) {
            return None;
        }
        scene.dispatch_event(&self.id, event, handler)
    }
}
impl SemanticSnapshot {
    /// Bind the observed entity generation, even if currently disabled/blocked.
    /// A stale snapshot never binds a newly recreated instance by accident.
    pub fn target(&self, id: &WidgetId) -> Option<SemanticTarget> {
        let &index = self.0.index.get(id)?;
        Some(SemanticTarget {
            id: id.clone(),
            entity: self.0.entities[index],
            world: self.0.world,
        })
    }
    /// Require a unique query match and bind its observed instance.
    ///
    /// ```
    /// use zest_core::{Scene, semantics::SemanticQuery};
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.button("save", "Save"); })?;
    /// let target = scene.semantic_snapshot().find_target(SemanticQuery::new().named("Save"))?;
    /// assert!(target.focus(&mut scene));
    /// scene.update(|_| {})?;
    /// scene.update(|ui| { ui.button("save", "Save"); })?;
    /// assert!(!target.is_live(&scene));
    /// assert!(!target.focus(&mut scene));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn find_target(&self, query: SemanticQuery<'_>) -> Result<SemanticTarget, FindError> {
        let node = self.find(query)?;
        Ok(self.target(&node.id).unwrap())
    }
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
