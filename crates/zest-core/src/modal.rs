//! Modal input boundaries. Rendering, visibility and window lifecycle remain
//! integration concerns; this module owns scene membership and focus restoration.
use crate::{FocusDirection, Scene, WidgetId, WidgetState};
use bevy_ecs::entity::Entity;

#[derive(Clone)]
struct RememberedWidget {
    id: WidgetId,
    entity: Entity,
}
impl RememberedWidget {
    fn new(scene: &Scene, id: &WidgetId) -> Self {
        Self {
            id: id.clone(),
            entity: scene.entities[id],
        }
    }
    fn live(&self, scene: &Scene) -> bool {
        scene.entities.get(&self.id) == Some(&self.entity)
    }
}
pub(super) struct ModalFrame {
    root: RememberedWidget,
    previous: Option<RememberedWidget>,
}

impl Scene {
    /// Root of the innermost modal input scope. A scope includes the root itself
    /// and its descendants; the root may be a button or a container.
    pub fn modal_root(&self) -> Option<&WidgetId> {
        self.modals.last().map(|frame| &frame.root.id)
    }

    /// Whether a known, effectively enabled node belongs to the active input
    /// scope. Use this alongside current geometry when filtering hit targets.
    /// Modal blocking does not alter `WidgetState::enabled()` or layout.
    pub fn input_allowed(&self, id: &WidgetId) -> bool {
        self.state(id).is_some_and(|state| state.enabled())
            && self
                .modal_root()
                .is_none_or(|root| id.0.starts_with(&root.0))
    }

    /// Push a modal input scope and remember the current focus by entity
    /// generation. Nested roots must be enabled and inside the current scope.
    /// Invalid roots return false without changing state. Successful transitions
    /// cancel all gestures and clear hover outside the new scope. Retain focus
    /// inside it, or focus its first enabled button; an empty scope keeps no focus.
    /// Scope transitions mark the scene paint-dirty without invalidating layout.
    ///
    /// This manages input only: the caller paints overlays and filters visibility.
    /// Revalidate focus against viewport geometry after a successful transition.
    ///
    /// ```
    /// use zest_core::Scene;
    /// let mut scene = Scene::default();
    /// scene.update(|ui| {
    ///     ui.button("open", "Open");
    ///     ui.container("dialog", |ui| { ui.button("ok", "OK"); });
    /// })?;
    /// let open = scene.roots()[0].clone();
    /// let dialog = scene.roots()[1].clone();
    /// scene.focus(&open);
    /// assert!(scene.push_modal(&dialog));
    /// assert!(!scene.input_allowed(&open));
    /// assert!(scene.pop_modal());
    /// assert_eq!(scene.focused(), Some(&open));
    /// # Ok::<(), zest_core::ReconcileError>(())
    /// ```
    pub fn push_modal(&mut self, root: &WidgetId) -> bool {
        if !self.input_allowed(root) {
            return false;
        }
        let previous = self.focused().map(|id| RememberedWidget::new(self, id));
        self.modals.push(ModalFrame {
            root: RememberedWidget::new(self, root),
            previous: previous.clone(),
        });
        self.apply_modal_change(previous);
        if self.focused().is_none() {
            self.focus_next(FocusDirection::Forward, |_| true);
        }
        true
    }

    /// Pop the innermost scope, cancel gestures, and restore its saved focus if
    /// that exact entity is still enabled and allowed by the remaining scope.
    /// Otherwise leave focus empty. Returns false when there is no open scope.
    pub fn pop_modal(&mut self) -> bool {
        let Some(frame) = self.modals.pop() else {
            return false;
        };
        self.apply_modal_change(frame.previous);
        true
    }

    /// Reconciliation and availability changes close the first invalid scope
    /// and all scopes nested within it. A recreated key is a different root.
    pub(super) fn reconcile_modals(&mut self) {
        let invalid = self.modals.iter().position(|frame| {
            !frame.root.live(self) || !self.state(&frame.root.id).unwrap().enabled()
        });
        if let Some(index) = invalid {
            let previous = self.modals[index].previous.clone();
            self.modals.truncate(index);
            self.apply_modal_change(previous);
        }
    }

    fn apply_modal_change(&mut self, previous: Option<RememberedWidget>) {
        self.clear_focus();
        for (id, &entity) in &self.entities {
            let allowed = self.input_allowed(id);
            let mut state = self.world.get_mut::<WidgetState>(entity).unwrap();
            state.0 &=
                !(WidgetState::PRESSED | WidgetState::POINTER_CAPTURE | WidgetState::KEY_CAPTURE);
            if !allowed {
                state.set(WidgetState::HOVERED, false);
            }
            // Eligibility and caller-painted scope decoration can change even
            // on a node with no hover/focus/press flags.
            state.set(WidgetState::PAINT_DIRTY, true);
        }
        if let Some(previous) =
            previous.filter(|target| target.live(self) && self.input_allowed(&target.id))
        {
            self.focus(&previous.id);
        }
    }
}
