//! Fence-ordered byte-range allocation. Reservations have no side effects until
//! submission succeeds; outstanding ranges are never overwritten on wrap.
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reservation {
    head: u64,
    start: u64,
    end: u64,
    pub offset: u64,
    pub size: u64,
    pub wrapped: bool,
}
struct Span {
    end: u64,
    fence: u64,
}
pub(crate) struct Ring {
    capacity: u64,
    head: u64,
    tail: u64,
    spans: VecDeque<Span>,
    last_fence: u64,
}
impl Ring {
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            head: 0,
            tail: 0,
            spans: VecDeque::new(),
            last_fence: 0,
        }
    }
    pub fn capacity(&self) -> u64 {
        self.capacity
    }
    pub fn used(&self) -> u64 {
        self.head - self.tail
    }
    pub fn oldest_fence(&self) -> Option<u64> {
        self.spans.front().map(|s| s.fence)
    }
    pub fn latest_fence(&self) -> Option<u64> {
        self.spans.back().map(|s| s.fence)
    }
    pub fn reserve(&self, size: u64, alignment: u64) -> Option<Reservation> {
        if size == 0
            || !alignment.is_power_of_two()
            || !self.capacity.is_power_of_two()
            || alignment > self.capacity
            || size > self.capacity
        {
            return None;
        }
        let reserved = size.checked_add(alignment - 1)? & !(alignment - 1);
        let mut start = self.head.checked_add(alignment - 1)? & !(alignment - 1);
        if (start % self.capacity).checked_add(reserved)? > self.capacity {
            start = start.checked_add(self.capacity - start % self.capacity)?;
        }
        let end = start.checked_add(reserved)?;
        let tail = if self.spans.is_empty() {
            start
        } else {
            self.tail
        };
        if end.checked_sub(tail)? > self.capacity {
            return None;
        }
        Some(Reservation {
            head: self.head,
            start,
            end,
            offset: start % self.capacity,
            size,
            wrapped: self.head != 0 && start / self.capacity > (self.head - 1) / self.capacity,
        })
    }
    pub fn commit(&mut self, reservation: Reservation, fence: u64) -> bool {
        if reservation.head != self.head || fence <= self.last_fence {
            return false;
        }
        if self.spans.is_empty() {
            self.tail = reservation.start;
        }
        self.head = reservation.end;
        self.last_fence = fence;
        self.spans.push_back(Span {
            end: self.head,
            fence,
        });
        true
    }
    pub fn reclaim(&mut self, completed: u64) {
        while self.spans.front().is_some_and(|s| s.fence <= completed) {
            self.tail = self.spans.pop_front().unwrap().end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outstanding_ranges_block_wrap_until_their_fence_completes() {
        let mut ring = Ring::new(1024);
        let a = ring.reserve(512, 256).unwrap();
        assert!(ring.commit(a, 1));
        let b = ring.reserve(512, 256).unwrap();
        assert!(ring.commit(b, 2));
        assert_eq!(ring.used(), 1024);
        assert!(ring.reserve(1, 256).is_none());
        ring.reclaim(1);
        let c = ring.reserve(300, 256).unwrap();
        assert_eq!(c.offset, 0);
        assert!(c.wrapped);
        assert!(ring.commit(c, 3));
        assert!(ring.reserve(1, 256).is_none());
        ring.reclaim(2);
        assert_eq!(ring.reserve(256, 256).unwrap().offset, 512);
        ring.reclaim(3);
        assert_eq!(ring.used(), 0);
    }
    #[test]
    fn empty_ring_can_wrap_a_full_capacity_allocation_without_charging_padding() {
        let mut ring = Ring::new(1024);
        let a = ring.reserve(100, 4).unwrap();
        assert!(ring.commit(a, 1));
        ring.reclaim(1);
        let b = ring.reserve(1024, 256).unwrap();
        assert_eq!(b.offset, 0);
        assert!(ring.commit(b, 2));
        assert_eq!(ring.used(), 1024);
        ring.reclaim(2);
        assert_eq!(ring.used(), 0);
    }
    #[test]
    fn abandoned_and_stale_reservations_do_not_consume_or_alias_space() {
        let mut ring = Ring::new(1024);
        let abandoned = ring.reserve(400, 256).unwrap();
        assert_eq!(ring.used(), 0);
        assert_eq!(ring.reserve(400, 256), Some(abandoned));
        assert!(ring.commit(abandoned, 1));
        assert!(!ring.commit(abandoned, 2));
        ring.reclaim(1);
        let next = ring.reserve(100, 4).unwrap();
        assert!(!ring.commit(next, 1)); // completed values must never be reused
        assert!(ring.commit(next, 2));
        assert!(ring.reserve(0, 4).is_none());
        assert!(ring.reserve(1025, 4).is_none());
        assert!(ring.reserve(1, 3).is_none());
        assert!(ring.reserve(1, 2048).is_none());
    }
    #[test]
    fn repeated_wraps_keep_live_physical_ranges_disjoint() {
        let mut ring = Ring::new(1024);
        let mut live: Vec<(u64, Reservation)> = Vec::new();
        for fence in 1..500u64 {
            let size = 1 + fence * 73 % 400;
            let mut reservation = ring.reserve(size, 64);
            while reservation.is_none() {
                let completed = ring.oldest_fence().unwrap();
                ring.reclaim(completed);
                live.retain(|(f, _)| *f > completed);
                reservation = ring.reserve(size, 64);
            }
            let r = reservation.unwrap();
            for (_, old) in &live {
                assert!(r.offset + r.size <= old.offset || old.offset + old.size <= r.offset);
            }
            assert!(ring.commit(r, fence));
            live.push((fence, r));
            assert!(ring.used() <= ring.capacity());
        }
    }
    #[test]
    fn counters_fail_closed_instead_of_wrapping() {
        let mut ring = Ring::new(1024);
        ring.head = u64::MAX - 2;
        ring.tail = ring.head;
        assert!(ring.reserve(8, 4).is_none());
    }
}
