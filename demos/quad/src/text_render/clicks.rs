use super::*;
use zest::text::raster::SelectionGranularity;

const INTERVAL: Duration = Duration::from_millis(500);
const DISTANCE: f64 = 4.0;

#[derive(Clone, Default)]
pub(super) struct ClickSequence {
    last: Option<(Instant, Point<Logical>, u8)>,
}
impl ClickSequence {
    pub fn reset(&mut self) {
        self.last = None;
    }
    pub fn moved(&mut self, point: Point<Logical>) {
        if self.last.is_some_and(|(_, origin, _)| {
            !point.x.is_finite()
                || !point.y.is_finite()
                || (point.x - origin.x).hypot(point.y - origin.y) > DISTANCE
        }) {
            self.reset();
        }
    }
    pub fn press(
        &mut self,
        point: Point<Logical>,
        now: Instant,
        extend: bool,
    ) -> SelectionGranularity {
        if extend {
            self.reset();
            return SelectionGranularity::Grapheme;
        }
        self.moved(point);
        let (origin, count) = match self.last {
            Some((last, origin, count))
                if now
                    .checked_duration_since(last)
                    .is_some_and(|elapsed| elapsed <= INTERVAL)
                    && count < 3 =>
            {
                (origin, count + 1)
            }
            _ => (point, 1),
        };
        self.last = Some((now, origin, count));
        match count {
            2 => SelectionGranularity::Word,
            3 => SelectionGranularity::Line,
            _ => SelectionGranularity::Grapheme,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn click_sequence_uses_controlled_time_and_distance_and_restarts_after_triple() {
        let mut clicks = ClickSequence::default();
        let now = Instant::now();
        let point = Point::new(310.0, 290.0);
        for (tick, expected) in [
            SelectionGranularity::Grapheme,
            SelectionGranularity::Word,
            SelectionGranularity::Line,
            SelectionGranularity::Grapheme,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                clicks.press(point, now + Duration::from_millis(tick as u64 * 100), false),
                expected
            );
        }
        assert_eq!(
            clicks.press(point, now + Duration::from_secs(2), false),
            SelectionGranularity::Grapheme
        );
        assert_eq!(
            clicks.press(
                Point::new(314.0, 290.0),
                now + Duration::from_millis(2100),
                false
            ),
            SelectionGranularity::Word
        );
        clicks.moved(Point::new(314.1, 290.0));
        assert_eq!(
            clicks.press(point, now + Duration::from_millis(2200), false),
            SelectionGranularity::Grapheme
        );
        assert_eq!(
            clicks.press(point, now, false),
            SelectionGranularity::Grapheme
        );
        assert_eq!(
            clicks.press(point, now + INTERVAL, false),
            SelectionGranularity::Word
        );
        assert_eq!(
            clicks.press(point, now + INTERVAL, true),
            SelectionGranularity::Grapheme
        );
        assert_eq!(
            clicks.press(point, now + INTERVAL, false),
            SelectionGranularity::Grapheme
        );
        clicks.reset();
        assert_eq!(
            clicks.press(point, now + INTERVAL, false),
            SelectionGranularity::Grapheme
        );
    }
}
