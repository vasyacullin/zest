//! Typed, arena-local generational identities. Removing an entry invalidates
//! its handle immediately; resource destruction is the owner's responsibility.
use std::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId<Kind> {
    owner: u64,
    slot: usize,
    generation: u64,
    kind: PhantomData<fn() -> Kind>,
}

struct Slot<T> {
    generation: u64,
    value: Option<T>,
}

/// Handles cannot address another arena, alias a reused slot, or outlive their
/// arena into a newly created one. Exhausted generations permanently retire a slot.
///
/// ```compile_fail
/// use zest_core::resource::ResourceArena;
/// enum Font {}
/// enum Texture {}
/// let mut fonts = ResourceArena::<Font, u32>::default();
/// let font = fonts.insert(1);
/// let textures = ResourceArena::<Texture, u32>::default();
/// textures.get(&font); // different resource kinds cannot be mixed
/// ```
pub struct ResourceArena<Kind, T> {
    owner: u64,
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
    len: usize,
    kind: PhantomData<fn() -> Kind>,
}

impl<Kind, T> Default for ResourceArena<Kind, T> {
    fn default() -> Self {
        let owner = NEXT_OWNER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .expect("resource owner identities exhausted");
        Self {
            owner,
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
            kind: PhantomData,
        }
    }
}

impl<Kind, T> ResourceArena<Kind, T> {
    pub fn insert(&mut self, value: T) -> ResourceId<Kind> {
        let slot = self.free.pop().unwrap_or_else(|| {
            self.slots.push(Slot {
                generation: 0,
                value: None,
            });
            self.slots.len() - 1
        });
        let entry = &mut self.slots[slot];
        entry.value = Some(value);
        self.len += 1;
        ResourceId {
            owner: self.owner,
            slot,
            generation: entry.generation,
            kind: PhantomData,
        }
    }
    fn slot(&self, id: &ResourceId<Kind>) -> Option<&Slot<T>> {
        if id.owner != self.owner {
            return None;
        }
        self.slots
            .get(id.slot)
            .filter(|s| s.generation == id.generation)
    }
    pub fn get(&self, id: &ResourceId<Kind>) -> Option<&T> {
        self.slot(id)?.value.as_ref()
    }
    pub fn get_mut(&mut self, id: &ResourceId<Kind>) -> Option<&mut T> {
        if id.owner != self.owner {
            return None;
        }
        self.slots
            .get_mut(id.slot)
            .filter(|s| s.generation == id.generation)?
            .value
            .as_mut()
    }
    pub fn remove(&mut self, id: &ResourceId<Kind>) -> Option<T> {
        self.get(id)?;
        let entry = &mut self.slots[id.slot];
        let value = entry.value.take();
        if let Some(next) = entry.generation.checked_add(1) {
            entry.generation = next;
            self.free.push(id.slot);
        }
        self.len -= 1;
        value
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Removes all values without permitting old handles to become valid again.
    pub fn drain(&mut self) -> std::vec::IntoIter<T> {
        let mut values = Vec::with_capacity(self.len);
        self.len = 0;
        self.free.clear();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if let Some(value) = slot.value.take() {
                values.push(value);
            }
            if let Some(next) = slot.generation.checked_add(1) {
                slot.generation = next;
                self.free.push(index);
            }
        }
        values.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Image {}
    #[test]
    fn stale_and_foreign_handles_cannot_alias_reused_slots() {
        let mut a = ResourceArena::<Image, _>::default();
        let old = a.insert("old");
        assert_eq!(a.remove(&old), Some("old"));
        let new = a.insert("new");
        assert_eq!(old.slot, new.slot);
        assert_ne!(old, new);
        assert_eq!(a.get(&old), None);
        assert_eq!(a.remove(&old), None);
        let mut b = ResourceArena::<Image, _>::default();
        let foreign = b.insert("foreign");
        assert_eq!(a.get(&foreign), None);
        *a.get_mut(&new).unwrap() = "changed";
        assert_eq!(a.get(&new), Some(&"changed"));
    }
    #[test]
    fn exhausted_generation_does_not_wrap_and_drain_invalidates_handles() {
        let mut a = ResourceArena::<Image, _>::default();
        let mut old = a.insert(1);
        a.slots[old.slot].generation = u64::MAX;
        old.generation = u64::MAX;
        assert_eq!(a.remove(&old), Some(1));
        let other = a.insert(2);
        assert_ne!(old.slot, other.slot);
        assert_eq!(a.drain().collect::<Vec<_>>(), [2]);
        assert!(a.is_empty());
        let new = a.insert(3);
        assert_eq!(a.get(&old), None);
        assert_eq!(a.get(&other), None);
        assert_eq!(a.get(&new), Some(&3));
    }
    #[test]
    fn unconsumed_drain_still_invalidates_every_entry() {
        let mut arena = ResourceArena::<Image, _>::default();
        let a = arena.insert(1);
        let b = arena.insert(2);
        drop(arena.drain());
        assert!(arena.is_empty());
        assert_eq!(arena.get(&a), None);
        assert_eq!(arena.get(&b), None);
        arena.insert(3);
        arena.insert(4);
        assert_eq!(arena.get(&a), None);
        assert_eq!(arena.get(&b), None);
    }
}
