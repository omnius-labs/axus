use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use parking_lot::Mutex;
use tokio::sync::Mutex as TokioMutex;

/// One lock per output destination, so a long write of one subscription delays only the operations on that destination.
#[derive(Default)]
pub struct OutputLocks {
    locks: Mutex<HashMap<PathBuf, Arc<TokioMutex<()>>>>,
}

impl OutputLocks {
    pub fn get(&self, destination: &Path) -> Arc<TokioMutex<()>> {
        let mut locks = self.locks.lock();
        // Only the map holds an unused lock, so a held or awaited lock stays in place.
        locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        locks.entry(destination.to_path_buf()).or_default().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_destination_shares_its_lock_and_an_unused_lock_is_dropped() {
        let locks = OutputLocks::default();
        let first = locks.get(Path::new("a"));
        let second = locks.get(Path::new("a"));
        assert!(Arc::ptr_eq(&first, &second));
        assert!(!Arc::ptr_eq(&first, &locks.get(Path::new("b"))));
        drop(first);
        drop(second);
        locks.get(Path::new("c"));
        assert_eq!(locks.locks.lock().len(), 1);
    }
}
