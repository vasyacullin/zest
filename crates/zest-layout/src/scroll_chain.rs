use super::*;

/// Read-only scroll advice for one signed logical offset delta. Apply absolute
/// adjustments to matching styles and sync/compute before delivering more input.
///
/// ```
/// use zest_core::{Scene, geometry::{Point, Size}};
/// use zest_layout::{LayoutTree, Length, Style};
/// let mut scene = Scene::default();
/// scene.update(|ui| { ui.container("outer", |ui| {
///     ui.container("inner", |ui| { ui.button("content", "Content"); });
/// }); })?;
/// let outer = scene.roots()[0].clone();
/// let inner = scene.children(&outer).unwrap()[0].clone();
/// let content = scene.children(&inner).unwrap()[0].clone();
/// let mut tree = LayoutTree::default();
/// tree.sync(&scene, |id| Style {
///     width: Length::Px(100.0), height: Length::Px(if id == &outer { 80.0 } else if id == &inner { 120.0 } else { 240.0 }),
///     clip_children: id != &content, shrink: 0.0, ..Default::default()
/// })?;
/// let frame = tree.compute(Size::new(100.0, 80.0))?.snapshot;
/// let plan = frame.scroll_plan(&content, Point::new(0.0, 150.0), None).unwrap();
/// assert_eq!(plan.adjustments[0].widget, inner);
/// assert_eq!(plan.adjustments[0].offset.y, 120.0);
/// assert_eq!(plan.adjustments[1].offset.y, 30.0);
/// assert_eq!(plan.remaining.y, 0.0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ScrollPlan {
    /// Changed clipping nodes, starting with the target and walking outward.
    pub adjustments: Vec<ScrollAdjustment>,
    /// Unconsumed offset delta. Each axis keeps its requested sign; callers may
    /// leave it unused or forward it to another owner outside this layout scene.
    pub remaining: Point<Logical>,
}

impl LayoutSnapshot {
    /// Route an offset delta from a known target through its clipping ancestors.
    /// Positive components increase scroll offsets (opposite to wheel content
    /// movement). Each node consumes only what fits its range and forwards the
    /// remainder independently per axis. A nonclipping node simply passes it on.
    ///
    /// `boundary` is an optional inclusive ancestor/target where routing stops,
    /// such as a modal root. Unknown targets/boundaries, unrelated boundaries and
    /// nonfinite deltas return None without changes. Zero deltas and exhausted
    /// chains return a plan with no adjustments. This owns no input eligibility,
    /// hit testing or entity-generation checks; use a fresh snapshot and validate
    /// the starting target against the scene before applying the plan.
    pub fn scroll_plan(
        &self,
        target: &WidgetId,
        delta: Point<Logical>,
        boundary: Option<&WidgetId>,
    ) -> Option<ScrollPlan> {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return None;
        }
        let start = *self.0.index.get(target)?;
        let stop = match boundary {
            Some(id) => Some(*self.0.index.get(id)?),
            None => None,
        };
        let mut path = Vec::new();
        let mut current = Some(start);
        let mut reached = stop.is_none();
        while let Some(index) = current {
            path.push(index);
            if Some(index) == stop {
                reached = true;
                break;
            }
            current = self.0.parents[index];
        }
        if !reached {
            return None;
        }
        let mut remaining = delta;
        let mut adjustments = Vec::new();
        for index in path {
            if self.0.clipping[index] {
                let node = &self.0.boxes[index];
                let (x, rest_x) =
                    consume(node.scroll.offset.x, node.scroll.max_offset.x, remaining.x);
                let (y, rest_y) =
                    consume(node.scroll.offset.y, node.scroll.max_offset.y, remaining.y);
                let offset = Point::new(x, y);
                if offset != node.scroll.offset {
                    adjustments.push(ScrollAdjustment {
                        widget: node.id.clone(),
                        offset,
                    });
                }
                remaining = Point::new(rest_x, rest_y);
            }
            if remaining.x == 0.0 && remaining.y == 0.0 {
                break;
            }
        }
        Some(ScrollPlan {
            adjustments,
            remaining,
        })
    }
}

fn consume(current: f64, max: f64, delta: f64) -> (f64, f64) {
    let amount = if delta >= 0.0 {
        delta.min((max - current).max(0.0))
    } else {
        delta.max(-current)
    };
    let next = (current + amount).clamp(0.0, max);
    let rest = delta - (next - current);
    // Rounding to a representable offset must not reverse the residual's sign.
    (
        next,
        if delta >= 0.0 {
            rest.max(0.0)
        } else {
            rest.min(0.0)
        },
    )
}

#[cfg(test)]
mod tests;
