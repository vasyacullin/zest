//! Synchronous typed events over the current scene hierarchy. Handlers borrow
//! the scene immutably; record scene edits/focus changes as intents and apply them
//! after dispatch. Input normalization and widget default behavior are separate.
use crate::{Node, Scene, WidgetId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventPhase {
    Capture,
    Target,
    Bubble,
}

/// Propagation and default behavior are independent. StopPropagation alone does
/// not cancel the default, even when it stops delivery before the target phase.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventControl {
    #[default]
    Continue,
    StopPropagation,
    PreventDefault,
    Consume,
}

#[derive(Clone, Copy, Debug)]
pub struct EventContext<'a> {
    pub target: &'a WidgetId,
    pub current_target: &'a WidgetId,
    pub phase: EventPhase,
    /// True if an earlier handler prevented the default. Later handlers cannot
    /// undo prevention by returning Continue.
    pub default_prevented: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DispatchResult {
    pub visits: usize,
    pub target_reached: bool,
    pub propagation_stopped: bool,
    pub default_prevented: bool,
}

impl Scene {
    /// Deliver to enabled/currently modal-eligible nodes in capture order
    /// (outermost ancestor to parent), then once to the target, then bubble back
    /// from parent to outermost ancestor. The active modal root is the inclusive
    /// boundary. Identity-only scopes are not event receivers.
    ///
    /// Returns None without calling handlers or changing the payload for unknown,
    /// disabled or modal-blocked targets. Choose positional targets from current
    /// geometry. This reads the current hierarchy synchronously, not a queued
    /// route or an old activation's entity generation. The scene cannot change
    /// during a handler, but application state and the typed payload can change.
    /// Nested synchronous dispatches have independent control/default state.
    ///
    /// Callers apply a default action only when dispatch returned Some and
    /// `default_prevented` is false. This method does not activate, focus, hover,
    /// capture a pointer or invalidate paint automatically.
    ///
    /// ```
    /// use zest_core::{Scene, event::{EventControl, EventPhase}};
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.container("panel", |ui| { ui.button("save", "Save"); }); })?;
    /// let panel = scene.roots()[0].clone();
    /// let save = scene.children(&panel).unwrap()[0].clone();
    /// let mut payload = Vec::new();
    /// let result = scene.dispatch_event(&save, &mut payload, |ctx, trace| {
    ///     trace.push(ctx.phase);
    ///     if ctx.current_target == &panel && ctx.phase == EventPhase::Capture {
    ///         EventControl::PreventDefault
    ///     } else { EventControl::Continue }
    /// }).unwrap();
    /// assert_eq!(payload, [EventPhase::Capture, EventPhase::Target, EventPhase::Bubble]);
    /// assert!(result.target_reached && result.default_prevented);
    /// # Ok::<(), zest_core::ReconcileError>(())
    /// ```
    pub fn dispatch_event<E>(
        &self,
        target: &WidgetId,
        event: &mut E,
        mut handler: impl FnMut(EventContext<'_>, &mut E) -> EventControl,
    ) -> Option<DispatchResult> {
        if !self.input_allowed(target) {
            return None;
        }
        let boundary = self.modal_root();
        let mut path = Vec::new();
        let mut current = Some(target.clone());
        while let Some(id) = current {
            if !self.input_allowed(&id) {
                return None;
            }
            let node = self.world.get::<Node>(*self.entities.get(&id)?)?;
            current = node.parent.clone();
            let at_boundary = boundary == Some(&id);
            path.push(id);
            if at_boundary {
                break;
            }
        }
        if boundary.is_some() && path.last() != boundary {
            return None;
        }
        let mut result = DispatchResult::default();
        let mut visit = |id: &WidgetId, phase: EventPhase, result: &mut DispatchResult| {
            result.visits += 1;
            result.target_reached |= phase == EventPhase::Target;
            let control = handler(
                EventContext {
                    target,
                    current_target: id,
                    phase,
                    default_prevented: result.default_prevented,
                },
                event,
            );
            result.default_prevented |= matches!(
                control,
                EventControl::PreventDefault | EventControl::Consume
            );
            result.propagation_stopped |= matches!(
                control,
                EventControl::StopPropagation | EventControl::Consume
            );
            result.propagation_stopped
        };
        for ancestor in path.iter().skip(1).rev() {
            if visit(ancestor, EventPhase::Capture, &mut result) {
                return Some(result);
            }
        }
        if visit(&path[0], EventPhase::Target, &mut result) {
            return Some(result);
        }
        for ancestor in path.iter().skip(1) {
            if visit(ancestor, EventPhase::Bubble, &mut result) {
                break;
            }
        }
        Some(result)
    }

    /// Route to the scene's current focus; no focused/eligible target yields None.
    pub fn dispatch_focused_event<E>(
        &self,
        event: &mut E,
        handler: impl FnMut(EventContext<'_>, &mut E) -> EventControl,
    ) -> Option<DispatchResult> {
        self.dispatch_event(self.focused()?, event, handler)
    }
}

#[cfg(test)]
mod tests;
