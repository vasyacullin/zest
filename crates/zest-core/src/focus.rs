use crate::{Node, NodeKind, Scene, WidgetId, WidgetState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDirection {
    Forward,
    Backward,
}

impl Scene {
    /// The widget's own setting, independent of disabled ancestors. This retained
    /// property survives declarations/reorders and resets when the node is removed.
    pub fn locally_enabled(&self, id: &WidgetId) -> Option<bool> {
        self.state(id)
            .map(|state| state.0 & WidgetState::LOCAL_DISABLED == 0)
    }

    /// Set availability for a widget and recompute only its subtree. Disabled
    /// ancestors override enabled children. Disabling immediately clears hover,
    /// focus, pressed state and capture ownership, even if re-enabled before the
    /// next input event. Re-enabling restores no interaction flags automatically.
    /// Disabling a modal root closes its scope and nested scopes, restoring focus
    /// when possible; that scope transition also invalidates paint scene-wide.
    ///
    /// Returns false for unknown IDs, true for known IDs (including no-ops).
    ///
    /// ```
    /// use zest_core::{FocusDirection, Scene};
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.button("save", "Save"); ui.button("delete", "Delete"); })?;
    /// let delete = scene.roots()[1].clone();
    /// scene.set_enabled(&delete, false);
    /// scene.focus_next(FocusDirection::Forward, |_| true);
    /// assert_eq!(scene.focused(), Some(&scene.roots()[0]));
    /// assert!(!scene.focus(&delete));
    /// # Ok::<(), zest_core::ReconcileError>(())
    /// ```
    pub fn set_enabled(&mut self, id: &WidgetId, enabled: bool) -> bool {
        let Some(&entity) = self.entities.get(id) else {
            return false;
        };
        let mut state = self.world.get_mut::<WidgetState>(entity).unwrap();
        if (state.0 & WidgetState::LOCAL_DISABLED == 0) == enabled {
            return true;
        }
        state.set(WidgetState::LOCAL_DISABLED, !enabled);
        let parent_disabled = self
            .world
            .get::<Node>(entity)
            .unwrap()
            .parent
            .as_ref()
            .is_some_and(|parent| !self.state(parent).unwrap().enabled());
        let mut pending = vec![(id.clone(), parent_disabled)];
        while let Some((id, inherited)) = pending.pop() {
            let mut state = self
                .world
                .get_mut::<WidgetState>(self.entities[&id])
                .unwrap();
            let disabled = inherited || state.0 & WidgetState::LOCAL_DISABLED != 0;
            if disabled != !state.enabled() {
                state.set(WidgetState::DISABLED, disabled);
                state.set(WidgetState::PAINT_DIRTY, true);
            }
            if disabled {
                state.0 &= !(WidgetState::HOVERED
                    | WidgetState::FOCUSED
                    | WidgetState::PRESSED
                    | WidgetState::POINTER_CAPTURE
                    | WidgetState::KEY_CAPTURE);
                if self.focused.as_ref() == Some(&id) {
                    self.focused = None;
                }
            }
            if let Some(children) = self.children.get(&Some(id)) {
                pending.extend(children.iter().cloned().map(|id| (id, disabled)));
            }
        }
        self.reconcile_modals();
        true
    }

    /// Cycle enabled buttons in scene declaration preorder, wrapping at either
    /// end. The predicate supplies viewport/visibility or application eligibility.
    /// Containers are skipped. If the old focus is excluded, start at the first
    /// (or last) eligible node; an empty order clears focus. Returns whether focus
    /// changed. No cached order can outlive a reconciliation.
    /// When a modal scope is open, traversal stays inside its innermost root.
    pub fn focus_next(
        &mut self,
        direction: FocusDirection,
        mut accepts: impl FnMut(&WidgetId) -> bool,
    ) -> bool {
        let mut pending: Vec<_> = match self.modal_root() {
            Some(root) => vec![root.clone()],
            None => self.roots().iter().rev().cloned().collect(),
        };
        let mut order = Vec::new();
        while let Some(id) = pending.pop() {
            let state = self.state(&id).unwrap();
            if !state.enabled() {
                continue;
            }
            if id.kind() == NodeKind::Button && accepts(&id) {
                order.push(id.clone());
            }
            if let Some(children) = self.children.get(&Some(id)) {
                pending.extend(children.iter().rev().cloned());
            }
        }
        let current = order.iter().position(|id| Some(id) == self.focused());
        let target = if order.is_empty() {
            None
        } else {
            let index = match direction {
                FocusDirection::Forward => current.map_or(0, |index| (index + 1) % order.len()),
                FocusDirection::Backward => current.map_or(order.len() - 1, |index| {
                    (index + order.len() - 1) % order.len()
                }),
            };
            Some(order[index].clone())
        };
        if self.focused() == target.as_ref() {
            return false;
        }
        if let Some(target) = target {
            self.focus(&target);
        } else {
            self.clear_focus();
        }
        true
    }
}
