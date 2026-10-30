use std::{collections::HashMap, time::Duration};

use tokio::time::Instant;

/// The buckets one node's record is kept in. The window is split into this many buckets, so a node costs a fixed
/// size however many blocks it provides and the throughput is a fixed number of additions.
const BUCKET_COUNT: usize = 16;

/// The bytes and blocks of one node in the recent period, bucketed by time.
struct Ring {
    bytes: [u64; BUCKET_COUNT],
    blocks: [u32; BUCKET_COUNT],
    next: usize,
    started_at: Instant,
    /// When the oldest block still in the ring arrived, which the rate is measured over.
    first_at: Instant,
}

impl Ring {
    fn new(now: Instant) -> Self {
        Self {
            bytes: [0; BUCKET_COUNT],
            blocks: [0; BUCKET_COUNT],
            next: 0,
            started_at: now,
            first_at: now,
        }
    }

    /// Moves the ring forward to `now` and returns whether anything is left in it.
    fn advance(&mut self, now: Instant, bucket: Duration) -> bool {
        let elapsed = now.saturating_duration_since(self.started_at);
        let steps = (elapsed.as_nanos() / bucket.as_nanos().max(1)) as usize;
        if steps >= BUCKET_COUNT {
            self.bytes = [0; BUCKET_COUNT];
            self.blocks = [0; BUCKET_COUNT];
            self.next = 0;
            self.started_at = now;
        } else if steps > 0 {
            for step in 0..steps {
                let index = (self.next + step) % BUCKET_COUNT;
                self.bytes[index] = 0;
                self.blocks[index] = 0;
            }
            self.next = (self.next + steps) % BUCKET_COUNT;
            self.started_at += bucket * steps as u32;
        }
        self.total_bytes() > 0
    }

    fn total_bytes(&self) -> u64 {
        self.bytes.iter().sum()
    }

    fn total_blocks(&self) -> u64 {
        self.blocks.iter().map(|blocks| u64::from(*blocks)).sum()
    }

    fn throughput(&self, now: Instant, window: Duration) -> f64 {
        let span = now.saturating_duration_since(self.first_at).clamp(Duration::from_secs(1), window);
        self.total_bytes() as f64 / span.as_secs_f64()
    }
}

/// The throughput of the blocks received from each node, used to order the dialing candidates and to set how
/// many requests one Session keeps awaiting responses. It records the same blocks as the choke ranking, but as
/// bytes per second over the recent period, so a node that stopped providing falls in the order.
pub struct FetchScore {
    window: Duration,
    bucket: Duration,
    min_blocks: usize,
    median_ttl: Duration,
    nodes: HashMap<Vec<u8>, Ring>,
    median: Option<(Instant, Option<f64>)>,
}

impl FetchScore {
    pub fn new(window: Duration, min_blocks: usize) -> Self {
        Self {
            window,
            bucket: window / BUCKET_COUNT as u32,
            min_blocks,
            median_ttl: window / 10,
            nodes: HashMap::new(),
            median: None,
        }
    }

    /// Records a block whose content hash matched and was stored.
    pub fn record(&mut self, node_id: &[u8], bytes: u64, now: Instant) {
        let bucket = self.bucket;
        let is_new = !self.nodes.contains_key(node_id);
        let ring = self.nodes.entry(node_id.to_vec()).or_insert_with(|| Ring::new(now));
        // A ring that went empty starts a new period, so the rate does not count the idle time before it.
        if is_new || !ring.advance(now, bucket) {
            ring.first_at = now;
        }
        ring.bytes[ring.next] += bytes;
        ring.blocks[ring.next] = ring.blocks[ring.next].saturating_add(1);
    }

    /// The bytes per second of the node in the recent period, or None while it provided too few blocks there.
    /// Blocks older than the period are ignored, so a node that stopped providing falls out of the order.
    pub fn throughput(&mut self, node_id: &[u8], now: Instant) -> Option<f64> {
        let bucket = self.bucket;
        let window = self.window;
        let min_blocks = self.min_blocks as u64;
        let ring = self.nodes.get_mut(node_id)?;
        if !ring.advance(now, bucket) {
            self.nodes.remove(node_id);
            return None;
        }
        if ring.total_blocks() < min_blocks {
            return None;
        }
        Some(ring.throughput(now, window))
    }

    /// The median throughput of the nodes that provided enough blocks. It is the reference the pending limit of a
    /// scored node is scaled against; the dial order places a node without a record at a random position instead.
    /// The value is kept for a fraction of the period, so neither the dial order nor the pending limit walks every
    /// node on every block.
    pub fn median(&mut self, now: Instant) -> Option<f64> {
        if let Some((cached_at, value)) = self.median
            && now.saturating_duration_since(cached_at) < self.median_ttl
        {
            return value;
        }

        let bucket = self.bucket;
        let window = self.window;
        let min_blocks = self.min_blocks as u64;
        // A node whose period is over is dropped here, so the record of a node that left does not stay for ever.
        self.nodes.retain(|_, ring| ring.advance(now, bucket));

        let mut values: Vec<f64> = self
            .nodes
            .values()
            .filter(|ring| ring.total_blocks() >= min_blocks)
            .map(|ring| ring.throughput(now, window))
            .collect();
        let value = if values.is_empty() {
            None
        } else {
            values.sort_by(|left, right| left.total_cmp(right));
            let middle = values.len() / 2;
            if values.len().is_multiple_of(2) {
                Some((values[middle - 1] + values[middle]) / 2.0)
            } else {
                Some(values[middle])
            }
        };
        self.median = Some((now, value));
        value
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
    fn a_node_that_stopped_providing_falls_after_the_period_and_is_forgotten() {
        let start = Instant::now();
        let mut score = FetchScore::new(Duration::from_secs(60), 4);
        for second in 0..4 {
            score.record(b"a", 1_000, start + Duration::from_secs(second));
        }
        assert!(score.throughput(b"a", start + Duration::from_secs(3)).is_some());
        assert_eq!(score.throughput(b"a", start + Duration::from_secs(61)), None);

        // The next median drops the node whose period is over instead of walking its old buckets forever.
        assert_eq!(score.median(start + Duration::from_secs(61)), None);
        assert_eq!(score.node_count(), 0);
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

    #[test]
    fn the_median_is_kept_for_a_fraction_of_the_period() {
        let start = Instant::now();
        let mut score = FetchScore::new(Duration::from_secs(60), 4);
        for second in 0..4 {
            score.record(b"a", 1_000, start + Duration::from_secs(second));
        }
        let first = score.median(start + Duration::from_secs(3)).unwrap();

        // A second node does not change the median until the cache expires.
        for _ in 0..4 {
            score.record(b"b", 1_000_000, start + Duration::from_secs(3));
        }
        assert_eq!(score.median(start + Duration::from_secs(3)), Some(first));
        assert_ne!(score.median(start + Duration::from_secs(30)), Some(first));
    }

    #[test]
    fn the_record_does_not_grow_with_the_blocks() {
        let start = Instant::now();
        let mut score = FetchScore::new(Duration::from_secs(600), 4);
        for block in 0..10_000u32 {
            score.record(b"a", 1, start + Duration::from_millis(block as u64));
        }
        assert_eq!(score.node_count(), 1);
        assert!(score.throughput(b"a", start + Duration::from_secs(10)).is_some());
    }
}

#[cfg(test)]
impl FetchScore {
    /// The nodes still recorded, for the test that checks a departed node is forgotten.
    fn node_count(&self) -> usize {
        self.nodes.len()
    }
}
