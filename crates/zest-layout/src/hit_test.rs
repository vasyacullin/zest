use super::*;
use zest_core::geometry::Point;

impl LayoutSnapshot {
    /// Returns the last accepted box in snapshot declaration order containing a
    /// logical point, clipped to the viewport. Paint in snapshot order to share
    /// the same layering. Minimum edges belong to a box, maximum edges do not.
    /// The predicate selects interactive/enabled IDs; containers can be excluded.
    /// Uses this immutable snapshot only: compute current geometry before input.
    /// Includes effective rectangular ancestor clips. Transforms are not supported.
    pub fn hit_test(
        &self,
        point: Point<Logical>,
        mut accepts: impl FnMut(&WidgetId) -> bool,
    ) -> Option<&WidgetId> {
        let viewport = self.viewport();
        if !Rect::from_xywh(0.0, 0.0, viewport.width, viewport.height).contains(point) {
            return None;
        }
        self.boxes()
            .iter()
            .rev()
            .find(|node| {
                node.clip.contains(point) && node.bounds.contains(point) && accepts(&node.id)
            })
            .map(|node| &node.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(
        bounds: [Rect<Logical>; 2],
        viewport: Size<Logical>,
    ) -> (LayoutSnapshot, WidgetId, WidgetId) {
        let mut scene = Scene::default();
        let (mut a, mut b) = (None, None);
        scene
            .update(|ui| {
                a = Some(ui.button("a", "A"));
                b = Some(ui.button("b", "B"));
            })
            .unwrap();
        let (a, b) = (a.unwrap(), b.unwrap());
        let snapshot = LayoutSnapshot(Arc::new(SnapshotData {
            viewport,
            boxes: vec![
                LayoutBox {
                    id: a.clone(),
                    bounds: bounds[0],
                    clip: Rect::from_xywh(0.0, 0.0, viewport.width, viewport.height),
                },
                LayoutBox {
                    id: b.clone(),
                    bounds: bounds[1],
                    clip: Rect::from_xywh(0.0, 0.0, viewport.width, viewport.height),
                },
            ],
            index: HashMap::from([(a.clone(), 0), (b.clone(), 1)]),
        }));
        (snapshot, a, b)
    }
    #[test]
    fn reverse_paint_order_and_interactivity_filter_choose_one_topmost_box() {
        let (frame, a, b) = snapshot(
            [
                Rect::from_xywh(0.0, 0.0, 80.0, 80.0),
                Rect::from_xywh(20.0, 20.0, 50.0, 50.0),
            ],
            Size::new(100.0, 100.0),
        );
        assert_eq!(frame.hit_test(Point::new(30.0, 30.0), |_| true), Some(&b));
        assert_eq!(
            frame.hit_test(Point::new(30.0, 30.0), |id| id != &b),
            Some(&a)
        );
        assert_eq!(frame.hit_test(Point::new(30.0, 30.0), |_| false), None);
        assert_eq!(frame.hit_test(Point::new(10.0, 10.0), |_| true), Some(&a));
    }
    #[test]
    fn viewport_clips_overflow_and_rejects_nonfinite_points() {
        let (frame, a, _) = snapshot(
            [
                Rect::from_xywh(-20.0, -20.0, 200.0, 200.0),
                Rect::from_xywh(0.0, 0.0, 0.0, 20.0),
            ],
            Size::new(100.0, 100.0),
        );
        assert_eq!(frame.hit_test(Point::new(0.0, 0.0), |_| true), Some(&a));
        for point in [
            Point::new(-0.1, 0.0),
            Point::new(100.0, 50.0),
            Point::new(50.0, 100.0),
            Point::new(f64::NAN, 0.0),
            Point::new(0.0, f64::INFINITY),
        ] {
            assert!(frame.hit_test(point, |_| true).is_none());
        }
        let (empty, _, _) = snapshot(
            [Rect::from_xywh(0.0, 0.0, 20.0, 20.0); 2],
            Size::new(0.0, 0.0),
        );
        assert!(empty.hit_test(Point::new(0.0, 0.0), |_| true).is_none());
    }
    #[test]
    fn adjacent_fractional_edges_have_one_owner_and_snapshot_clones_are_immutable() {
        let (frame, a, b) = snapshot(
            [
                Rect::from_xywh(0.0, 0.0, 10.25, 20.0),
                Rect::from_xywh(10.25, 0.0, 10.25, 20.0),
            ],
            Size::new(20.5, 20.0),
        );
        let clone = frame.clone();
        assert_eq!(frame.hit_test(Point::new(10.249, 0.0), |_| true), Some(&a));
        assert_eq!(frame.hit_test(Point::new(10.25, 0.0), |_| true), Some(&b));
        assert!(frame.hit_test(Point::new(20.5, 0.0), |_| true).is_none());
        assert!(frame.same_snapshot(&clone));
        assert_eq!(clone.hit_test(Point::new(10.25, 0.0), |_| true), Some(&b));
    }
}
