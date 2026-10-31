//! Persistent UI scene. Updates declare the whole scene; partial rebuilds are
//! not exposed yet. Widget keys are explicit, independent of labels and order.
use bevy_ecs::prelude::{Component, Entity, World};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Container,
    Button,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Segment {
    Scope(String),
    Node(NodeKind, String),
}

/// Full scoped identity: hash collisions cannot alias widgets because equality
/// checks the entire path. ECS entities are never exposed to applications.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WidgetId(Arc<[Segment]>);

/// Frequent flags share one component; toggling them never changes archetypes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetState(u8);
impl WidgetState {
    const HOVERED: u8 = 1;
    const FOCUSED: u8 = 2;
    const PAINT_DIRTY: u8 = 4;
    pub fn hovered(self) -> bool {
        self.0 & Self::HOVERED != 0
    }
    pub fn focused(self) -> bool {
        self.0 & Self::FOCUSED != 0
    }
    pub fn paint_dirty(self) -> bool {
        self.0 & Self::PAINT_DIRTY != 0
    }
    fn set(&mut self, flag: u8, value: bool) {
        if value {
            self.0 |= flag;
        } else {
            self.0 &= !flag;
        }
    }
}

#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct Node {
    label: String,
    parent: Option<WidgetId>,
    order: usize,
}
struct Declaration {
    id: WidgetId,
    node: Node,
}

/// Immediate builder; this initial API requires explicit keys.
#[derive(Default)]
pub struct Ui {
    path: Vec<Segment>,
    parent: Option<WidgetId>,
    order: usize,
    declarations: Vec<Declaration>,
}
impl Ui {
    /// Identity scope only; does not create a scene node.
    pub fn push_id(&mut self, key: impl Into<String>, build: impl FnOnce(&mut Self)) {
        self.path.push(Segment::Scope(key.into()));
        build(self);
        self.path.pop();
    }
    /// Declares a button; returns identity, not a click response.
    pub fn button(&mut self, key: impl Into<String>, label: impl Into<String>) -> WidgetId {
        self.declare(NodeKind::Button, key.into(), label.into())
    }
    pub fn container(&mut self, key: impl Into<String>, build: impl FnOnce(&mut Self)) -> WidgetId {
        let key = key.into();
        let id = self.declare(NodeKind::Container, key.clone(), String::new());
        let parent = self.parent.replace(id.clone());
        let order = self.order;
        self.order = 0;
        self.path.push(Segment::Node(NodeKind::Container, key));
        build(self);
        self.path.pop();
        self.parent = parent;
        self.order = order;
        id
    }
    fn declare(&mut self, kind: NodeKind, key: String, label: String) -> WidgetId {
        let mut path = self.path.clone();
        path.push(Segment::Node(kind, key));
        let id = WidgetId(path.into());
        self.declarations.push(Declaration {
            id: id.clone(),
            node: Node {
                label,
                parent: self.parent.clone(),
                order: self.order,
            },
        });
        self.order += 1;
        id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconcileError {
    DuplicateId(WidgetId),
}
impl fmt::Display for ReconcileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId(id) => write!(f, "duplicate widget identity: {id:?}"),
        }
    }
}
impl std::error::Error for ReconcileError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UpdateStats {
    pub created: usize,
    pub updated: usize,
    pub removed: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneStats {
    pub nodes: usize,
    pub archetypes: usize,
}

/// One persistent scene per window. Declaration errors leave it unchanged.
#[derive(Default)]
pub struct Scene {
    world: World,
    entities: HashMap<WidgetId, Entity>,
    focused: Option<WidgetId>,
}
impl Scene {
    pub fn update(&mut self, build: impl FnOnce(&mut Ui)) -> Result<UpdateStats, ReconcileError> {
        let mut ui = Ui::default();
        build(&mut ui);
        let mut seen = HashSet::new();
        for d in &ui.declarations {
            if !seen.insert(d.id.clone()) {
                return Err(ReconcileError::DuplicateId(d.id.clone()));
            }
        }
        let mut stats = UpdateStats::default();
        for Declaration { id, node } in ui.declarations {
            if let Some(&entity) = self.entities.get(&id) {
                if self.world.get::<Node>(entity) != Some(&node) {
                    *self.world.get_mut::<Node>(entity).expect("scene node") = node;
                    self.world
                        .get_mut::<WidgetState>(entity)
                        .expect("scene state")
                        .set(WidgetState::PAINT_DIRTY, true);
                    stats.updated += 1;
                }
            } else {
                let entity = self
                    .world
                    .spawn((node, WidgetState(WidgetState::PAINT_DIRTY)))
                    .id();
                self.entities.insert(id, entity);
                stats.created += 1;
            }
        }
        let removed: Vec<_> = self
            .entities
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        for id in removed {
            let entity = self.entities.remove(&id).expect("known node");
            self.world.despawn(entity);
            if self.focused.as_ref() == Some(&id) {
                self.focused = None;
            }
            stats.removed += 1;
        }
        Ok(stats)
    }
    pub fn state(&self, id: &WidgetId) -> Option<WidgetState> {
        self.entities
            .get(id)
            .and_then(|&e| self.world.get::<WidgetState>(e))
            .copied()
    }
    pub fn label(&self, id: &WidgetId) -> Option<&str> {
        self.entities
            .get(id)
            .and_then(|&e| self.world.get::<Node>(e))
            .map(|n| n.label.as_str())
    }
    pub fn set_hovered(&mut self, id: &WidgetId, hovered: bool) -> bool {
        let Some(&entity) = self.entities.get(id) else {
            return false;
        };
        let mut state = self
            .world
            .get_mut::<WidgetState>(entity)
            .expect("scene state");
        if state.hovered() != hovered {
            state.set(WidgetState::HOVERED, hovered);
            state.set(WidgetState::PAINT_DIRTY, true);
        }
        true
    }
    /// Unknown targets do not disturb the current focus.
    pub fn focus(&mut self, id: &WidgetId) -> bool {
        if !self.entities.contains_key(id) {
            return false;
        }
        if self.focused.as_ref() == Some(id) {
            return true;
        }
        self.clear_focus();
        let mut state = self
            .world
            .get_mut::<WidgetState>(self.entities[id])
            .expect("scene state");
        state.set(WidgetState::FOCUSED, true);
        state.set(WidgetState::PAINT_DIRTY, true);
        self.focused = Some(id.clone());
        true
    }
    pub fn clear_focus(&mut self) {
        if let Some(id) = self.focused.take() {
            let mut state = self
                .world
                .get_mut::<WidgetState>(self.entities[&id])
                .expect("scene state");
            state.set(WidgetState::FOCUSED, false);
            state.set(WidgetState::PAINT_DIRTY, true);
        }
    }
    pub fn focused(&self) -> Option<&WidgetId> {
        self.focused.as_ref()
    }
    pub fn stats(&self) -> SceneStats {
        SceneStats {
            nodes: self.entities.len(),
            archetypes: self.world.archetypes().len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reorder_and_conditional_insertion_preserve_identity_and_state() {
        let mut scene = Scene::default();
        let mut saved = None;
        scene
            .update(|ui| {
                ui.push_id("list", |ui| {
                    saved = Some(ui.button("b", "B"));
                });
            })
            .unwrap();
        let id = saved.unwrap();
        let entity = scene.entities[&id];
        scene.focus(&id);
        scene.set_hovered(&id, true);
        scene
            .update(|ui| {
                ui.button("conditional", "Optional");
                ui.push_id("list", |ui| {
                    ui.button("a", "A");
                    ui.button("b", "Renamed");
                });
            })
            .unwrap();
        assert_eq!(scene.entities[&id], entity);
        assert!(scene.state(&id).unwrap().focused());
        assert!(scene.state(&id).unwrap().hovered());
        assert_eq!(scene.label(&id), Some("Renamed"));
    }
    #[test]
    fn duplicate_validation_is_atomic() {
        let mut scene = Scene::default();
        scene
            .update(|ui| {
                ui.button("save", "Original");
            })
            .unwrap();
        let before = scene.stats();
        assert!(matches!(
            scene.update(|ui| {
                ui.button("save", "Changed");
                ui.button("save", "Duplicate");
            }),
            Err(ReconcileError::DuplicateId(_))
        ));
        assert_eq!(scene.stats(), before);
        assert_eq!(
            scene.label(scene.entities.keys().next().unwrap()),
            Some("Original")
        );
    }
    #[test]
    fn flags_do_not_migrate_archetypes_and_deletion_clears_focus() {
        let mut scene = Scene::default();
        let mut saved = None;
        scene
            .update(|ui| {
                saved = Some(ui.button("save", "Save"));
            })
            .unwrap();
        let id = saved.unwrap();
        let entity = scene.entities[&id];
        let archetype = scene.world.entity(entity).archetype().id();
        let stats = scene.stats();
        for hovered in [true, false, true] {
            scene.set_hovered(&id, hovered);
            scene.focus(&id);
        }
        assert_eq!(scene.stats(), stats);
        assert_eq!(scene.world.entity(entity).archetype().id(), archetype);
        assert_eq!(scene.update(|_| {}).unwrap().removed, 1);
        assert_eq!(scene.focused(), None);
        assert_eq!(scene.state(&id), None);
        scene
            .update(|ui| {
                ui.button("save", "Save");
            })
            .unwrap();
        assert_ne!(scene.entities[&id], entity);
        assert!(!scene.state(&id).unwrap().focused());
        assert!(!scene.state(&id).unwrap().hovered());
    }
    #[test]
    fn nested_scopes_are_stable_and_unchanged_updates_are_noops() {
        fn build(ui: &mut Ui) {
            ui.container("left", |ui| {
                ui.button("save", "Save");
            });
            ui.container("right", |ui| {
                ui.button("save", "Save");
            });
            ui.push_id("overlay", |ui| {
                ui.button("save", "Save");
            });
        }
        let mut scene = Scene::default();
        assert_eq!(scene.update(build).unwrap().created, 5);
        assert_eq!(scene.update(build).unwrap(), UpdateStats::default());
    }
}
