//! Window-independent activation gestures. Use one controller per scene and
//! primary pointer; the caller supplies hit-tested or focused eligible widgets.
//! This is logical capture: the window backend must deliver pointer releases.
use crate::{Scene, WidgetId, WidgetState};
use bevy_ecs::{entity::Entity, world::WorldId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationKey {
    Enter,
    Space,
}
impl ActivationKey {
    fn bit(self) -> u8 {
        match self {
            Self::Enter => 1,
            Self::Space => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationSource {
    Pointer,
    Keyboard(ActivationKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activation {
    pub widget: WidgetId,
    pub source: ActivationSource,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputResult {
    /// Pressed appearance changed, or an activation was emitted.
    pub changed: bool,
    pub activated: Option<Activation>,
}

#[derive(Debug)]
struct Target {
    id: WidgetId,
    entity: Entity,
    world: WorldId,
    capture: u8,
}
impl Target {
    fn new(scene: &mut Scene, id: Option<&WidgetId>, capture: u8) -> Option<Self> {
        let id = id?;
        if !scene.input_allowed(id) {
            return None;
        }
        let entity = *scene.entities.get(id)?;
        scene
            .world
            .get_mut::<WidgetState>(entity)
            .unwrap()
            .set(capture, true);
        Some(Self {
            id: id.clone(),
            entity,
            world: scene.world.id(),
            capture,
        })
    }
    fn live(&self, scene: &Scene) -> bool {
        self.world == scene.world.id() && scene.entities.get(&self.id) == Some(&self.entity)
    }
    fn matches(&self, scene: &Scene, id: Option<&WidgetId>) -> bool {
        Some(&self.id) == id && self.valid(scene)
    }
    fn valid(&self, scene: &Scene) -> bool {
        self.live(scene)
            && scene.input_allowed(&self.id)
            && scene
                .state(&self.id)
                .is_some_and(|state| state.0 & self.capture != 0)
    }
    fn pressed(&self, scene: &mut Scene, pressed: bool) -> bool {
        if !self.valid(scene) {
            return false;
        }
        let mut state = scene.world.get_mut::<WidgetState>(self.entity).unwrap();
        let changed = state.pressed() != pressed;
        if changed {
            state.set(WidgetState::PRESSED, pressed);
            state.set(WidgetState::PAINT_DIRTY, true);
        }
        changed
    }
    fn end(&self, scene: &mut Scene) -> bool {
        let changed = self.pressed(scene, false);
        if self.live(scene) {
            scene
                .world
                .get_mut::<WidgetState>(self.entity)
                .unwrap()
                .set(self.capture, false);
        }
        changed
    }
}

/// Press/release pairing with one activation per gesture. Pointer capture never
/// transfers to a sibling: leaving disarms it, re-entering re-arms it. Enter and
/// Space activate on matching release while the original widget remains focused.
/// Repeats, synthetic keys, disabled nodes, stale ECS generations and foreign
/// scenes cannot fire. Disabling a captured node irrevocably cancels that gesture.
///
/// Call [`Self::cancel`] when geometry or eligibility changes, and
/// [`Self::reset`] on window focus loss/suspension. A controller must not be
/// moved between scenes during a gesture. Dropping it does not clear scene flags.
///
/// ```
/// use zest_core::{Scene, input::ActivationState};
/// let mut scene = Scene::default();
/// scene.update(|ui| { ui.button("save", "Save"); })?;
/// let save = scene.roots()[0].clone();
/// let mut input = ActivationState::default();
/// input.pointer_press(&mut scene, Some(&save));
/// assert!(scene.state(&save).unwrap().pressed());
/// let action = input.pointer_release(&mut scene, Some(&save)).activated.unwrap();
/// assert_eq!(action.widget, save);
/// # Ok::<(), zest_core::ReconcileError>(())
/// ```
#[derive(Debug, Default)]
pub struct ActivationState {
    pointer: Option<Target>,
    keyboard: Option<(Target, ActivationKey)>,
    pointer_down: bool,
    held_keys: u8,
}
impl ActivationState {
    /// Cached pointer target. Reconcile scene/focus/availability changes before
    /// querying it; revoked ownership cannot activate even before reconciliation.
    pub fn captured(&self) -> Option<&WidgetId> {
        self.pointer.as_ref().map(|target| &target.id)
    }
    pub fn pointer_down(&self) -> bool {
        self.pointer_down
    }
    /// Clear captures for removed/replaced/disabled nodes and keyboard focus changes.
    /// Call after scene reconciliation, before painting or delivering more input.
    pub fn reconcile(&mut self, scene: &mut Scene) -> bool {
        let mut changed = false;
        if self
            .pointer
            .as_ref()
            .is_some_and(|target| !target.valid(scene))
            && let Some(target) = self.pointer.take()
        {
            changed |= target.end(scene);
        }
        if self
            .keyboard
            .as_ref()
            .is_some_and(|(target, _)| !target.valid(scene) || scene.focused() != Some(&target.id))
            && let Some((target, _)) = self.keyboard.take()
        {
            changed |= target.end(scene);
        }
        changed
    }
    pub fn pointer_press(&mut self, scene: &mut Scene, hit: Option<&WidgetId>) -> InputResult {
        let mut changed = self.reconcile(scene);
        if self.pointer_down {
            return InputResult {
                changed,
                activated: None,
            };
        }
        changed |= self.cancel(scene);
        self.pointer_down = true;
        self.pointer = Target::new(scene, hit, WidgetState::POINTER_CAPTURE);
        if let Some(target) = &self.pointer {
            changed |= target.pressed(scene, true);
        }
        InputResult {
            changed,
            activated: None,
        }
    }
    pub fn pointer_move(&mut self, scene: &mut Scene, hit: Option<&WidgetId>) -> bool {
        let mut changed = self.reconcile(scene);
        if let Some(target) = &self.pointer {
            changed |= target.pressed(scene, target.matches(scene, hit));
        }
        changed
    }
    pub fn pointer_release(&mut self, scene: &mut Scene, hit: Option<&WidgetId>) -> InputResult {
        let mut changed = self.reconcile(scene);
        self.pointer_down = false;
        let activated = self.pointer.take().and_then(|target| {
            let matches = target.matches(scene, hit);
            changed |= target.end(scene);
            matches.then_some(Activation {
                widget: target.id,
                source: ActivationSource::Pointer,
            })
        });
        InputResult {
            changed: changed || activated.is_some(),
            activated,
        }
    }
    /// `focused` must be eligible and equal the scene's current focus. Track
    /// releases even when modifiers make `focused` ineligible (`None`). Synthetic
    /// events do not alter key latches; pair focus loss with `reset` instead.
    pub fn key(
        &mut self,
        scene: &mut Scene,
        key: ActivationKey,
        pressed: bool,
        repeat: bool,
        synthetic: bool,
        focused: Option<&WidgetId>,
    ) -> InputResult {
        if synthetic {
            return InputResult::default();
        }
        let mut changed = self.reconcile(scene);
        let bit = key.bit();
        if pressed {
            let held = self.held_keys & bit != 0;
            self.held_keys |= bit;
            if !held
                && !repeat
                && !self.pointer_down
                && self.keyboard.is_none()
                && focused == scene.focused()
                && let Some(target) = Target::new(scene, focused, WidgetState::KEY_CAPTURE)
            {
                changed |= target.pressed(scene, true);
                self.keyboard = Some((target, key));
            }
            return InputResult {
                changed,
                activated: None,
            };
        }
        self.held_keys &= !bit;
        let activated = if self
            .keyboard
            .as_ref()
            .is_some_and(|(_, armed)| *armed == key)
        {
            let (target, _) = self.keyboard.take().unwrap();
            let matches = target.matches(scene, focused);
            changed |= target.end(scene);
            matches.then_some(Activation {
                widget: target.id,
                source: ActivationSource::Keyboard(key),
            })
        } else {
            None
        };
        InputResult {
            changed: changed || activated.is_some(),
            activated,
        }
    }
    /// Cancel gestures while retaining held-input latches. A repeat or delayed
    /// release cannot begin a new gesture after a layout/eligibility change.
    pub fn cancel(&mut self, scene: &mut Scene) -> bool {
        let mut changed = false;
        if let Some(target) = self.pointer.take() {
            changed |= target.end(scene);
        }
        if let Some((target, _)) = self.keyboard.take() {
            changed |= target.end(scene);
        }
        changed
    }
    /// Cancel and forget held inputs after the window loses input ownership.
    pub fn reset(&mut self, scene: &mut Scene) -> bool {
        let changed = self.cancel(scene);
        self.pointer_down = false;
        self.held_keys = 0;
        changed
    }
}

#[cfg(test)]
mod tests;
