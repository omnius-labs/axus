use std::{collections::HashMap, hash::Hash};

use tokio::time::Instant;

/// Keys that are set aside until a time. Time is passed in, so tests need no clock.
pub struct Expirations<K> {
    entries: HashMap<K, Instant>,
}

impl<K: Hash + Eq> Expirations<K> {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Sets the key aside until `until`. A later time already held for the key is kept.
    pub fn insert(&mut self, key: K, until: Instant) {
        let entry = self.entries.entry(key).or_insert(until);
        *entry = (*entry).max(until);
    }

    pub fn contains(&mut self, key: &K, now: Instant) -> bool {
        self.entries.retain(|_, until| *until > now);
        self.entries.contains_key(key)
    }

    /// Removes the key and tells whether it was held. A key past its time is already gone.
    pub fn take(&mut self, key: &K, now: Instant) -> bool {
        self.entries.retain(|_, until| *until > now);
        self.entries.remove(key).is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::Expirations;

    #[test]
    fn a_taken_key_is_gone_and_expired_keys_are_not_taken() {
        let now = Instant::now();
        let mut expirations = Expirations::new();
        expirations.insert(1, now + Duration::from_secs(30));
        expirations.insert(2, now + Duration::from_secs(30));

        assert!(expirations.take(&1, now));
        assert!(!expirations.take(&1, now));
        assert!(!expirations.take(&2, now + Duration::from_secs(30)));
        assert!(!expirations.take(&3, now));
    }

    #[test]
    fn a_key_is_held_until_its_time_and_a_shorter_insert_does_not_shorten_it() {
        let now = Instant::now();
        let mut expirations = Expirations::new();
        expirations.insert(1, now + Duration::from_secs(30));
        expirations.insert(1, now + Duration::from_secs(10));

        assert!(expirations.contains(&1, now + Duration::from_secs(29)));
        assert!(!expirations.contains(&1, now + Duration::from_secs(30)));
        assert!(!expirations.contains(&2, now));
    }
}
