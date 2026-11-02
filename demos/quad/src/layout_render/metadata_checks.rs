use super::*;
use zest::{
    UpdateStats,
    semantics::{FindError, SemanticQuery},
};

pub(super) fn check(
    demo: &mut LayoutDemo,
    renderer: &mut Renderer,
) -> Result<(), Box<dyn std::error::Error>> {
    let layout = demo.snapshot.as_ref().unwrap().clone();
    let old = demo.scene.semantic_snapshot();
    let footer = old.find(SemanticQuery::new().test_id("layout.footer"))?;
    let root = old.find(SemanticQuery::new().test_id("layout.root"))?;
    if footer.name != "Layout status"
        || footer.description.as_deref() != Some("Follows measured text")
        || root.name != "Layout preview"
        || demo.scene.label(&footer.id) != Some("Layout follows text")
    {
        return Err("declared semantic names/descriptions changed the visible scene label".into());
    }
    let guard = demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .layout_selection(zest::text::Selection::caret(0))?;
    let staged = renderer.upload_stats().staged_bytes;
    let images: Vec<_> = demo
        .labels
        .iter()
        .map(|label| label.prepared.as_ref().unwrap().image)
        .collect();
    let archetypes = demo.scene.stats().archetypes;
    demo.scene.take_paint_invalidation();
    let list = demo.cached_paint()?;
    let before = renderer.capture_list(&list, ScaleFactor::new(1.25)?, 300, 563)?;
    if redeclare(demo, "Layout marker", "layout.marker")?
        != (UpdateStats {
            created: 0,
            updated: 1,
            removed: 0,
        })
    {
        return Err("metadata update changed declarations outside the footer".into());
    }
    let current = demo.scene.semantic_snapshot();
    let changes = current.changes_since(&old);
    if current
        .find(
            SemanticQuery::new()
                .test_id("layout.marker")
                .named("Layout marker"),
        )?
        .id
        != footer.id
        || current.find(SemanticQuery::new().test_id("layout.footer")) != Err(FindError::NotFound)
        || old
            .find(SemanticQuery::new().test_id("layout.footer"))?
            .name
            != "Layout status"
        || demo.scene.needs_paint()
        || changes.changed != std::slice::from_ref(&footer.id)
        || !changes.replaced.is_empty()
        || changes.order_changed
        || changes.roots_changed
        || changes.modal_changed
    {
        return Err(
            "metadata update changed identity/paint or mutated the old semantic snapshot".into(),
        );
    }
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    let updated = demo.cached_paint()?;
    let after = renderer.capture_list(&updated, ScaleFactor::new(1.25)?, 300, 563)?;
    if !Arc::ptr_eq(&list, &updated) || before.rgba8 != after.rgba8 {
        return Err("metadata-only update rebuilt paint or changed rendered pixels".into());
    }
    redeclare(demo, "Layout status", "layout.footer")?;
    demo.prepare(renderer, Size::new(240.0, 450.0), 1.25)?;
    demo.labels[0]
        .prepared
        .as_ref()
        .unwrap()
        .paragraph
        .selection_caret(&guard)?;
    if !demo
        .scene
        .semantic_snapshot()
        .changes_since(&old)
        .is_empty()
        || !layout.same_snapshot(demo.snapshot.as_ref().unwrap())
        || demo.scene.needs_paint()
        || !Arc::ptr_eq(&list, &demo.cached_paint()?)
        || renderer.upload_stats().staged_bytes != staged
        || demo.scene.stats().archetypes != archetypes
        || demo
            .labels
            .iter()
            .map(|label| label.prepared.as_ref().unwrap().image)
            .collect::<Vec<_>>()
            != images
    {
        return Err(
            "semantic metadata invalidated layout, shaping, images, paint cache or ECS archetypes"
                .into(),
        );
    }
    println!(
        "GPU semantic metadata checks passed: independent names/descriptions, stable test IDs, immutable observations, metadata-only reconciliation, shared paint/pixels and retained layout/text/images"
    );
    Ok(())
}
pub(super) fn redeclare(
    demo: &mut LayoutDemo,
    name: &str,
    test_id: &str,
) -> Result<UpdateStats, zest::ReconcileError> {
    declare_footer(demo, Some((name, test_id)))
}
pub(super) fn without_footer(demo: &mut LayoutDemo) -> Result<UpdateStats, zest::ReconcileError> {
    declare_footer(demo, None)
}
fn declare_footer(
    demo: &mut LayoutDemo,
    properties: Option<(&str, &str)>,
) -> Result<UpdateStats, zest::ReconcileError> {
    let labels = &demo.labels;
    demo.scene.update_children(&demo.root, |ui| {
        for (key, label) in ["title", "body"].iter().zip(labels) {
            let id = ui.button(*key, label.source);
            ui.set_semantics(
                &id,
                SemanticProperties::default().test_id(format!("layout.{key}")),
            );
        }
        if let Some((name, test_id)) = properties {
            let footer = ui.button("footer", "Layout follows text");
            ui.set_semantics(
                &footer,
                SemanticProperties::default()
                    .named(name)
                    .described("Follows measured text")
                    .test_id(test_id),
            );
        }
    })
}
