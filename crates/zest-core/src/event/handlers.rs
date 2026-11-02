use super::{DispatchResult, EventContext, EventControl, EventPhase};
use crate::semantics::SemanticTarget;
use crate::{Scene, WidgetId};
use bevy_ecs::{entity::Entity, world::WorldId};
use std::collections::HashMap;

type Callback<E, S> = Box<dyn FnMut(EventContext<'_>, &mut E, &mut S) -> EventControl>;
struct Handler<E, S> {
    entity: Entity,
    callback: Callback<E, S>,
}

/// Typed handlers owned by one scene, with one callback per widget/phase.
/// Bindings survive keyed reorders and label changes, but never deletion and
/// recreation. A registry for a different scene cannot receive or bind events.
/// Disabling or modal-blocking a node suppresses delivery without removing its
/// handlers. Dispatch borrows the scene immutably; apply scene edits afterward.
///
/// Callbacks may own captured data and mutate the supplied payload/application
/// state. No Send/Sync bound is required. Remove/replace/clear drops callbacks;
/// call [`Self::prune`] after reconciliation to release deleted nodes' captures.
/// Dispatch ignores stale bindings even without pruning. The registry stays
/// mutably borrowed until dispatch returns; handlers cannot change its bindings
/// or dispatch recursively through the same registry.
///
/// ```
/// use zest_core::{Scene, event::{EventHandlers, EventPhase, EventControl}};
/// let mut scene = Scene::default();
/// scene.update(|ui| { ui.button("save", "Save"); })?;
/// let save = scene.roots()[0].clone();
/// let mut handlers = EventHandlers::new(&scene);
/// assert!(handlers.set(&scene, &save, EventPhase::Target, |_, amount: &mut u32, total: &mut u32| {
///     *total += *amount;
///     EventControl::PreventDefault
/// }));
/// let mut total = 0;
/// let result = handlers.dispatch(&scene, &save, &mut 2, &mut total).unwrap();
/// assert_eq!(total, 2);
/// assert!(result.default_prevented);
/// # Ok::<(), zest_core::ReconcileError>(())
/// ```
pub struct EventHandlers<E, S> {
    world: WorldId,
    handlers: HashMap<(WidgetId, EventPhase), Handler<E, S>>,
}

impl<E, S> EventHandlers<E, S> {
    pub fn new(scene: &Scene) -> Self {
        Self {
            world: scene.world.id(),
            handlers: HashMap::new(),
        }
    }

    /// Install or replace a callback for a current node, including a disabled or
    /// modal-blocked one. Unknown nodes/foreign scenes return false unchanged.
    /// Replacing explicitly binds to the node's current entity generation.
    pub fn set(
        &mut self,
        scene: &Scene,
        widget: &WidgetId,
        phase: EventPhase,
        handler: impl FnMut(EventContext<'_>, &mut E, &mut S) -> EventControl + 'static,
    ) -> bool {
        if self.world != scene.world.id() {
            return false;
        }
        let Some(&entity) = scene.entities.get(widget) else {
            return false;
        };
        self.handlers.insert(
            (widget.clone(), phase),
            Handler {
                entity,
                callback: Box::new(handler),
            },
        );
        true
    }

    /// Number of stored bindings, including stale ones not yet pruned.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
    pub fn remove(&mut self, widget: &WidgetId, phase: EventPhase) -> bool {
        self.handlers.remove(&(widget.clone(), phase)).is_some()
    }
    pub fn clear(&mut self) {
        self.handlers.clear();
    }

    /// Drop bindings whose node was removed/recreated. Returns the number
    /// removed. A foreign scene leaves this registry unchanged and returns zero.
    pub fn prune(&mut self, scene: &Scene) -> usize {
        if self.world != scene.world.id() {
            return 0;
        }
        let before = self.len();
        self.handlers
            .retain(|(id, _), handler| scene.entities.get(id) == Some(&handler.entity));
        before - self.len()
    }

    /// Use the scene's current capture/target/bubble route and modal boundary.
    /// Unhandled nodes continue; stale bindings are skipped. Result visits count
    /// route nodes/phases, not callback invocations. Default behavior remains the
    /// caller's responsibility, just as for [`Scene::dispatch_event`].
    pub fn dispatch(
        &mut self,
        scene: &Scene,
        target: &WidgetId,
        event: &mut E,
        state: &mut S,
    ) -> Option<DispatchResult> {
        if self.world != scene.world.id() {
            return None;
        }
        scene.dispatch_event(target, event, |ctx, event| {
            let Some(handler) = self
                .handlers
                .get_mut(&(ctx.current_target.clone(), ctx.phase))
            else {
                return EventControl::Continue;
            };
            if scene.entities.get(ctx.current_target) != Some(&handler.entity) {
                return EventControl::Continue;
            }
            (handler.callback)(ctx, event, state)
        })
    }

    /// Route to an observed semantic instance. Stale/foreign bindings return None
    /// without invoking handlers; current enabled/modal rules still apply.
    pub fn dispatch_target(
        &mut self,
        scene: &Scene,
        target: &SemanticTarget,
        event: &mut E,
        state: &mut S,
    ) -> Option<DispatchResult> {
        if !target.is_live(scene) {
            return None;
        }
        self.dispatch(scene, target.id(), event, state)
    }

    /// Dispatch to the current focused node; no eligible focus yields None.
    pub fn dispatch_focused(
        &mut self,
        scene: &Scene,
        event: &mut E,
        state: &mut S,
    ) -> Option<DispatchResult> {
        self.dispatch(scene, scene.focused()?, event, state)
    }
}

#[cfg(test)]
mod tests;
