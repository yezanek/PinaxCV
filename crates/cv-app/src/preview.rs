//! The latest preview's page images, served to the UI over `preview://`.

use std::sync::{Mutex, PoisonError};

/// Holds the pages of the newest preview revision only.
#[derive(Debug, Default)]
pub struct PreviewCache {
    latest: Mutex<Option<(u64, Vec<Vec<u8>>)>>,
}

impl PreviewCache {
    /// Stores `pages` unless a newer revision is already cached.
    pub fn put(&self, revision: u64, pages: Vec<Vec<u8>>) {
        let mut latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
        if latest.as_ref().is_none_or(|(r, _)| *r <= revision) {
            *latest = Some((revision, pages));
        }
    }

    /// Page `page` (1-based) of `revision`, if that revision is still cached.
    pub fn get(&self, revision: u64, page: u32) -> Option<Vec<u8>> {
        let latest = self.latest.lock().unwrap_or_else(PoisonError::into_inner);
        let (r, pages) = latest.as_ref()?;
        if *r != revision {
            return None;
        }
        let index = usize::try_from(page.checked_sub(1)?).ok()?;
        pages.get(index).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_newest_revision() {
        let cache = PreviewCache::default();
        assert_eq!(cache.get(1, 1), None);
        cache.put(2, vec![b"p1".to_vec(), b"p2".to_vec()]);
        cache.put(1, vec![b"stale".to_vec()]);
        assert_eq!(cache.get(2, 2), Some(b"p2".to_vec()));
        assert_eq!(cache.get(2, 0), None);
        assert_eq!(cache.get(2, 3), None);
        assert_eq!(cache.get(1, 1), None);
        cache.put(3, vec![b"new".to_vec()]);
        assert_eq!(cache.get(2, 1), None);
        assert_eq!(cache.get(3, 1), Some(b"new".to_vec()));
    }
}
