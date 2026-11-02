use super::*;
fn bar(axis: Axis) -> ScrollbarGeometry {
    ScrollbarGeometry::new(
        Rect::from_xywh(10.25, 20.5, 100.0, 100.0),
        axis,
        100.0,
        ScrollMetrics {
            offset: Point::new(50.0, 50.0),
            max_offset: Point::new(300.0, 300.0),
        },
        8.0,
    )
    .unwrap()
}

#[test]
fn proportional_thumbs_and_half_open_hits_match_both_axes() {
    for axis in [Axis::Row, Axis::Column] {
        let bar = bar(axis);
        let thumb = bar.thumb();
        let point = Point::new(thumb.origin.x + 1.0, thumb.origin.y + 1.0);
        assert_eq!(bar.hit_test(point), Some(ScrollbarPart::Thumb));
        let (start, length) = match axis {
            Axis::Row => (thumb.origin.x, thumb.size.width),
            Axis::Column => (thumb.origin.y, thumb.size.height),
        };
        assert_eq!(length, 25.0);
        let at = |position| match axis {
            Axis::Row => Point::new(position, 30.0),
            Axis::Column => Point::new(20.0, position),
        };
        assert_eq!(
            bar.hit_test(at(start - 0.001)),
            Some(ScrollbarPart::BeforeThumb)
        );
        assert_eq!(
            bar.hit_test(at(start + length)),
            Some(ScrollbarPart::AfterThumb)
        );
        assert!(
            bar.hit_test(at(match axis {
                Axis::Row => 110.25,
                Axis::Column => 120.5,
            }))
            .is_none()
        );
    }
}

#[test]
fn grabbed_position_does_not_jump_and_dragging_clamps_outside_the_track() {
    for axis in [Axis::Row, Axis::Column] {
        let bar = bar(axis);
        let thumb = bar.thumb();
        let point = Point::new(
            thumb.origin.x + thumb.size.width * 0.5,
            thumb.origin.y + thumb.size.height * 0.5,
        );
        let grab = bar.grab_offset(point).unwrap();
        assert_eq!(bar.drag_offset(point, grab), Some(50.0));
        assert_eq!(
            bar.drag_offset(Point::new(-1000.0, -1000.0), grab),
            Some(0.0)
        );
        assert_eq!(
            bar.drag_offset(Point::new(1000.0, 1000.0), grab),
            Some(300.0)
        );
        assert!(bar.drag_offset(Point::new(f64::NAN, 0.0), grab).is_none());
        assert!(bar.drag_offset(point, -1.0).is_none());
    }
}

#[test]
fn page_clicks_move_one_viewport_and_clamp_to_content_ends() {
    let bar = bar(Axis::Column);
    assert_eq!(bar.page_offset(ScrollbarPart::BeforeThumb), 0.0);
    assert_eq!(bar.page_offset(ScrollbarPart::Thumb), 50.0);
    assert_eq!(bar.page_offset(ScrollbarPart::AfterThumb), 150.0);
    let end = ScrollbarGeometry::new(
        bar.track(),
        Axis::Column,
        100.0,
        ScrollMetrics {
            offset: Point::new(0.0, 290.0),
            max_offset: Point::new(0.0, 300.0),
        },
        8.0,
    )
    .unwrap();
    assert_eq!(end.page_offset(ScrollbarPart::AfterThumb), 300.0);
}

#[test]
fn invalid_or_nonoverflowing_tracks_are_hidden_and_tiny_tracks_never_divide_by_zero() {
    let metrics = ScrollMetrics {
        offset: Point::new(0.0, 20.0),
        max_offset: Point::new(0.0, 1000.0),
    };
    let track = Rect::from_xywh(0.0, 0.0, 8.0, 4.0);
    let tiny = ScrollbarGeometry::new(track, Axis::Column, 100.0, metrics, 16.0).unwrap();
    assert_eq!(tiny.thumb(), track);
    assert_eq!(tiny.drag_offset(Point::new(4.0, 1000.0), 2.0), Some(20.0));
    for extent in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(ScrollbarGeometry::new(track, Axis::Column, extent, metrics, 8.0).is_none());
    }
    assert!(
        ScrollbarGeometry::new(track, Axis::Column, 100.0, ScrollMetrics::default(), 8.0).is_none()
    );
    assert!(
        ScrollbarGeometry::new(
            Rect::from_xywh(0.0, 0.0, 0.0, 20.0),
            Axis::Column,
            100.0,
            metrics,
            8.0
        )
        .is_none()
    );
    assert!(ScrollbarGeometry::new(track, Axis::Column, 100.0, metrics, f64::NAN).is_none());
}

#[test]
fn extreme_finite_ranges_keep_thumb_and_offsets_finite() {
    let bar = ScrollbarGeometry::new(
        Rect::from_xywh(0.0, 0.0, 8.0, 100.0),
        Axis::Column,
        f64::MAX,
        ScrollMetrics {
            offset: Point::new(0.0, f64::MAX),
            max_offset: Point::new(0.0, f64::MAX),
        },
        8.0,
    )
    .unwrap();
    assert_eq!(bar.thumb().size.height, 50.0);
    assert_eq!(bar.offset(), f64::MAX);
    assert_eq!(bar.page_offset(ScrollbarPart::AfterThumb), f64::MAX);
    assert_eq!(
        bar.drag_offset(Point::new(4.0, 1000.0), 25.0),
        Some(f64::MAX)
    );
}
