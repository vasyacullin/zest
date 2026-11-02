use super::*;
use winit::keyboard::{Key, NamedKey};

fn demo() -> LayoutDemo {
    let mut demo = LayoutDemo::new().unwrap();
    demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap();
    demo
}

#[test]
fn geometry_changes_mark_affected_nodes_without_scene_reconciliation() {
    let mut demo = demo();
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let archetypes = demo.scene.stats().archetypes;
    demo.finish_frame(RenderStatus::Presented, true);
    assert!(demo.update_geometry(Size::new(300.0, 450.0), 1.25).unwrap());
    let changes = demo.snapshot.as_ref().unwrap().changes_since(&snapshot);
    assert!(changes.viewport_changed && !changes.changed.is_empty());
    let work = demo.scene.take_paint_invalidation();
    assert!(!work.structure_changed);
    for id in &changes.changed {
        assert!(work.widgets.contains(id));
    }
    assert!(work.widgets.contains(&demo.root));
    assert!(!demo.update_geometry(Size::new(300.0, 450.0), 1.25).unwrap());
    assert!(!demo.scene.needs_paint());
    assert_eq!(snapshot.viewport(), Size::new(240.0, 450.0));
    assert_eq!(demo.scene.stats().archetypes, archetypes);
}

#[test]
fn clipped_scroll_projection_marks_owner_and_children_without_scene_structure_changes() {
    let mut demo = demo();
    demo.key(&Key::Named(NamedKey::F4), true, false);
    let old = demo.snapshot.as_ref().unwrap().clone();
    demo.finish_frame(RenderStatus::Presented, true);
    assert!(demo.scroll_to(Point::new(0.0, 30.0), false).unwrap());
    let snapshot = demo.snapshot.as_ref().unwrap();
    let changes = snapshot.changes_since(&old);
    let work = demo.scene.take_paint_invalidation();
    assert!(!changes.order_changed && !changes.viewport_changed);
    assert!(!work.structure_changed && !work.is_empty());
    assert!(changes.changed.contains(&demo.root));
    for id in &changes.changed {
        assert!(work.widgets.contains(id));
    }
    for (new, old) in snapshot.boxes().iter().zip(old.boxes()) {
        assert_eq!(new.layout_bounds, old.layout_bounds);
    }
}

#[test]
fn frame_completion_drains_only_a_successfully_painted_scene() {
    let mut demo = demo();
    let work = demo.scene.paint_invalidation();
    assert!(!work.is_empty());
    for status in [
        RenderStatus::Suspended,
        RenderStatus::Skipped,
        RenderStatus::NeedsResize,
    ] {
        demo.finish_frame(status, true);
        assert_eq!(demo.scene.paint_invalidation(), work);
    }
    demo.finish_frame(RenderStatus::Presented, false);
    assert_eq!(demo.scene.paint_invalidation(), work);
    demo.finish_frame(RenderStatus::Presented, true);
    assert!(!demo.scene.needs_paint());
}

#[test]
fn selection_invalidates_both_old_and_new_nodes_and_interception_marks_the_root() {
    let mut demo = demo();
    let title = demo.labels[0].id.clone();
    let footer = demo.footer.clone();
    let snapshot = demo.snapshot.as_ref().unwrap().clone();
    let archetypes = demo.scene.stats().archetypes;
    let enter = Key::Named(NamedKey::Enter);
    demo.set_focus(Some(title.clone()));
    demo.key(&enter, true, false);
    demo.key(&enter, false, false);
    assert_eq!(demo.selected.as_ref(), Some(&title));
    demo.set_focus(Some(footer.clone()));
    demo.finish_frame(RenderStatus::Presented, true);
    demo.key(&enter, true, false);
    demo.key(&enter, false, false);
    assert_eq!(demo.selected.as_ref(), Some(&footer));
    let work = demo.scene.take_paint_invalidation();
    assert_eq!(work.widgets, [title, footer]);
    assert!(!work.structure_changed);
    demo.key(&Key::Named(NamedKey::F6), true, false);
    assert_eq!(
        demo.scene.take_paint_invalidation().widgets,
        [demo.root.clone()]
    );
    assert!(!demo.update_geometry(Size::new(240.0, 450.0), 1.25).unwrap());
    assert!(!demo.scene.needs_paint());
    assert!(snapshot.same_snapshot(demo.snapshot.as_ref().unwrap()));
    assert_eq!(demo.scene.stats().archetypes, archetypes);
}
