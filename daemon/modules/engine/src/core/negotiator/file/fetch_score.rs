use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use tokio::time::Instant;

/// The throughput of the blocks received from each node, used to order the dialing candidates and to set how
/// many requests one Session keeps awaiting responses. It records the same blocks as the choke ranking, but as
/// bytes per second over the recent period, so a node that stopped providing falls in the order.
pub struct FetchScore {
    window: Duration,
    min_blocks: usize,
    received: HashMap<Vec<u8>, VecDeque<(Instant, u64)>>,
}

impl FetchScore {
    pub fn new(window: Duration, min_blocks: usize) -> Self {
        Self {
            window,
            min_blocks,
            received: HashMap::new(),
        }
    }

    /// Records a block whose content hash matched.
    pub fn record(&mut self, node_id: &[u8], bytes: u64, now: Instant) {
        let window = self.window;
        let entries = self.received.entry(node_id.to_vec()).or_default();
        entries.push_back((now, bytes));
        while entries.front().is_some_and(|(at, _)| now.saturating_duration_since(*at) >= window) {
            entries.pop_front();
        }
    }

    /// The bytes per second of the node in the recent period, or None while it provided too few blocks there.
    /// Blocks older than the period are ignored, so a node that stopped providing falls out of the order.
    pub fn throughput(&self, node_id: &[u8], now: Instant) -> Option<f64> {
        let entries: Vec<&(Instant, u64)> = self
            .received
            .get(node_id)?
            .iter()
            .filter(|(at, _)| now.saturating_duration_since(*at) < self.window)
            .collect();
        if entries.len() < self.min_blocks {
            return None;
        }
        let bytes: u64 = entries.iter().map(|(_, bytes)| *bytes).sum();
        let span = entries
            .first()
            .map_or(Duration::ZERO, |(at, _)| now.saturating_duration_since(*at))
            // A burst of blocks arrives within less than a second, and dividing by it would overstate the rate.
            .max(Duration::from_secs(1));
        Some(bytes as f64 / span.as_secs_f64())
    }

    /// The median throughput of the nodes that provided enough blocks. It is the reference the pending limit of a
    /// scored node is scaled against; the dial order places a node without a record at a random position instead.
    pub fn median(&self, now: Instant) -> Option<f64> {
        let mut values: Vec<f64> = self.received.keys().filter_map(|node_id| self.throughput(node_id, now)).collect();
        if values.is_empty() {
            return None;
        }
        values.sort_by(|left, right| left.total_cmp(right));
        let middle = values.len() / 2;
        if values.len().is_multiple_of(2) {
            Some((values[middle - 1] + values[middle]) / 2.0)
        } else {
            Some(values[middle])
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    use super::FetchScore;

    #[test]
    fn a_node_counts_only_after_the_minimum_number_of_blocks() {
        let now = Instant::now();
        let mut score = FetchScore::new(Duration::from_secs(600), 4);

        for _ in 0..3 {
            score.record(b"a", 1_000, now);
        }
        assert_eq!(score.throughput(b"a", now), None);

        score.record(b"a", 1_000, now);
        assert_eq!(score.throughput(b"a", now), Some(4_000.0));
        assert_eq!(score.throughput(b"b", now), None);
    }

    #[test]
    fn the_throughput_is_measured_over_the_time_the_blocks_took() {
        let start = Instant::now();
        let mut score = FetchScore::new(Duration::from_secs(600), 4);
        for second in 0..4 {
            score.record(b"fast", 1_000_000, start + Duration::from_secs(second));
        }
        for second in 0..4 {
            score.record(b"slow", 1_000, start + Duration::from_secs(second * 5));
        }

        let fast = score.throughput(b"fast", start + Duration::from_secs(3)).unwrap();
        let slow = score.throughput(b"slow", start + Duration::from_secs(15)).unwrap();
        assert!(fast > slow * 100.0);
    }

    #[test]
    fn a_node_that_stopped_providing_falls_after_the_period() {
        let start = Instant::now();
        let mut score = FetchScore::new(Duration::from_secs(60), 4);
        for second in 0..4 {
            score.record(b"a", 1_000, start + Duration::from_secs(second));
        }
        assert!(score.throughput(b"a", start + Duration::from_secs(3)).is_some());
        assert_eq!(score.throughput(b"a", start + Duration::from_secs(61)), None);
    }

    #[test]
    fn the_median_of_the_known_nodes_is_between_them() {
        let start = Instant::now();
        let mut score = FetchScore::new(Duration::from_secs(600), 4);
        for second in 0..4 {
            score.record(b"fast", 10_000, start + Duration::from_secs(second));
            score.record(b"slow", 1_000, start + Duration::from_secs(second));
        }

        let now = start + Duration::from_secs(3);
        let fast = score.throughput(b"fast", now).unwrap();
        let slow = score.throughput(b"slow", now).unwrap();
        let median = score.median(now).unwrap();
        assert!(median > slow && median < fast);
    }
}
