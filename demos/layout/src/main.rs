use zest::{
    Scene,
    geometry::{Point, Size},
    layout::{Axis, Edges, LayoutTree, Length, Style},
    semantics::{SemanticQuery, SemanticRole},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::default();
    let (mut toolbar, mut first, mut second) = (None, None, None);
    scene.update(|ui| {
        toolbar = Some(ui.container("toolbar", |ui| {
            first = Some(ui.button("one", "First"));
            second = Some(ui.button("two", "Second"));
        }));
    })?;
    let (toolbar, first, second) = (toolbar.unwrap(), first.unwrap(), second.unwrap());
    let mut tree = LayoutTree::default();
    let styles = |id: &_| {
        if id == &toolbar {
            Style {
                axis: Axis::Row,
                padding: Edges::all(10.25),
                gap: 8.5,
                ..Default::default()
            }
        } else {
            Style {
                width: Length::Px(80.5),
                height: Length::Px(30.25),
                shrink: 0.0,
                ..Default::default()
            }
        }
    };
    tree.sync(&scene, styles)?;
    let viewport = Size::new(400.0, 200.0);
    let initial = tree.compute(viewport)?;
    println!(
        "Initial layout: {:?}, {:?}",
        initial.snapshot.bounds(&first),
        initial.stats
    );
    scene.focus(&first);
    scene.set_hovered(&first, true);
    assert_eq!(tree.sync(&scene, styles)?, Default::default());
    let cached = tree.compute(viewport)?;
    assert!(!cached.stats.performed && initial.snapshot.same_snapshot(&cached.snapshot));
    assert!(cached.snapshot.changes_since(&initial.snapshot).is_empty());
    scene.update_children(&toolbar, |ui| {
        assert_eq!(ui.button("two", "Second"), second);
        assert_eq!(ui.button("one", "First"), first);
    })?;
    let sync = tree.sync(&scene, styles)?;
    assert_eq!(sync.created, 0);
    assert_eq!(sync.removed, 0);
    let reordered = tree.compute(viewport)?;
    let changes = reordered.snapshot.changes_since(&initial.snapshot);
    assert!(changes.order_changed && !changes.viewport_changed);
    assert_eq!(changes.changed, [second.clone(), first.clone()]);
    assert!(changes.added.is_empty() && changes.removed.is_empty());
    scene.take_paint_invalidation();
    for id in &changes.changed {
        scene.invalidate_paint(id);
    }
    assert_eq!(scene.take_paint_invalidation().widgets, changes.changed);
    assert!(
        reordered.snapshot.bounds(&first).unwrap().origin.x
            > reordered.snapshot.bounds(&second).unwrap().origin.x
    );
    assert_eq!(scene.focused(), Some(&first));
    assert!(scene.state(&first).unwrap().hovered());
    assert_eq!(initial.snapshot.bounds(&first).unwrap().origin.x, 10.25);
    let bounds = reordered.snapshot.bounds(&first).unwrap();
    let point = Point::new(bounds.origin.x + 1.0, bounds.origin.y + 1.0);
    let target = reordered
        .snapshot
        .hit_test(point, |id| id != &toolbar)
        .unwrap()
        .clone();
    assert_eq!(target, first);
    let view = reordered.snapshot.semantic_view(&scene.semantic_snapshot());
    let query = SemanticQuery::new()
        .role(SemanticRole::Button)
        .named("First")
        .input_allowed(true);
    let inspected = view.find_visible(query)?;
    assert_eq!(inspected.semantic.id, first);
    assert_eq!(
        view.hit_test(point, |node| node.role == SemanticRole::Button
            && node.state.input_allowed)
            .unwrap()
            .semantic
            .id,
        first
    );
    assert!(view.find_visible_target(query)?.focus(&mut scene));
    assert_eq!(
        initial.snapshot.hit_test(point, |id| id != &toolbar),
        Some(&second)
    );
    scene.focus(&target);
    scene.set_hovered(&target, true);
    assert_eq!(tree.sync(&scene, styles)?, Default::default());
    assert!(
        tree.compute(viewport)?
            .snapshot
            .same_snapshot(&reordered.snapshot)
    );
    let list = zest::paint::DisplayList::build(|painter| {
        for node in reordered
            .snapshot
            .boxes()
            .iter()
            .filter(|node| node.id != toolbar)
        {
            painter.with_clip(node.clip, |painter| {
                painter.quad(zest::paint::Quad {
                    bounds: node.bounds,
                    color: [0.1, 0.4, 0.8, 1.0],
                })
            })?;
        }
        Ok(())
    })?;
    assert_eq!(list.len(), 2);
    println!("Reordered retained nodes: {sync:?}; {:?}", reordered.stats);
    println!(
        "Layout smoke passed: fractional logical bounds, keyed reorder/diff, geometry paint invalidation, semantic visibility/hit/target queries, focus/hover, cache reuse, immutable snapshots and display-list geometry."
    );
    Ok(())
}
