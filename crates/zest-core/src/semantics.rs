//! Read-only semantic observations for inspection and headless UI queries.
//! Platform accessibility bridges, custom roles/names and actions are separate.
use crate::{Node, NodeKind, Scene, WidgetId};
use std::{collections::HashMap, fmt, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticRole {
    Group,
    Button,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticState {
    pub enabled: bool,
    pub focused: bool,
    pub hovered: bool,
    pub pressed: bool,
    /// Effective availability and membership in the innermost modal scope.
    /// Geometry/visibility is not included; combine with current layout bounds.
    pub input_allowed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticNode {
    pub id: WidgetId,
    pub role: SemanticRole,
    /// Current scene label, matched exactly; containers initially have no name.
    pub name: String,
    pub parent: Option<WidgetId>,
    pub children: Vec<WidgetId>,
    pub state: SemanticState,
}

/// Exact, case-sensitive filters combined with AND. Missing filters match any
/// value. within includes the scope node itself; an unknown scope matches none.
/// Disabled/modal-blocked nodes are included unless explicitly filtered out.
#[derive(Clone, Copy, Debug, Default)]
pub struct SemanticQuery<'a> {
    role: Option<SemanticRole>,
    name: Option<&'a str>,
    id: Option<&'a WidgetId>,
    scope: Option<&'a WidgetId>,
    enabled: Option<bool>,
    input_allowed: Option<bool>,
}
impl<'a> SemanticQuery<'a> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn role(mut self, role: SemanticRole) -> Self {
        self.role = Some(role);
        self
    }
    pub fn named(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }
    pub fn id(mut self, id: &'a WidgetId) -> Self {
        self.id = Some(id);
        self
    }
    pub fn within(mut self, scope: &'a WidgetId) -> Self {
        self.scope = Some(scope);
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }
    pub fn input_allowed(mut self, allowed: bool) -> Self {
        self.input_allowed = Some(allowed);
        self
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FindError {
    NotFound,
    /// Matching identities in declaration preorder; never silently picks one.
    Ambiguous(Vec<WidgetId>),
}
impl fmt::Display for FindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("semantic query matched no widgets"),
            Self::Ambiguous(ids) => {
                write!(f, "semantic query matched {} widgets: {ids:?}", ids.len())
            }
        }
    }
}
impl std::error::Error for FindError {}

#[derive(Debug)]
struct SnapshotData {
    nodes: Vec<SemanticNode>,
    index: HashMap<WidgetId, usize>,
    roots: Vec<WidgetId>,
    modal_root: Option<WidgetId>,
}
/// An immutable semantic observation. Clones share owned data, with no borrow of
/// the scene and no ECS entities exposed. Old snapshots remain readable after
/// reconciliation. IDs identify paths, not generations: query a fresh snapshot
/// before input/actions and validate current eligibility and geometry separately.
/// This is not an OS accessibility tree or an input/activation implementation.
#[derive(Clone, Debug)]
pub struct SemanticSnapshot(Arc<SnapshotData>);
impl SemanticSnapshot {
    pub fn nodes(&self) -> &[SemanticNode] {
        &self.0.nodes
    }
    pub fn roots(&self) -> &[WidgetId] {
        &self.0.roots
    }
    pub fn modal_root(&self) -> Option<&WidgetId> {
        self.0.modal_root.as_ref()
    }
    pub fn node(&self, id: &WidgetId) -> Option<&SemanticNode> {
        self.0.index.get(id).map(|&index| &self.0.nodes[index])
    }
    /// All matches in declaration preorder. This initial query scans the nodes.
    pub fn find_all(&self, query: SemanticQuery<'_>) -> Vec<&SemanticNode> {
        if query.scope.is_some_and(|scope| self.node(scope).is_none()) {
            return Vec::new();
        }
        self.nodes()
            .iter()
            .filter(|node| {
                query.role.is_none_or(|role| role == node.role)
                    && query.name.is_none_or(|name| name == node.name)
                    && query.id.is_none_or(|id| id == &node.id)
                    && query
                        .scope
                        .is_none_or(|scope| node.id.0.starts_with(&scope.0))
                    && query
                        .enabled
                        .is_none_or(|enabled| enabled == node.state.enabled)
                    && query
                        .input_allowed
                        .is_none_or(|allowed| allowed == node.state.input_allowed)
            })
            .collect()
    }
    /// Require exactly one match. No matches or ambiguity return a diagnostic
    /// error rather than falling back to the first widget.
    pub fn find(&self, query: SemanticQuery<'_>) -> Result<&SemanticNode, FindError> {
        let matches = self.find_all(query);
        match matches.as_slice() {
            [] => Err(FindError::NotFound),
            [node] => Ok(node),
            _ => Err(FindError::Ambiguous(
                matches.into_iter().map(|node| node.id.clone()).collect(),
            )),
        }
    }
}
impl Scene {
    /// Copy the current hierarchy, role/name and interaction state in declaration
    /// preorder. Buttons map to Button and containers to Group. Identity-only
    /// scopes are omitted. Disabled/modal-blocked nodes remain in the tree.
    /// Reads do not consume paint flags or alter input/layout state.
    ///
    /// ```
    /// use zest_core::{Scene, semantics::{SemanticQuery, SemanticRole}};
    /// let mut scene = Scene::default();
    /// scene.update(|ui| { ui.container("toolbar", |ui| { ui.button("save", "Save"); }); })?;
    /// let snapshot = scene.semantic_snapshot();
    /// let save = snapshot.find(SemanticQuery::new().role(SemanticRole::Button).named("Save"))?;
    /// assert!(save.state.enabled && save.state.input_allowed);
    /// assert_eq!(snapshot.node(save.parent.as_ref().unwrap()).unwrap().role, SemanticRole::Group);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn semantic_snapshot(&self) -> SemanticSnapshot {
        let roots = self.roots().to_vec();
        let mut pending: Vec<_> = roots.iter().rev().cloned().collect();
        let mut nodes = Vec::with_capacity(self.entities.len());
        let mut index = HashMap::with_capacity(self.entities.len());
        while let Some(id) = pending.pop() {
            let node = self.world.get::<Node>(self.entities[&id]).unwrap();
            let children = self.children(&id).unwrap().to_vec();
            pending.extend(children.iter().rev().cloned());
            let state = self.state(&id).unwrap();
            index.insert(id.clone(), nodes.len());
            nodes.push(SemanticNode {
                role: match id.kind() {
                    NodeKind::Button => SemanticRole::Button,
                    NodeKind::Container => SemanticRole::Group,
                },
                name: node.label.clone(),
                parent: node.parent.clone(),
                children,
                state: SemanticState {
                    enabled: state.enabled(),
                    focused: state.focused(),
                    hovered: state.hovered(),
                    pressed: state.pressed(),
                    input_allowed: self.input_allowed(&id),
                },
                id,
            });
        }
        SemanticSnapshot(Arc::new(SnapshotData {
            nodes,
            index,
            roots,
            modal_root: self.modal_root().cloned(),
        }))
    }
}

#[cfg(test)]
mod tests;
