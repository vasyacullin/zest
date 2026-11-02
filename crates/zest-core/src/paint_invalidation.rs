use crate::{Scene, WidgetId, WidgetState};

/// Pending CPU paint work. Widgets are current dirty nodes in declaration
/// preorder, including disabled/modal-blocked nodes, without identity-only scopes.
/// Structure changes require repaint even when no nodes remain. This does not
/// contain damage rectangles or a cached display list; choose repaint bounds
/// using current geometry, or redraw the complete frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaintInvalidation {
    pub widgets: Vec<WidgetId>,
    /// Additions, removals or order changes since the last drain.
    pub structure_changed: bool,
}
impl PaintInvalidation {
    pub fn is_empty(&self) -> bool {
        self.widgets.is_empty() && !self.structure_changed
    }
}

impl Scene {
    /// Whether scene structure or any node's appearance needs paint. Reads the
    /// current flags without consuming them; this initial query scans nodes.
    pub fn needs_paint(&self) -> bool {
        self.paint_structure_changed
            || self
                .entities
                .values()
                .any(|&entity| self.world.get::<WidgetState>(entity).unwrap().paint_dirty())
    }

    /// Mark a known widget's paint dirty after changing application-owned visual
    /// state (selection, theme, etc.). Enabled/modal eligibility is irrelevant to
    /// invalidation. Returns false for an unknown ID; repeated marks coalesce.
    /// Does not change geometry, focus, input ownership or ECS archetypes.
    pub fn invalidate_paint(&mut self, id: &WidgetId) -> bool {
        let Some(&entity) = self.entities.get(id) else {
            return false;
        };
        self.world
            .get_mut::<WidgetState>(entity)
            .unwrap()
            .set(WidgetState::PAINT_DIRTY, true);
        true
    }

    /// Copy pending invalidation without consuming it. This traverses the full
    /// current hierarchy; removed nodes are represented by structure_changed.
    pub fn paint_invalidation(&self) -> PaintInvalidation {
        let mut widgets = Vec::new();
        let mut pending: Vec<_> = self.roots().iter().rev().collect();
        while let Some(id) = pending.pop() {
            if self.state(id).unwrap().paint_dirty() {
                widgets.push(id.clone());
            }
            if let Some(children) = self.children(id) {
                pending.extend(children.iter().rev());
            }
        }
        PaintInvalidation {
            widgets,
            structure_changed: self.paint_structure_changed,
        }
    }

    /// Drain current paint flags and structural invalidation, preserving every
    /// other interaction bit. Changes made after this call accumulate normally.
    /// This is a CPU drain, not GPU acknowledgement: retain the returned work if
    /// consuming before rendering, and retry it on failure/suspension. Alternatively
    /// inspect with paint_invalidation and drain only after successful rendering,
    /// with no scene changes between producing that frame and draining its flags.
    /// Window exposure, resize and device recovery can require paint even when
    /// the scene is clean; this API does not schedule redraws automatically.
    ///
    /// ```
    /// use zest_core::Scene;
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.button("save", "Save"); })?;
    /// assert!(scene.needs_paint());
    /// let work = scene.take_paint_invalidation();
    /// assert!(work.structure_changed && work.widgets.len() == 1);
    /// assert!(!scene.needs_paint());
    /// scene.update(|_| {})?;
    /// let work = scene.take_paint_invalidation();
    /// assert!(work.structure_changed && work.widgets.is_empty());
    /// assert!(!work.is_empty()); // The old button still needs clearing.
    /// # Ok::<(), zest_core::ReconcileError>(())
    /// ```
    pub fn take_paint_invalidation(&mut self) -> PaintInvalidation {
        let work = self.paint_invalidation();
        for id in &work.widgets {
            self.world
                .get_mut::<WidgetState>(self.entities[id])
                .unwrap()
                .set(WidgetState::PAINT_DIRTY, false);
        }
        self.paint_structure_changed = false;
        work
    }
}

#[cfg(test)]
mod tests;
