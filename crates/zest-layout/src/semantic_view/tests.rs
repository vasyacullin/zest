use super::*;
use crate::{ComputeStats, Edges, LayoutTree, Length, Style};
use zest_core::{Scene, geometry::Size, semantics::SemanticRole};

struct Fixture {
    scene: Scene,
    root: WidgetId,
    one: WidgetId,
    two: WidgetId,
    outside: WidgetId,
}
fn fixture() -> Fixture {
    let mut scene = Scene::default();
    scene
        .update(|ui| {
            ui.container("root", |ui| {
                ui.button("one", "Save");
                ui.button("two", "Save");
            });
            ui.button("outside", "Outside");
        })
        .unwrap();
    let root = scene.roots()[0].clone();
    let one = scene.children(&root).unwrap()[0].clone();
    let two = scene.children(&root).unwrap()[1].clone();
    let outside = scene.roots()[1].clone();
    Fixture {
        scene,
        root,
        one,
        two,
        outside,
    }
}
impl Fixture {
    fn style(&self, id: &WidgetId, clip: bool, offset: f64) -> Style {
        if id == &self.root {
            Style {
                width: Length::Px(80.0),
                height: Length::Px(40.0),
                padding: Edges::all(10.0),
                shrink: 0.0,
                clip_children: clip,
                scroll_offset: Point::new(0.0, offset),
                ..Default::default()
            }
        } else if id == &self.outside {
            Style {
                width: Length::Px(80.0),
                height: Length::Px(20.0),
                shrink: 0.0,
                ..Default::default()
            }
        } else {
            Style {
                width: Length::Px(100.0),
                height: Length::Px(30.0),
                shrink: 0.0,
                ..Default::default()
            }
        }
    }
    fn view(&self, tree: &mut LayoutTree, clip: bool, offset: f64) -> SemanticView {
        tree.sync(&self.scene, |id| self.style(id, clip, offset))
            .unwrap();
        tree.compute(Size::new(100.0, 100.0))
            .unwrap()
            .snapshot
            .semantic_view(&self.scene.semantic_snapshot())
    }
}
fn save() -> SemanticQuery<'static> {
    SemanticQuery::new()
        .role(SemanticRole::Button)
        .named("Save")
}

#[test]
fn joins_roles_and_names_to_projected_bounds_clips_and_shared_snapshots() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let paint = f.scene.paint_invalidation();
    let view = f.view(&mut tree, true, 0.0);
    let clone = view.clone();
    assert!(view.layout().same_snapshot(clone.layout()));
    assert!(view.semantics().same_snapshot(clone.semantics()));
    assert_eq!(view.nodes().len(), 4);
    assert_eq!(view.nodes().next_back().unwrap().semantic.id, f.outside);
    let one = view.node(&f.one).unwrap();
    assert_eq!(one.semantic.role, SemanticRole::Button);
    assert_eq!(one.semantic.name, "Save");
    assert_eq!(
        one.geometry.unwrap().bounds,
        Rect::from_xywh(10.0, 10.0, 100.0, 30.0)
    );
    assert_eq!(
        one.visible_bounds(),
        Some(Rect::from_xywh(10.0, 10.0, 70.0, 30.0))
    );
    assert!(one.is_visible());
    assert_eq!(f.scene.paint_invalidation(), paint);
}

#[test]
fn missing_geometry_remains_observable_but_is_not_visible_or_hittable() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    tree.sync(&Scene::default(), |_| Style::default()).unwrap();
    let empty = tree.compute(Size::new(100.0, 100.0)).unwrap().snapshot;
    let missing = empty.semantic_view(&f.scene.semantic_snapshot());
    assert_eq!(missing.nodes().len(), 4);
    assert!(
        missing
            .find(SemanticQuery::new().id(&f.one))
            .unwrap()
            .geometry
            .is_none()
    );
    assert_eq!(
        missing.find_visible(save()).unwrap_err(),
        FindError::NotFound
    );
    assert!(missing.find_visible_target(save()).is_err());
    assert!(missing.hit_test(Point::new(20.0, 20.0), |_| true).is_none());
    let complete = f.view(&mut tree, true, 0.0);
    let no_semantics = complete
        .layout()
        .semantic_view(&Scene::default().semantic_snapshot());
    assert!(
        no_semantics
            .hit_test(Point::new(20.0, 20.0), |_| true)
            .is_none()
    );
    assert!(no_semantics.node(&f.one).is_none());
}

#[test]
fn visible_filter_precedes_uniqueness_and_targets_keep_semantic_instance_bindings() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let view = f.view(&mut tree, true, 0.0);
    assert_eq!(
        view.find(save()).unwrap_err(),
        FindError::Ambiguous(vec![f.one.clone(), f.two.clone()])
    );
    assert_eq!(view.find_all(save()).len(), 2);
    assert_eq!(view.find_visible(save()).unwrap().semantic.id, f.one);
    assert_eq!(
        view.find_visible_target(save()).unwrap(),
        view.semantics().target(&f.one).unwrap()
    );
    let hidden = view.find(SemanticQuery::new().id(&f.two)).unwrap();
    assert!(hidden.geometry.is_some() && !hidden.is_visible());
    assert_eq!(
        view.find_visible(save().within(&f.two)).unwrap_err(),
        FindError::NotFound
    );
    assert_eq!(
        view.find_visible(SemanticQuery::new().id(&f.root))
            .unwrap()
            .semantic
            .role,
        SemanticRole::Group
    );
}

#[test]
fn scrolling_changes_visible_matches_without_relayout_or_mutating_old_views() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let old = f.view(&mut tree, true, 0.0);
    tree.sync(&f.scene, |id| f.style(id, true, 40.0)).unwrap();
    let computed = tree.compute(Size::new(100.0, 100.0)).unwrap();
    assert_eq!(computed.stats, ComputeStats::default());
    let new = computed
        .snapshot
        .semantic_view(&f.scene.semantic_snapshot());
    assert_eq!(old.find_visible(save()).unwrap().semantic.id, f.one);
    assert_eq!(new.find_visible(save()).unwrap().semantic.id, f.two);
    assert!(!new.node(&f.one).unwrap().is_visible());
    let two = new.node(&f.two).unwrap().geometry.unwrap();
    assert_eq!(two.bounds.origin.y, 0.0);
    assert_eq!(two.layout_bounds.origin.y, 40.0);
    assert_eq!(
        new.node(&f.root).unwrap().geometry.unwrap().scroll.offset.y,
        40.0
    );
    assert_eq!(
        old.node(&f.root).unwrap().geometry.unwrap().scroll.offset.y,
        0.0
    );
}

#[test]
fn clip_policy_can_change_visibility_without_changing_node_bounds() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let clipped = f.view(&mut tree, true, 0.0);
    tree.sync(&f.scene, |id| f.style(id, false, 0.0)).unwrap();
    let computed = tree.compute(Size::new(100.0, 100.0)).unwrap();
    assert_eq!(computed.stats, ComputeStats::default());
    let open = computed
        .snapshot
        .semantic_view(&f.scene.semantic_snapshot());
    assert_eq!(
        clipped.node(&f.two).unwrap().geometry.unwrap().bounds,
        open.node(&f.two).unwrap().geometry.unwrap().bounds
    );
    assert!(!clipped.node(&f.two).unwrap().is_visible());
    assert!(open.node(&f.two).unwrap().is_visible());
    assert_eq!(
        open.find_visible(save()).unwrap_err(),
        FindError::Ambiguous(vec![f.one, f.two])
    );
}

#[test]
fn semantic_hit_policy_shares_reverse_paint_order_clips_and_half_open_edges() {
    let f = fixture();
    let mut tree = LayoutTree::default();
    let clipped = f.view(&mut tree, true, 0.0);
    let buttons = |node: &SemanticNode| node.role == SemanticRole::Button;
    assert_eq!(
        clipped
            .hit_test(Point::new(10.0, 10.0), buttons)
            .unwrap()
            .semantic
            .id,
        f.one
    );
    assert_eq!(
        clipped
            .hit_test(Point::new(20.0, 39.99), buttons)
            .unwrap()
            .semantic
            .id,
        f.one
    );
    assert_eq!(
        clipped
            .hit_test(Point::new(20.0, 40.0), buttons)
            .unwrap()
            .semantic
            .id,
        f.outside
    );
    assert!(clipped.hit_test(Point::new(80.0, 20.0), buttons).is_none());
    assert!(
        clipped
            .hit_test(Point::new(f64::NAN, 20.0), buttons)
            .is_none()
    );
    let open = f.view(&mut tree, false, 0.0);
    assert_eq!(
        open.hit_test(Point::new(20.0, 45.0), buttons)
            .unwrap()
            .semantic
            .id,
        f.outside
    );
    assert_eq!(
        open.hit_test(Point::new(20.0, 45.0), |node| buttons(node)
            && node.id != f.outside)
            .unwrap()
            .semantic
            .id,
        f.two
    );
    let zero = tree
        .compute(Size::new(0.0, 0.0))
        .unwrap()
        .snapshot
        .semantic_view(open.semantics());
    assert!(zero.nodes().all(|node| !node.is_visible()));
    assert!(
        zero.hit_test(Point::new(0.0, 0.0), |_| panic!("zero viewport"))
            .is_none()
    );
}

#[test]
fn visibility_and_current_input_eligibility_are_separate_policies() {
    let mut f = fixture();
    let mut tree = LayoutTree::default();
    let view = f.view(&mut tree, true, 0.0);
    f.scene.set_enabled(&f.one, false);
    let disabled = view.layout().semantic_view(&f.scene.semantic_snapshot());
    assert!(disabled.find_visible(save()).unwrap().is_visible());
    assert!(
        !disabled
            .find_visible(save())
            .unwrap()
            .semantic
            .state
            .input_allowed
    );
    assert_eq!(
        disabled
            .find_visible(save().input_allowed(true))
            .unwrap_err(),
        FindError::NotFound
    );
    f.scene.set_enabled(&f.one, true);
    f.scene.push_modal(&f.two);
    let modal = view.layout().semantic_view(&f.scene.semantic_snapshot());
    assert_eq!(
        modal.find(save().input_allowed(true)).unwrap().semantic.id,
        f.two
    );
    assert_eq!(
        modal.find_visible(save().input_allowed(true)).unwrap_err(),
        FindError::NotFound
    );
    assert!(
        modal
            .hit_test(Point::new(20.0, 20.0), |node| node.state.input_allowed)
            .is_none()
    );
    assert_eq!(
        modal
            .hit_test(Point::new(20.0, 20.0), |node| node.role
                == SemanticRole::Button)
            .unwrap()
            .semantic
            .id,
        f.one
    );
}

#[test]
fn reused_geometry_never_upgrades_stale_semantic_bindings_after_recreation() {
    let mut f = fixture();
    let mut tree = LayoutTree::default();
    let old = f.view(&mut tree, true, 0.0);
    let stale = old.find_visible_target(save()).unwrap();
    f.scene
        .update_children(&f.root, |ui| {
            ui.button("two", "Save");
        })
        .unwrap();
    f.scene
        .update_children(&f.root, |ui| {
            ui.button("one", "Save");
            ui.button("two", "Save");
        })
        .unwrap();
    let new = f.view(&mut tree, true, 0.0);
    assert!(old.layout().same_snapshot(new.layout()));
    assert_eq!(old.find_visible_target(save()).unwrap(), stale);
    assert!(!stale.is_live(&f.scene));
    let fresh = new.find_visible_target(save()).unwrap();
    assert!(fresh.is_live(&f.scene));
    assert_ne!(stale, fresh);
    assert_eq!(
        new.node(&f.one).unwrap().visible_bounds(),
        old.node(&f.one).unwrap().visible_bounds()
    );
}
