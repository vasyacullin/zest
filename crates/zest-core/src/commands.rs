//! Ordered scene intents for application after synchronous event dispatch.
use crate::semantics::SemanticTarget;
use crate::{Scene, WidgetId};
use bevy_ecs::{entity::Entity, world::WorldId};

struct Target {
    id: WidgetId,
    entity: Entity,
}
impl Target {
    fn new(scene: &Scene, id: &WidgetId) -> Option<Self> {
        Some(Self {
            id: id.clone(),
            entity: *scene.entities.get(id)?,
        })
    }
    fn live(&self, scene: &Scene) -> bool {
        scene.entities.get(&self.id) == Some(&self.entity)
    }
}
enum Command {
    Focus(Target),
    ClearFocus,
    SetEnabled(Target, bool),
    PushModal(Target),
    PopModal,
    InvalidatePaint(Target),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandStats {
    /// Accepted commands, including no-ops.
    pub applied: usize,
    /// Stale targets or commands rejected by current scene input/scope rules.
    pub skipped: usize,
    /// Scene state/paint changed. Local availability changes count even when
    /// inherited disability already hides their visual effect.
    pub changed: bool,
}

/// A queue bound to one scene. Targeted commands capture entity generations when
/// queued; removal/recreation never transfers them to a new node with the same ID.
/// Disabled/modal-blocked targets may be queued; apply checks current eligibility.
/// Commands run in insertion order and are not a transaction. Earlier commands
/// can change whether later commands are allowed. ClearFocus and PopModal act on
/// the current focus/stack at application time, rather than a remembered target.
///
/// Applying to a foreign scene returns None and retains the complete queue.
/// Applying to its owner drains all commands, including rejected ones. No layout,
/// hit testing, controller reconciliation or rendering runs automatically.
/// Reconcile input/geometry after application before the next positional event.
///
/// ```
/// use zest_core::{Scene, commands::SceneCommands, event::EventControl};
/// let mut scene = Scene::default();
/// scene.update(|ui| { ui.button("save", "Save"); })?;
/// let save = scene.roots()[0].clone();
/// let mut commands = SceneCommands::new(&scene);
/// scene.dispatch_event(&save, &mut (), |ctx, _| {
///     commands.set_enabled(ctx.scene(), ctx.target, false);
///     EventControl::Continue
/// });
/// assert!(scene.state(&save).unwrap().enabled());
/// let stats = commands.apply(&mut scene).unwrap();
/// assert_eq!(stats.applied, 1);
/// assert!(stats.changed && !scene.state(&save).unwrap().enabled());
/// # Ok::<(), zest_core::ReconcileError>(())
/// ```
pub struct SceneCommands {
    world: WorldId,
    pending: Vec<Command>,
}
impl SceneCommands {
    pub fn new(scene: &Scene) -> Self {
        Self {
            world: scene.world.id(),
            pending: Vec::new(),
        }
    }
    pub fn len(&self) -> usize {
        self.pending.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    pub fn clear(&mut self) {
        self.pending.clear();
    }
    fn target(&self, scene: &Scene, id: &WidgetId) -> Option<Target> {
        if self.world != scene.world.id() {
            return None;
        }
        Target::new(scene, id)
    }
    /// Queue a known node; unknown IDs/foreign scenes return false unchanged.
    pub fn focus(&mut self, scene: &Scene, id: &WidgetId) -> bool {
        let Some(target) = self.target(scene, id) else {
            return false;
        };
        self.pending.push(Command::Focus(target));
        true
    }
    /// Queue focus for an observed semantic instance. Rejected/stale bindings
    /// leave the queue unchanged. Current input eligibility is checked on apply.
    pub fn focus_target(&mut self, scene: &Scene, target: &SemanticTarget) -> bool {
        target.is_live(scene) && self.focus(scene, target.id())
    }
    pub fn clear_focus(&mut self) {
        self.pending.push(Command::ClearFocus);
    }
    pub fn set_enabled(&mut self, scene: &Scene, id: &WidgetId, enabled: bool) -> bool {
        let Some(target) = self.target(scene, id) else {
            return false;
        };
        self.pending.push(Command::SetEnabled(target, enabled));
        true
    }
    pub fn push_modal(&mut self, scene: &Scene, root: &WidgetId) -> bool {
        let Some(target) = self.target(scene, root) else {
            return false;
        };
        self.pending.push(Command::PushModal(target));
        true
    }
    pub fn pop_modal(&mut self) {
        self.pending.push(Command::PopModal);
    }
    pub fn invalidate_paint(&mut self, scene: &Scene, id: &WidgetId) -> bool {
        let Some(target) = self.target(scene, id) else {
            return false;
        };
        self.pending.push(Command::InvalidatePaint(target));
        true
    }
    pub fn apply(&mut self, scene: &mut Scene) -> Option<CommandStats> {
        if self.world != scene.world.id() {
            return None;
        }
        let mut stats = CommandStats::default();
        for command in self.pending.drain(..) {
            let (accepted, changed) = match command {
                Command::Focus(target) if target.live(scene) => {
                    let changed = scene.focused() != Some(&target.id);
                    let accepted = scene.focus(&target.id);
                    (accepted, accepted && changed)
                }
                Command::ClearFocus => {
                    let changed = scene.focused().is_some();
                    scene.clear_focus();
                    (true, changed)
                }
                Command::SetEnabled(target, enabled) if target.live(scene) => {
                    let changed = scene.locally_enabled(&target.id) != Some(enabled);
                    (scene.set_enabled(&target.id, enabled), changed)
                }
                Command::PushModal(target) if target.live(scene) => {
                    let accepted = scene.push_modal(&target.id);
                    (accepted, accepted)
                }
                Command::PopModal => {
                    let accepted = scene.pop_modal();
                    (accepted, accepted)
                }
                Command::InvalidatePaint(target) if target.live(scene) => {
                    let changed = !scene.state(&target.id).unwrap().paint_dirty();
                    (scene.invalidate_paint(&target.id), changed)
                }
                _ => (false, false),
            };
            if accepted {
                stats.applied += 1;
            } else {
                stats.skipped += 1;
            }
            stats.changed |= changed;
        }
        Some(stats)
    }
}

#[cfg(test)]
mod tests;
