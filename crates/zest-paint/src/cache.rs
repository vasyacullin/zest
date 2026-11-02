use crate::DisplayList;
use std::sync::Arc;

/// One reusable CPU paint snapshot, selected by an application-owned key.
/// Equal keys reuse the same Arc without calling the builder or cloning command
/// buffers. A failed/panicking builder leaves the last successful entry intact.
/// This does not track scene/layout changes or own referenced GPU images.
///
/// Keys must cover every paint input, including resource identities and device
/// recovery. Clear or change the key when any such input changes. Holding an Arc
/// keeps CPU commands alive, not their GPU resources; resource validity remains
/// the renderer/application's responsibility. This cache does not acknowledge
/// scene invalidation or submit/present frames.
///
/// ```
/// use std::sync::Arc;
/// use zest_paint::{DisplayList, DisplayListCache, PaintError};
/// let mut cache = DisplayListCache::default();
/// let first = cache.get_or_try_build(1, || Ok::<_, PaintError>(DisplayList::default()))?;
/// let repeated = cache.get_or_try_build::<PaintError>(1, || panic!("unchanged key"))?;
/// assert!(Arc::ptr_eq(&first, &repeated));
/// cache.clear();
/// assert!(cache.get(&1).is_none());
/// assert!(first.is_empty()); // Published snapshots remain valid CPU data.
/// # Ok::<(), PaintError>(())
/// ```
pub struct DisplayListCache<K> {
    entry: Option<(K, Arc<DisplayList>)>,
}
impl<K> Default for DisplayListCache<K> {
    fn default() -> Self {
        Self { entry: None }
    }
}
impl<K> DisplayListCache<K> {
    pub fn clear(&mut self) {
        self.entry = None;
    }
}
impl<K: PartialEq> DisplayListCache<K> {
    /// Read only if the requested key matches the last successful build.
    pub fn get(&self, key: &K) -> Option<Arc<DisplayList>> {
        self.entry
            .as_ref()
            .filter(|(cached, _)| cached == key)
            .map(|(_, list)| Arc::clone(list))
    }
    /// Build on a miss and publish only on success. The error type is supplied
    /// by the caller; errors are returned without falling back to stale paint.
    pub fn get_or_try_build<E>(
        &mut self,
        key: K,
        build: impl FnOnce() -> Result<DisplayList, E>,
    ) -> Result<Arc<DisplayList>, E> {
        if let Some(list) = self.get(&key) {
            return Ok(list);
        }
        let list = Arc::new(build()?);
        self.entry = Some((key, Arc::clone(&list)));
        Ok(list)
    }
}

#[cfg(test)]
mod tests;
