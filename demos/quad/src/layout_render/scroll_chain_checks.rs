use super::*;
use zest::layout::{ComputeStats, ScrollAdjustment};

struct Fixture {
    scene: Scene,
    root: WidgetId,
    inner: WidgetId,
    prefix: WidgetId,
    leaf: WidgetId,
    footer: WidgetId,
}
impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut scene = Scene::default();
        scene.update(|ui| {
            ui.container("outer", |ui| {
                ui.button("prefix", "Prefix");
                ui.container("inner", |ui| {
                    ui.button("leaf", "Leaf");
                });
                ui.button("footer", "Footer");
            });
        })?;
        let root = scene.roots()[0].clone();
        let children = scene.children(&root).unwrap();
        let prefix = children[0].clone();
        let inner = children[1].clone();
        let footer = children[2].clone();
        let leaf = scene.children(&inner).unwrap()[0].clone();
        Ok(Self {
            scene,
            root,
            inner,
            prefix,
            leaf,
            footer,
        })
    }
    fn style(&self, id: &WidgetId, changes: &[ScrollAdjustment]) -> Style {
        let (width, height, clip) = if id == &self.root {
            (200.0, 140.0, true)
        } else if id == &self.inner {
            (100.0, 60.0, true)
        } else if id == &self.prefix {
            (200.0, 100.0, false)
        } else if id == &self.leaf {
            (100.0, 180.0, false)
        } else {
            (200.0, 60.0, false)
        };
        Style {
            width: Length::Px(width),
            height: Length::Px(height),
            shrink: 0.0,
            clip_children: clip,
            scroll_offset: changes
                .iter()
                .find(|change| change.widget == *id)
                .map_or(Point::new(0.0, 0.0), |change| change.offset),
            ..Default::default()
        }
    }
    fn capture(
        &self,
        frame: &LayoutSnapshot,
        renderer: &mut Renderer,
    ) -> Result<Capture, Box<dyn std::error::Error>> {
        let list = DisplayList::build(|painter| {
            painter.quad(Quad {
                bounds: Rect::from_xywh(0.0, 0.0, 200.0, 140.0),
                color: [0.0, 0.0, 0.0, 1.0],
            })?;
            for node in frame.boxes() {
                let color = if node.id == self.prefix {
                    [0.1, 0.1, 0.1, 1.0]
                } else if node.id == self.leaf {
                    [0.2, 0.6, 1.0, 1.0]
                } else if node.id == self.footer {
                    [1.0, 0.2, 0.1, 1.0]
                } else {
                    continue;
                };
                painter.with_clip(node.clip, |painter| {
                    painter.quad(Quad {
                        bounds: node.bounds,
                        color,
                    })?;
                    if node.id == self.leaf {
                        // A content marker makes inner scrolling observable
                        // separately from movement of the outer scrollport.
                        painter.quad(Quad {
                            bounds: Rect::from_xywh(
                                node.bounds.origin.x,
                                node.bounds.origin.y + 150.0,
                                node.bounds.size.width,
                                10.0,
                            ),
                            color: [0.1, 0.8, 0.2, 1.0],
                        })?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })?;
        Ok(renderer.capture_list(&list, ScaleFactor::new(1.25)?, 250, 175)?)
    }
}
pub(super) fn check(renderer: &mut Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    let mut tree = LayoutTree::default();
    let viewport = Size::new(200.0, 140.0);
    tree.sync(&fixture.scene, |id| fixture.style(id, &[]))?;
    let original = tree.compute(viewport)?.snapshot;
    let target = original
        .hit_test(Point::new(10.0, 110.0), |id| {
            fixture.scene.input_allowed(id)
        })
        .unwrap();
    let plan = original
        .scroll_plan(target, Point::new(0.0, 150.0), None)
        .unwrap();
    if plan.adjustments.len() != 2
        || plan.adjustments[0].offset.y != 120.0
        || plan.adjustments[1].offset.y != 30.0
        || plan.remaining.y != 0.0
    {
        return Err("nested delta consumption did not pass its remainder outward".into());
    }
    tree.sync(&fixture.scene, |id| fixture.style(id, &plan.adjustments))?;
    let moved = tree.compute(viewport)?;
    if moved.stats != ComputeStats::default() {
        return Err("scroll chain reran layout/measurement".into());
    }
    let image = fixture.capture(&moved.snapshot, renderer)?;
    expect_pixel(&image, 12, 100, [124, 203, 255, 255])?;
    expect_pixel(&image, 12, 130, [89, 231, 124, 255])?;
    expect_pixel(&image, 137, 100, [0, 0, 0, 255])?;
    expect_pixel(&image, 12, 168, [255, 124, 89, 255])?;
    expect_pixel(&image, 12, 81, [89, 89, 89, 255])?;
    if moved
        .snapshot
        .hit_test(Point::new(10.0, 80.0), |id| id == &fixture.leaf)
        != Some(&fixture.leaf)
    {
        return Err("nested scrolled paint/hit positions diverged".into());
    }
    // Start a fresh scope at the original offsets, then stop at its inclusive root.
    tree.sync(&fixture.scene, |id| fixture.style(id, &[]))?;
    let scoped = tree.compute(viewport)?.snapshot;
    fixture.scene.push_modal(&fixture.inner);
    if scoped
        .hit_test(Point::new(10.0, 80.0), |id| fixture.scene.input_allowed(id))
        .is_some()
    {
        return Err("modal hit routing accepted a blocked outer node".into());
    }
    let plan = scoped
        .scroll_plan(
            &fixture.leaf,
            Point::new(0.0, 150.0),
            fixture.scene.modal_root(),
        )
        .unwrap();
    if plan.adjustments.len() != 1 || plan.remaining.y != 30.0 {
        return Err("modal scroll boundary leaked residual input outside its scope".into());
    }
    tree.sync(&fixture.scene, |id| fixture.style(id, &plan.adjustments))?;
    let confined = tree.compute(viewport)?;
    let image = fixture.capture(&confined.snapshot, renderer)?;
    expect_pixel(&image, 12, 100, [89, 89, 89, 255])?;
    expect_pixel(&image, 12, 150, [124, 203, 255, 255])?;
    expect_pixel(&image, 12, 168, [89, 231, 124, 255])?;
    if confined.snapshot.scroll(&fixture.root).unwrap().offset.y != 0.0
        || original.scroll(&fixture.inner).unwrap().offset.y != 0.0
    {
        return Err("scroll boundary moved its parent or mutated an old snapshot".into());
    }
    println!(
        "GPU scroll chain checks passed: nested consumption/residuals, inclusive modal boundary, projected clip/hit pixels and cached geometry"
    );
    Ok(())
}
