//! Retained no-wrap Flexbox layout keyed by scene WidgetId. The scene remains
//! window/GPU independent. Synchronization validates every supplied style before
//! changing the layout tree. Computation publishes immutable logical bounds;
//! paint-only scene changes reuse the retained tree, caches and snapshot.
//!
//! ```
//! use zest_core::{Scene, geometry::{Rect, Size}};
//! use zest_layout::{LayoutTree, Length, Style};
//! let mut scene = Scene::default();
//! let mut button = None;
//! scene.update(|ui| button = Some(ui.button("save", "Save")))?;
//! let mut layout = LayoutTree::default();
//! layout.sync(&scene, |_| Style {
//!     width: Length::Px(100.0), height: Length::Px(24.0), ..Default::default()
//! })?;
//! let first = layout.compute(Size::new(400.0, 200.0))?;
//! assert_eq!(first.snapshot.bounds(&button.unwrap()), Some(Rect::from_xywh(0.0, 0.0, 100.0, 24.0)));
//! let cached = layout.compute(Size::new(400.0, 200.0))?;
//! assert!(cached.snapshot.same_snapshot(&first.snapshot));
//! assert!(!cached.stats.performed);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
mod style;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};
pub use style::{Align, Axis, Edges, Justify, Length, Style};
use taffy::{AvailableSpace, NodeId, TaffyTree};
use zest_core::{
    Scene, WidgetId,
    geometry::{Logical, Rect, Size},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutError {
    InvalidStyle {
        widget: WidgetId,
        field: &'static str,
    },
    InvalidViewport,
    InvalidGeometry(WidgetId),
    Engine(String),
}
impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStyle { widget, field } => {
                write!(f, "invalid layout {field} for {widget:?}")
            }
            Self::InvalidViewport => {
                f.write_str("viewport must fit finite nonnegative f32 logical dimensions")
            }
            Self::InvalidGeometry(widget) => {
                write!(
                    f,
                    "nonfinite coordinates or negative computed size for {widget:?}"
                )
            }
            Self::Engine(error) => write!(f, "layout engine: {error}"),
        }
    }
}
impl std::error::Error for LayoutError {}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncStats {
    pub created: usize,
    pub removed: usize,
    pub restyled: usize,
    /// Updated child lists, including the private viewport root.
    pub children_changed: usize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComputeStats {
    pub performed: bool,
    /// Calls to intrinsic content measurement; not total Taffy node visits.
    pub measured_leaves: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutBox {
    pub id: WidgetId,
    /// Absolute, unrounded logical bounds relative to the viewport origin.
    pub bounds: Rect<Logical>,
}
#[derive(Debug)]
struct SnapshotData {
    viewport: Size<Logical>,
    boxes: Vec<LayoutBox>,
    index: HashMap<WidgetId, usize>,
}
/// An immutable computed scene, in parent-before-child declaration order. Old
/// snapshots stay valid after sync/recompute; use the current one for input.
#[derive(Clone, Debug)]
pub struct LayoutSnapshot(Arc<SnapshotData>);
impl LayoutSnapshot {
    pub fn viewport(&self) -> Size<Logical> {
        self.0.viewport
    }
    pub fn boxes(&self) -> &[LayoutBox] {
        &self.0.boxes
    }
    pub fn bounds(&self, id: &WidgetId) -> Option<Rect<Logical>> {
        self.0
            .index
            .get(id)
            .map(|&index| self.0.boxes[index].bounds)
    }
    pub fn same_snapshot(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
#[derive(Clone, Debug)]
pub struct ComputedLayout {
    pub snapshot: LayoutSnapshot,
    pub stats: ComputeStats,
}
struct Entry {
    node: NodeId,
    style: Style,
    parent: Option<WidgetId>,
    children: Vec<WidgetId>,
}
struct Declaration {
    id: WidgetId,
    style: Style,
    parent: Option<WidgetId>,
    children: Vec<WidgetId>,
}
/// One retained layout tree per scene/window. Node IDs and Taffy storage stay
/// private. The viewport stacks scene roots in a stretched vertical column.
pub struct LayoutTree {
    tree: TaffyTree<Size<Logical>>,
    root: NodeId,
    entries: HashMap<WidgetId, Entry>,
    roots: Vec<WidgetId>,
    order: Vec<WidgetId>,
    viewport: Option<Size<Logical>>,
    snapshot: Option<LayoutSnapshot>,
}
impl Default for LayoutTree {
    fn default() -> Self {
        let mut tree = TaffyTree::new();
        tree.disable_rounding();
        let root = tree
            .new_leaf(Style::default().taffy())
            .expect("new viewport node");
        Self {
            tree,
            root,
            entries: HashMap::new(),
            roots: Vec::new(),
            order: Vec::new(),
            viewport: None,
            snapshot: None,
        }
    }
}
impl LayoutTree {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Latest snapshot, or None after a successful sync changes layout inputs.
    pub fn snapshot(&self) -> Option<&LayoutSnapshot> {
        self.snapshot.as_ref()
    }
    /// Projects the whole scene. Styles (including intrinsic leaf sizes) are
    /// supplied by the caller; hover/focus/labels alone do not dirty layout.
    /// Invalid styles or a panicking style callback leave the tree unchanged.
    pub fn sync(
        &mut self,
        scene: &Scene,
        mut style: impl FnMut(&WidgetId) -> Style,
    ) -> Result<SyncStats, LayoutError> {
        let mut declarations = Vec::with_capacity(scene.stats().nodes);
        let mut stack: Vec<_> = scene
            .roots()
            .iter()
            .rev()
            .map(|id| (id.clone(), None))
            .collect();
        while let Some((id, parent)) = stack.pop() {
            let style = style(&id);
            if let Some(field) = style.invalid_field() {
                return Err(LayoutError::InvalidStyle { widget: id, field });
            }
            let children = scene.children(&id).expect("declared scene node").to_vec();
            stack.extend(
                children
                    .iter()
                    .rev()
                    .map(|child| (child.clone(), Some(id.clone()))),
            );
            declarations.push(Declaration {
                id,
                style,
                parent,
                children,
            });
        }
        let retained: HashSet<_> = declarations.iter().map(|d| d.id.clone()).collect();
        let removed: Vec<_> = self
            .entries
            .keys()
            .filter(|id| !retained.contains(*id))
            .cloned()
            .collect();
        let mut stats = SyncStats::default();
        // After validation, all Taffy operations address privately owned nodes.
        for declaration in &declarations {
            match self.entries.get_mut(&declaration.id) {
                Some(entry) if entry.style != declaration.style => {
                    self.tree
                        .set_style(entry.node, declaration.style.taffy())
                        .expect("owned layout node");
                    self.tree
                        .set_node_context(entry.node, Some(declaration.style.intrinsic_size))
                        .expect("owned layout node");
                    entry.style = declaration.style;
                    stats.restyled += 1;
                }
                Some(_) => {}
                None => {
                    let node = self
                        .tree
                        .new_leaf_with_context(
                            declaration.style.taffy(),
                            declaration.style.intrinsic_size,
                        )
                        .expect("new layout node");
                    self.entries.insert(
                        declaration.id.clone(),
                        Entry {
                            node,
                            style: declaration.style,
                            parent: declaration.parent.clone(),
                            children: Vec::new(),
                        },
                    );
                    stats.created += 1;
                }
            }
        }
        for declaration in &declarations {
            let entry = &self.entries[&declaration.id];
            if entry.children != declaration.children {
                let children: Vec<_> = declaration
                    .children
                    .iter()
                    .map(|id| self.entries[id].node)
                    .collect();
                self.tree
                    .set_children(entry.node, &children)
                    .expect("owned layout children");
                self.entries
                    .get_mut(&declaration.id)
                    .unwrap()
                    .children
                    .clone_from(&declaration.children);
                stats.children_changed += 1;
            }
            self.entries
                .get_mut(&declaration.id)
                .unwrap()
                .parent
                .clone_from(&declaration.parent);
        }
        if self.roots != scene.roots() {
            let roots: Vec<_> = scene
                .roots()
                .iter()
                .map(|id| self.entries[id].node)
                .collect();
            self.tree
                .set_children(self.root, &roots)
                .expect("owned layout roots");
            self.roots = scene.roots().to_vec();
            stats.children_changed += 1;
        }
        for id in removed {
            let entry = self.entries.remove(&id).unwrap();
            self.tree
                .remove(entry.node)
                .expect("owned removed layout node");
            stats.removed += 1;
        }
        if stats != SyncStats::default() {
            self.order = declarations.into_iter().map(|d| d.id).collect();
            self.snapshot = None;
        }
        Ok(stats)
    }
    /// Computes an unrounded snapshot, reusing retained Taffy caches. Identical
    /// inputs return the very same snapshot without calling the layout engine.
    pub fn compute(&mut self, viewport: Size<Logical>) -> Result<ComputedLayout, LayoutError> {
        if !style::valid_size(viewport) {
            return Err(LayoutError::InvalidViewport);
        }
        if let Some(snapshot) = &self.snapshot
            && self.viewport == Some(viewport)
        {
            return Ok(ComputedLayout {
                snapshot: snapshot.clone(),
                stats: ComputeStats::default(),
            });
        }
        self.snapshot = None;
        if self.viewport != Some(viewport) {
            let style = Style {
                width: Length::Px(viewport.width as f32),
                height: Length::Px(viewport.height as f32),
                ..Default::default()
            };
            self.tree
                .set_style(self.root, style.taffy())
                .expect("owned viewport node");
            self.viewport = Some(viewport);
        }
        let mut measured_leaves = 0;
        self.tree
            .compute_layout_with_measure(
                self.root,
                taffy::Size {
                    width: AvailableSpace::Definite(viewport.width as f32),
                    height: AvailableSpace::Definite(viewport.height as f32),
                },
                |input, _, context, style| {
                    taffy::compute_leaf_layout(
                        input,
                        style,
                        |_, _| 0.0,
                        |known, _| {
                            measured_leaves += 1;
                            let intrinsic = context.copied().unwrap_or(Size::new(0.0, 0.0));
                            taffy::Size {
                                width: known.width.unwrap_or(intrinsic.width as f32),
                                height: known.height.unwrap_or(intrinsic.height as f32),
                            }
                        },
                    )
                },
            )
            .map_err(|error| LayoutError::Engine(error.to_string()))?;
        let mut boxes: Vec<LayoutBox> = Vec::with_capacity(self.order.len());
        let mut index: HashMap<WidgetId, usize> = HashMap::with_capacity(self.order.len());
        for id in &self.order {
            let entry = &self.entries[id];
            let layout = self
                .tree
                .layout(entry.node)
                .expect("owned computed layout node");
            let (x, y) = entry
                .parent
                .as_ref()
                .map(|parent| {
                    let bounds = boxes[*index.get(parent).expect("parent precedes child")].bounds;
                    (bounds.origin.x, bounds.origin.y)
                })
                .unwrap_or((0.0, 0.0));
            let bounds = Rect::from_xywh(
                x + f64::from(layout.location.x),
                y + f64::from(layout.location.y),
                f64::from(layout.size.width),
                f64::from(layout.size.height),
            );
            if ![
                bounds.origin.x,
                bounds.origin.y,
                bounds.size.width,
                bounds.size.height,
            ]
            .into_iter()
            .all(f64::is_finite)
                || bounds.size.width < 0.0
                || bounds.size.height < 0.0
            {
                self.tree
                    .mark_dirty(self.root)
                    .expect("owned viewport node");
                return Err(LayoutError::InvalidGeometry(id.clone()));
            }
            index.insert(id.clone(), boxes.len());
            boxes.push(LayoutBox {
                id: id.clone(),
                bounds,
            });
        }
        let snapshot = LayoutSnapshot(Arc::new(SnapshotData {
            viewport,
            boxes,
            index,
        }));
        self.snapshot = Some(snapshot.clone());
        Ok(ComputedLayout {
            snapshot,
            stats: ComputeStats {
                performed: true,
                measured_leaves,
            },
        })
    }
}

#[cfg(test)]
mod tests;
