use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};

use rand::{
    Rng,
    seq::{IndexedRandom as _, SliceRandom as _},
};
use tokio::time::Instant;

/// Records the transferred bytes per node ID and decides which nodes are unchoked.
/// Choke is judged per node, so every Session with the node conveys the same state.
pub struct ChokeManager {
    window: Duration,
    unchoke_count: usize,
    rotation: Duration,
    received: HashMap<Vec<u8>, VecDeque<(Instant, u64)>>,
    sent: HashMap<Vec<u8>, VecDeque<(Instant, u64)>>,
    unchoked: HashSet<Vec<u8>>,
    random_slot: Option<(Vec<u8>, Instant)>,
}

impl ChokeManager {
    pub fn new(window: Duration, unchoke_count: usize, rotation: Duration) -> Self {
        Self {
            window,
            unchoke_count,
            rotation,
            received: HashMap::new(),
            sent: HashMap::new(),
            unchoked: HashSet::new(),
            random_slot: None,
        }
    }

    /// Records the bytes of a block whose content hash matched.
    pub fn record_received(&mut self, node_id: &[u8], bytes: u64, now: Instant) {
        self.received.entry(node_id.to_vec()).or_default().push_back((now, bytes));
    }

    pub fn record_sent(&mut self, node_id: &[u8], bytes: u64, now: Instant) {
        self.sent.entry(node_id.to_vec()).or_default().push_back((now, bytes));
    }

    #[allow(unused)]
    pub fn sent_bytes(&self, node_id: &[u8]) -> u64 {
        self.sent.get(node_id).map_or(0, |entries| entries.iter().map(|(_, bytes)| bytes).sum())
    }

    /// Everything is choked until the first decision, because Sessions start choking the peer.
    pub fn is_choked(&self, node_id: &[u8]) -> bool {
        !self.unchoked.contains(node_id)
    }

    /// Unchokes the top nodes by the bytes received in the recent period, and one more node at random.
    pub fn decide(&mut self, wanting: &[Vec<u8>], now: Instant, rng: &mut (impl Rng + ?Sized)) {
        let window = self.window;
        self.received.retain(|_, entries| {
            while entries.front().is_some_and(|(at, _)| now.saturating_duration_since(*at) >= window) {
                entries.pop_front();
            }
            !entries.is_empty()
        });
        self.sent.retain(|_, entries| {
            while entries.front().is_some_and(|(at, _)| now.saturating_duration_since(*at) >= window) {
                entries.pop_front();
            }
            !entries.is_empty()
        });

        let mut candidates: Vec<&Vec<u8>> = wanting.iter().collect();
        candidates.shuffle(rng);
        // The sort is stable, so equal volumes stay in the shuffled order.
        candidates.sort_by_key(|node_id| std::cmp::Reverse(self.recent_received(node_id)));

        let mut unchoked: HashSet<Vec<u8>> = candidates.iter().take(self.unchoke_count).map(|node_id| (*node_id).clone()).collect();

        if self
            .random_slot
            .as_ref()
            .is_none_or(|(node_id, chosen_at)| now.saturating_duration_since(*chosen_at) >= self.rotation || !wanting.contains(node_id))
        {
            let rest: Vec<&Vec<u8>> = wanting.iter().filter(|node_id| !unchoked.contains(*node_id)).collect();
            self.random_slot = rest.choose(rng).map(|node_id| ((*node_id).clone(), now));
        }
        if let Some((node_id, _)) = &self.random_slot {
            unchoked.insert(node_id.clone());
        }
        self.unchoked = unchoked;
    }

    fn recent_received(&self, node_id: &[u8]) -> u64 {
        self.received.get(node_id).map_or(0, |entries| entries.iter().map(|(_, bytes)| bytes).sum())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::{SeedableRng as _, rngs::ChaCha20Rng};
    use tokio::time::Instant;

    use super::ChokeManager;

    fn nodes(count: u8) -> Vec<Vec<u8>> {
        (0..count).map(|i| vec![i]).collect()
    }

    fn manager() -> ChokeManager {
        ChokeManager::new(Duration::from_secs(600), 2, Duration::from_secs(30))
    }

    #[test]
    fn nodes_are_choked_until_the_first_decision() {
        assert!(manager().is_choked(&[0]));
    }

    #[test]
    fn the_nodes_that_gave_the_most_recently_are_unchoked_first_and_the_rest_are_choked() {
        let now = Instant::now();
        let mut rng = ChaCha20Rng::seed_from_u64(1);
        let mut manager = ChokeManager::new(Duration::from_secs(600), 2, Duration::from_secs(30));
        manager.record_received(&[3], 300, now);
        manager.record_received(&[1], 200, now);
        manager.record_received(&[2], 100, now);

        manager.decide(&nodes(5), now, &mut rng);

        assert!(!manager.is_choked(&[3]));
        assert!(!manager.is_choked(&[1]));
        // One more node is unchoked by the random slot, and the others are choked.
        let unchoked = (0..5u8).filter(|i| !manager.is_choked(&[*i])).count();
        assert_eq!(unchoked, 3);
    }

    #[test]
    fn a_node_that_stopped_giving_loses_its_rank_after_the_period() {
        let now = Instant::now();
        let mut rng = ChaCha20Rng::seed_from_u64(1);
        let mut manager = ChokeManager::new(Duration::from_secs(600), 1, Duration::from_secs(30));
        manager.record_received(&[0], 1000, now);
        manager.record_received(&[1], 10, now + Duration::from_secs(590));

        manager.decide(&nodes(2), now + Duration::from_secs(601), &mut rng);

        assert!(!manager.is_choked(&[1]));
    }

    #[test]
    fn only_wanting_nodes_are_unchoked_and_a_new_node_gets_the_random_slot() {
        let now = Instant::now();
        let mut rng = ChaCha20Rng::seed_from_u64(7);
        let mut manager = ChokeManager::new(Duration::from_secs(600), 1, Duration::from_secs(30));
        manager.record_received(&[0], 1000, now);

        manager.decide(&[vec![0], vec![9]], now, &mut rng);

        assert!(!manager.is_choked(&[0]));
        assert!(!manager.is_choked(&[9]));
        assert!(manager.is_choked(&[5]));
    }

    #[test]
    fn the_random_slot_is_kept_until_the_rotation_interval() {
        let now = Instant::now();
        let mut rng = ChaCha20Rng::seed_from_u64(3);
        let mut manager = ChokeManager::new(Duration::from_secs(600), 0, Duration::from_secs(30));

        manager.decide(&nodes(8), now, &mut rng);
        let chosen: Vec<u8> = (0..8u8).filter(|i| !manager.is_choked(&[*i])).collect();
        assert_eq!(chosen.len(), 1);

        manager.decide(&nodes(8), now + Duration::from_secs(10), &mut rng);
        assert!(!manager.is_choked(&[chosen[0]]));
    }

    #[test]
    fn sent_bytes_accumulate_per_node_and_the_record_is_pruned() {
        let mut manager = manager();
        let now = Instant::now();
        manager.record_sent(&[1], 10, now);
        manager.record_sent(&[1], 5, now);

        assert_eq!(manager.sent_bytes(&[1]), 15);
        assert_eq!(manager.sent_bytes(&[2]), 0);

        // The decision drops what the period no longer covers, as it does for the received bytes.
        let mut rng = ChaCha20Rng::seed_from_u64(1);
        manager.decide(&nodes(1), now + Duration::from_secs(11 * 60), &mut rng);
        assert_eq!(manager.sent_bytes(&[1]), 0);
    }
}
