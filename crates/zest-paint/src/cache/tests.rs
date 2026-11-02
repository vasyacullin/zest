use super::*;
use crate::{PaintError, Quad};
use zest_core::geometry::Rect;

fn list(red: f32) -> DisplayList {
    DisplayList::build(|painter| {
        painter.quad(Quad {
            bounds: Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
            color: [red, 0.0, 0.0, 1.0],
        })
    })
    .unwrap()
}

#[test]
fn hits_share_commands_and_changed_keys_preserve_published_old_snapshots() {
    let mut cache = DisplayListCache::default();
    assert!(cache.get(&1).is_none());
    let first = cache
        .get_or_try_build(1, || Ok::<_, ()>(list(0.25)))
        .unwrap();
    let hit = cache
        .get_or_try_build::<()>(1, || panic!("hit must not build"))
        .unwrap();
    assert!(Arc::ptr_eq(&first, &hit));
    assert!(std::ptr::eq(first.quads().as_ptr(), hit.quads().as_ptr()));
    let next = cache
        .get_or_try_build(2, || Ok::<_, ()>(list(0.75)))
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &next));
    assert_eq!(first.quads()[0].color[0], 0.25);
    assert_eq!(next.quads()[0].color[0], 0.75);
    assert!(cache.get(&1).is_none());
    assert!(Arc::ptr_eq(&cache.get(&2).unwrap(), &next));
}

#[test]
fn failed_or_panicking_build_keeps_the_previous_entry_without_serving_it_for_a_new_key() {
    let mut cache = DisplayListCache::default();
    let first = cache
        .get_or_try_build(1, || Ok::<_, &str>(list(1.0)))
        .unwrap();
    assert_eq!(cache.get_or_try_build(2, || Err("failed")), Err("failed"));
    assert!(cache.get(&2).is_none());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<_, ()> = cache.get_or_try_build(3, || panic!("builder"));
    }));
    assert!(result.is_err() && cache.get(&3).is_none());
    assert!(Arc::ptr_eq(&cache.get(&1).unwrap(), &first));
    assert_eq!(
        cache
            .get_or_try_build(2, || Ok::<_, &str>(list(0.0)))
            .unwrap()
            .quads()[0]
            .color[0],
        0.0
    );
}

#[test]
fn invalid_paint_never_replaces_an_accepted_snapshot() {
    let mut cache = DisplayListCache::default();
    let first = cache
        .get_or_try_build(1, || Ok::<_, PaintError>(list(1.0)))
        .unwrap();
    let error = cache.get_or_try_build(2, || {
        DisplayList::build(|painter| {
            painter.quad(Quad {
                bounds: Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
                color: [f32::NAN, 0.0, 0.0, 1.0],
            })
        })
    });
    assert_eq!(error, Err(PaintError::InvalidColor));
    assert!(Arc::ptr_eq(&cache.get(&1).unwrap(), &first));
}

#[test]
fn clearing_drops_cache_ownership_without_invalidating_held_snapshots() {
    let mut cache = DisplayListCache::default();
    let first = cache
        .get_or_try_build((), || Ok::<_, ()>(list(1.0)))
        .unwrap();
    assert_eq!(Arc::strong_count(&first), 2);
    cache.clear();
    assert_eq!(Arc::strong_count(&first), 1);
    assert!(cache.get(&()).is_none());
    assert_eq!(first.len(), 1);
}

#[test]
fn keys_need_no_clone_or_default_and_empty_frames_are_cached() {
    #[derive(PartialEq)]
    struct Key(u32);
    let mut cache: DisplayListCache<Key> = Default::default();
    let first = cache
        .get_or_try_build(Key(7), || Ok::<_, ()>(DisplayList::default()))
        .unwrap();
    let again = cache
        .get_or_try_build::<()>(Key(7), || panic!("empty frame is still cached"))
        .unwrap();
    assert!(first.is_empty() && Arc::ptr_eq(&first, &again));
}
