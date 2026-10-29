use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use chrono::{DateTime, Duration, Utc};
use omnius_core_base::clock::Clock;

use crate::model::AssetKey;

struct Relayed {
    /// The peers that asked for this AssetKey.
    peers: HashSet<Vec<u8>>,
    last_seen: DateTime<Utc>,
}

/// The AssetKeys this node asks for, and the ones its peers asked it to relay.
///
/// The local wants come from the higher layer and do not expire; a relayed want lives as long as one of its peers keeps asking.
pub struct WantCache {
    local: HashSet<Arc<AssetKey>>,
    relayed: HashMap<Arc<AssetKey>, Relayed>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

impl WantCache {
    /// The most AssetKeys one node keeps, counting the local and the relayed ones.
    pub const MAX_ASSET_KEYS: usize = 4096;
    /// How long a relayed want is kept after the last peer asked for it.
    pub const LIFETIME: Duration = Duration::minutes(30);

    pub fn new(clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Self {
        Self {
            local: HashSet::new(),
            relayed: HashMap::new(),
            clock,
        }
    }

    /// Replaces the AssetKeys the higher layer asks for.
    pub fn set_local(&mut self, keys: impl IntoIterator<Item = Arc<AssetKey>>) {
        self.local = keys.into_iter().collect();
        self.shrink_if_needed();
    }

    /// Records that `peer` asked for these AssetKeys.
    pub fn insert_relayed(&mut self, peer: &[u8], keys: impl IntoIterator<Item = Arc<AssetKey>>) {
        let now = self.clock.now();
        for key in keys {
            let entry = self.relayed.entry(key).or_insert_with(|| Relayed {
                peers: HashSet::new(),
                last_seen: now,
            });
            entry.peers.insert(peer.to_vec());
            entry.last_seen = now;
        }
        self.shrink_if_needed();
    }

    #[cfg(test)]
    pub fn local_keys(&self) -> Vec<Arc<AssetKey>> {
        self.local.iter().cloned().collect()
    }

    /// The live relayed AssetKeys, whoever asked for them.
    pub fn relayed_keys(&self) -> Vec<Arc<AssetKey>> {
        self.relayed.iter().filter(|(_, entry)| self.is_live(entry)).map(|(key, _)| key.clone()).collect()
    }

    /// The live AssetKeys one peer asked for.
    pub fn relayed_keys_of(&self, peer: &[u8]) -> Vec<Arc<AssetKey>> {
        self.relayed
            .iter()
            .filter(|(_, entry)| self.is_live(entry) && entry.peers.contains(peer))
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// Every live AssetKey this node asks for, either for itself or on behalf of a peer.
    pub fn keys(&self) -> Vec<Arc<AssetKey>> {
        let mut keys: HashSet<Arc<AssetKey>> = self.local.clone();
        keys.extend(self.relayed_keys());
        keys.into_iter().collect()
    }

    pub fn refresh(&mut self) {
        let clock = self.clock.clone();
        let lifetime = Self::LIFETIME;
        self.relayed.retain(|_, entry| clock.now() - entry.last_seen < lifetime);
    }

    fn is_live(&self, entry: &Relayed) -> bool {
        self.clock.now() - entry.last_seen < Self::LIFETIME
    }

    fn shrink_if_needed(&mut self) {
        if self.local.len() + self.relayed.len() <= Self::MAX_ASSET_KEYS {
            return;
        }

        self.refresh();

        let room = Self::MAX_ASSET_KEYS.saturating_sub(self.local.len());
        let mut relayed: Vec<(Arc<AssetKey>, DateTime<Utc>)> = self.relayed.iter().map(|(key, entry)| (key.clone(), entry.last_seen)).collect();
        relayed.sort_by_key(|(_, last_seen)| std::cmp::Reverse(*last_seen));
        let keep: HashSet<Arc<AssetKey>> = relayed.into_iter().take(room).map(|(key, _)| key).collect();
        self.relayed.retain(|key, _| keep.contains(key));
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use testresult::TestResult;

    use omnius_core_base::clock::FakeClockUtc;
    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

    use super::*;

    fn seconds(value: i64) -> std::time::Duration {
        std::time::Duration::from_secs(u64::try_from(value).unwrap())
    }

    fn clock() -> Arc<FakeClockUtc> {
        Arc::new(FakeClockUtc::new(Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap()))
    }

    fn key(name: &str) -> Arc<AssetKey> {
        Arc::new(AssetKey {
            typ: name.to_owned(),
            hash: OmniHash {
                typ: OmniHashAlgorithmType::Sha3_256,
                value: vec![1; 32],
            },
        })
    }

    #[test]
    fn a_local_want_does_not_expire() -> TestResult {
        let clock = clock();
        let mut cache = WantCache::new(clock.clone());
        cache.set_local([key("mine")]);
        clock.advance(seconds(WantCache::LIFETIME.num_seconds() * 2));

        assert_eq!(cache.local_keys(), vec![key("mine")]);
        assert_eq!(cache.keys(), vec![key("mine")]);
        Ok(())
    }

    #[test]
    fn a_relayed_want_expires_after_its_last_peer() -> TestResult {
        let clock = clock();
        let mut cache = WantCache::new(clock.clone());
        cache.insert_relayed(b"peer", [key("theirs")]);
        assert_eq!(cache.relayed_keys_of(b"peer"), vec![key("theirs")]);

        clock.advance(seconds(WantCache::LIFETIME.num_seconds() - 1));
        cache.insert_relayed(b"peer", [key("theirs")]);
        clock.advance(seconds(2));
        assert_eq!(cache.relayed_keys(), vec![key("theirs")], "asking again keeps it alive");

        clock.advance(seconds(WantCache::LIFETIME.num_seconds()));
        assert!(cache.relayed_keys().is_empty());
        assert!(cache.relayed_keys_of(b"peer").is_empty());
        Ok(())
    }

    #[test]
    fn a_want_is_answered_only_for_the_peer_that_asked() -> TestResult {
        let mut cache = WantCache::new(clock());
        cache.insert_relayed(b"a", [key("one")]);
        cache.insert_relayed(b"b", [key("two")]);

        assert_eq!(cache.relayed_keys_of(b"a"), vec![key("one")]);
        assert_eq!(cache.relayed_keys_of(b"b"), vec![key("two")]);
        Ok(())
    }

    #[test]
    fn relayed_wants_leave_room_for_the_local_ones() -> TestResult {
        let clock = clock();
        let mut cache = WantCache::new(clock.clone());
        cache.set_local([key("mine")]);
        for i in 0..WantCache::MAX_ASSET_KEYS {
            cache.insert_relayed(b"peer", [key(&format!("theirs-{i:05}"))]);
            clock.advance(std::time::Duration::from_millis(1));
        }

        assert_eq!(cache.local_keys(), vec![key("mine")]);
        assert_eq!(cache.relayed_keys().len(), WantCache::MAX_ASSET_KEYS - 1);
        Ok(())
    }
}
