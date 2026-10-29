use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use omnius_core_base::clock::Clock;

use crate::model::{AssetKey, NodeProfile};

struct Entry {
    locations: Vec<Arc<NodeProfile>>,
    last_seen: DateTime<Utc>,
}

/// The nodes that advertise an AssetKey, shared by every Session.
///
/// Keeping the information here rather than in the Session that carried it means a location does not disappear when that Session ends, and an advertisement is also visible to a local lookup.
pub struct LocationCache {
    entries: HashMap<Arc<AssetKey>, Entry>,
    profile_count: usize,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

impl LocationCache {
    /// The most AssetKeys one node keeps.
    pub const MAX_ASSET_KEYS: usize = 4096;
    /// The most NodeProfiles one node keeps across every AssetKey.
    pub const MAX_NODE_PROFILES: usize = 16384;
    /// How long a location is kept after it was last received.
    pub const LIFETIME: Duration = Duration::minutes(30);
    /// The locations one AssetKey may hold, the same as the wire limit.
    pub const MAX_LOCATIONS_PER_KEY: usize = 8;

    pub fn new(clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Self {
        Self {
            entries: HashMap::new(),
            profile_count: 0,
            clock,
        }
    }

    /// Adds the locations of one AssetKey, keeping one entry per node ID.
    ///
    /// The entry's `last_seen` moves to now even when every profile is already known, because receiving the advertisement again is what keeps it alive.
    pub fn insert(&mut self, key: Arc<AssetKey>, locations: impl IntoIterator<Item = Arc<NodeProfile>>) {
        let now = self.clock.now();
        let entry = self.entries.entry(key).or_insert_with(|| Entry {
            locations: Vec::new(),
            last_seen: now,
        });

        entry.last_seen = now;

        for location in locations {
            if entry.locations.len() >= Self::MAX_LOCATIONS_PER_KEY {
                break;
            }
            if entry.locations.iter().any(|known| known.id() == location.id()) {
                continue;
            }
            entry.locations.push(location);
            self.profile_count += 1;
        }

        self.shrink_if_needed();
    }

    /// The live locations of an AssetKey.
    pub fn get(&self, key: &AssetKey) -> &[Arc<NodeProfile>] {
        match self.entries.get(key) {
            Some(entry) if self.is_live(entry) => &entry.locations,
            _ => &[],
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn profile_count(&self) -> usize {
        self.profile_count
    }

    /// Drops the entries past their lifetime.
    pub fn refresh(&mut self) {
        let clock = self.clock.clone();
        let lifetime = Self::LIFETIME;
        self.entries.retain(|_, entry| clock.now() - entry.last_seen < lifetime);
        self.recount();
    }

    fn is_live(&self, entry: &Entry) -> bool {
        self.clock.now() - entry.last_seen < Self::LIFETIME
    }

    fn recount(&mut self) {
        self.profile_count = self.entries.values().map(|entry| entry.locations.len()).sum();
    }

    /// Evicts the entries seen longest ago until both limits hold.
    fn shrink_if_needed(&mut self) {
        if self.entries.len() <= Self::MAX_ASSET_KEYS && self.profile_count <= Self::MAX_NODE_PROFILES {
            return;
        }

        self.refresh();

        let mut keys: Vec<(Arc<AssetKey>, DateTime<Utc>)> = self.entries.iter().map(|(key, entry)| (key.clone(), entry.last_seen)).collect();
        keys.sort_by_key(|(_, last_seen)| std::cmp::Reverse(*last_seen));

        let mut kept = 0;
        let mut profiles = 0;
        let mut evicted: Vec<Arc<AssetKey>> = Vec::new();
        for (key, _) in keys {
            let entry = &self.entries[&key];
            let fits = kept < Self::MAX_ASSET_KEYS && profiles + entry.locations.len() <= Self::MAX_NODE_PROFILES;
            if !fits {
                evicted.push(key);
                continue;
            }
            kept += 1;
            profiles += entry.locations.len();
        }

        for key in evicted {
            self.entries.remove(&key);
        }
        self.profile_count = profiles;
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use testresult::TestResult;

    use omnius_core_base::clock::FakeClockUtc;
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use super::*;

    fn seconds(value: i64) -> std::time::Duration {
        std::time::Duration::from_secs(u64::try_from(value).unwrap())
    }

    fn clock() -> Arc<FakeClockUtc> {
        Arc::new(FakeClockUtc::new(Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap()))
    }

    fn profile(name: &str) -> Arc<NodeProfile> {
        Arc::new(NodeProfile::random_for_test(vec![OmniAddr::new(name)]))
    }

    fn key(name: &str) -> Arc<AssetKey> {
        use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
        Arc::new(AssetKey {
            typ: name.to_owned(),
            hash: OmniHash {
                typ: OmniHashAlgorithmType::Sha3_256,
                value: vec![1; 32],
            },
        })
    }

    #[test]
    fn a_location_survives_the_session_that_carried_it() -> TestResult {
        // The cache has no Session at all, so a value that was inserted is still there when every Session is gone.
        let mut cache = LocationCache::new(clock());
        let profile = profile("a");
        cache.insert(key("asset"), [profile.clone()]);

        assert_eq!(cache.get(&key("asset")), &[profile]);
        Ok(())
    }

    #[test]
    fn the_same_node_is_kept_once_and_the_entry_is_refreshed() -> TestResult {
        let clock = clock();
        let mut cache = LocationCache::new(clock.clone());
        let profile = profile("a");
        cache.insert(key("asset"), [profile.clone()]);
        cache.insert(key("asset"), [profile.clone()]);

        assert_eq!(cache.get(&key("asset")).len(), 1);
        assert_eq!(cache.profile_count(), 1);

        clock.advance(seconds(LocationCache::LIFETIME.num_seconds() - 1));
        cache.insert(key("asset"), []);
        clock.advance(seconds(2));
        assert_eq!(cache.get(&key("asset")).len(), 1, "receiving the entry again keeps it alive");

        clock.advance(seconds(LocationCache::LIFETIME.num_seconds()));
        assert!(cache.get(&key("asset")).is_empty());
        Ok(())
    }

    #[test]
    fn one_key_keeps_at_most_the_wire_limit() -> TestResult {
        let mut cache = LocationCache::new(clock());
        let locations: Vec<Arc<NodeProfile>> = (0..LocationCache::MAX_LOCATIONS_PER_KEY + 2).map(|i| profile(&format!("a{i}"))).collect();
        cache.insert(key("asset"), locations);

        assert_eq!(cache.get(&key("asset")).len(), LocationCache::MAX_LOCATIONS_PER_KEY);
        assert_eq!(cache.profile_count(), LocationCache::MAX_LOCATIONS_PER_KEY);
        Ok(())
    }

    #[test]
    fn the_oldest_entry_is_evicted_first() -> TestResult {
        let clock = clock();
        let mut cache = LocationCache::new(clock.clone());
        for i in 0..LocationCache::MAX_ASSET_KEYS {
            cache.insert(key(&format!("asset-{i:05}")), [profile(&format!("node-{i}"))]);
            clock.advance(std::time::Duration::from_millis(1));
        }
        assert_eq!(cache.len(), LocationCache::MAX_ASSET_KEYS);

        // The oldest entry is refreshed, so the second oldest becomes the eviction target.
        cache.insert(key("asset-00000"), [profile("node-0")]);
        cache.insert(key("late"), [profile("late")]);

        assert_eq!(cache.len(), LocationCache::MAX_ASSET_KEYS);
        assert!(!cache.get(&key("asset-00000")).is_empty());
        assert!(cache.get(&key("asset-00001")).is_empty());
        assert!(!cache.get(&key("late")).is_empty());
        Ok(())
    }
}
