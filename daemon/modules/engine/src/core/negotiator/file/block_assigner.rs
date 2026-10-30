use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use omnius_core_omnikit::generated::omni_hash::OmniHash;
use tokio::time::Instant;

use super::expirations::Expirations;

/// A block to fetch and the rank it belongs to. The rank names the group a peer lacks as a whole, so that an
/// answer with no present bit stops the node from being asked for that rank instead of for each absent hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WantedBlock {
    pub block_hash: OmniHash,
    pub rank: u32,
}

/// The thresholds of the tail duplication, where one block is assigned to two Sessions.
#[derive(Debug, Clone)]
pub struct TailOption {
    pub remaining: usize,
    pub stall_remaining: usize,
    pub stall: Duration,
    pub copies: usize,
}

/// Decides which block each Session requests.
/// A block is requested from one Session at a time, except in the tail of a root hash, where it is requested
/// from two, because the slowest peer holding one of the last blocks would otherwise decide the completion time.
/// It keeps the state per root hash and does no I/O; the Sessions only send and receive.
pub struct BlockAssigner {
    rank_absent_retry: Duration,
    tail: TailOption,
    roots: HashMap<OmniHash, RootAssignments>,
}

struct RootAssignments {
    wanted: Vec<WantedBlock>,
    assigned: HashMap<OmniHash, Vec<u64>>,
    /// The ranks the node answered with no present bit, which it is not asked for again for a while.
    rank_absent: Expirations<(Vec<u8>, u32)>,
    /// When a block of the root hash was last saved, which decides the tail when few blocks are left.
    last_progress: Instant,
}

impl BlockAssigner {
    pub fn new(rank_absent_retry: Duration, tail: TailOption) -> Self {
        Self {
            rank_absent_retry,
            tail,
            roots: HashMap::new(),
        }
    }

    /// Replaces the blocks to fetch. An assignment of a block that is no longer wanted is dropped.
    pub fn update_wanted(&mut self, root_hash: &OmniHash, wanted: Vec<WantedBlock>, now: Instant) {
        let root = self.roots.entry(root_hash.clone()).or_insert_with(|| RootAssignments::new(now));
        let wanted_set: HashSet<&OmniHash> = wanted.iter().map(|wanted| &wanted.block_hash).collect();
        root.assigned.retain(|hash, _| wanted_set.contains(hash));
        root.wanted = wanted;
    }

    /// Forgets the root hashes that are not fetched anymore.
    pub fn retain_roots(&mut self, root_hashes: &HashSet<OmniHash>) {
        self.roots.retain(|root_hash, _| root_hashes.contains(root_hash));
    }

    /// Assigns up to `limit` blocks to the Session, skipping the ranks the node answered it does not have.
    /// A block is assigned again only while fewer Sessions hold it than the tail allows.
    pub fn assign(&mut self, root_hash: &OmniHash, session_id: u64, node_id: &[u8], limit: usize, now: Instant) -> Vec<WantedBlock> {
        let Some(root) = self.roots.get_mut(root_hash) else {
            return Vec::new();
        };
        let wanted_count = root.wanted.len();
        let idle = now.saturating_duration_since(root.last_progress);
        let tail = &self.tail;
        let copies = if wanted_count <= tail.remaining || (wanted_count <= tail.stall_remaining && idle >= tail.stall) {
            tail.copies
        } else {
            1
        };

        let mut assigned = Vec::new();
        for wanted in &root.wanted {
            if assigned.len() >= limit {
                break;
            }
            if root.rank_absent.contains(&(node_id.to_vec(), wanted.rank), now) {
                continue;
            }
            match root.assigned.get(&wanted.block_hash) {
                Some(sessions) if sessions.len() >= copies || sessions.contains(&session_id) => continue,
                _ => {}
            }
            assigned.push(wanted.clone());
        }
        for wanted in &assigned {
            root.assigned.entry(wanted.block_hash.clone()).or_default().push(session_id);
        }
        assigned
    }

    /// The block is saved, so it is not requested anymore.
    pub fn complete(&mut self, root_hash: &OmniHash, block_hash: &OmniHash, now: Instant) {
        if let Some(root) = self.roots.get_mut(root_hash) {
            root.assigned.remove(block_hash);
            root.wanted.retain(|wanted| &wanted.block_hash != block_hash);
            root.last_progress = now;
        }
    }

    /// Whether the Session is still expected to answer with the block, which the cross-completion drops.
    pub fn is_assigned(&self, root_hash: &OmniHash, session_id: u64, block_hash: &OmniHash) -> bool {
        self.roots
            .get(root_hash)
            .and_then(|root| root.assigned.get(block_hash))
            .is_some_and(|sessions| sessions.contains(&session_id))
    }

    /// Returns the blocks to unassigned for that Session, so that another Session can request them.
    pub fn release(&mut self, root_hash: &OmniHash, session_id: u64, block_hashes: impl IntoIterator<Item = OmniHash>) {
        if let Some(root) = self.roots.get_mut(root_hash) {
            for hash in block_hashes {
                if let Some(sessions) = root.assigned.get_mut(&hash) {
                    sessions.retain(|assigned_to| *assigned_to != session_id);
                    if sessions.is_empty() {
                        root.assigned.remove(&hash);
                    }
                }
            }
        }
    }

    pub fn release_session(&mut self, root_hash: &OmniHash, session_id: u64) {
        if let Some(root) = self.roots.get_mut(root_hash) {
            root.assigned.retain(|_, sessions| {
                sessions.retain(|assigned_to| *assigned_to != session_id);
                !sessions.is_empty()
            });
        }
    }

    /// The node answered that it holds none of the request's blocks, so it is not asked for those ranks for a while.
    pub fn mark_rank_absent(&mut self, root_hash: &OmniHash, node_id: &[u8], ranks: impl IntoIterator<Item = u32>, now: Instant) {
        let Some(root) = self.roots.get_mut(root_hash) else {
            return;
        };
        for rank in ranks {
            root.rank_absent.insert((node_id.to_vec(), rank), now + self.rank_absent_retry);
        }
    }
}

impl RootAssignments {
    fn new(now: Instant) -> Self {
        Self {
            wanted: Vec::new(),
            assigned: HashMap::new(),
            rank_absent: Expirations::new(),
            last_progress: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Duration};

    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
    use tokio::time::Instant;

    use super::{BlockAssigner, TailOption, WantedBlock};

    fn hash(value: u8) -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [value])
    }

    fn wanted(value: u8, rank: u32) -> WantedBlock {
        WantedBlock { block_hash: hash(value), rank }
    }

    fn assigner_with(wanted: Vec<WantedBlock>) -> (BlockAssigner, OmniHash, Instant) {
        let tail = TailOption {
            remaining: 32,
            stall_remaining: 256,
            stall: Duration::from_secs(15),
            copies: 2,
        };
        let mut assigner = BlockAssigner::new(Duration::from_secs(30), tail);
        let root = hash(0);
        let now = Instant::now();
        assigner.update_wanted(&root, wanted, now);
        (assigner, root, now)
    }

    fn assigner() -> (BlockAssigner, OmniHash, Instant) {
        assigner_with(vec![wanted(1, 0), wanted(2, 0), wanted(3, 1)])
    }

    #[test]
    fn a_block_is_assigned_to_only_one_session_while_the_root_is_not_in_the_tail() {
        let (mut assigner, root, now) = assigner_with((1..=100).map(|value| wanted(value, 0)).collect());

        assert_eq!(assigner.assign(&root, 1, b"a", 2, now), vec![wanted(1, 0), wanted(2, 0)]);
        // Outside the tail the second Session gets other blocks, not a copy of the first Session's.
        assert_eq!(assigner.assign(&root, 2, b"b", 3, now), vec![wanted(3, 0), wanted(4, 0), wanted(5, 0)]);
    }

    #[test]
    fn released_and_closed_session_blocks_are_assigned_again() {
        let (mut assigner, root, now) = assigner_with((1..=100).map(|value| wanted(value, 0)).collect());
        assigner.assign(&root, 1, b"a", 3, now);

        assigner.release(&root, 1, [hash(2)]);
        assert_eq!(
            assigner.assign(&root, 2, b"b", 5, now),
            vec![wanted(2, 0), wanted(4, 0), wanted(5, 0), wanted(6, 0), wanted(7, 0)]
        );

        assigner.release_session(&root, 1);
        assert_eq!(assigner.assign(&root, 3, b"c", 2, now), vec![wanted(1, 0), wanted(3, 0)]);
    }

    #[test]
    fn a_rank_the_node_answered_with_no_present_bit_is_not_asked_until_the_retry_time() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);
        assigner.release(&root, 1, [hash(1), hash(2)]);
        assigner.mark_rank_absent(&root, b"a", [0], now);

        // The rank the node lacks is not assigned to it...
        assert!(assigner.assign(&root, 1, b"a", 5, now + Duration::from_secs(29)).is_empty());
        // Another node is asked for the same rank.
        assert_eq!(
            assigner.assign(&root, 2, b"b", 5, now + Duration::from_secs(29)),
            vec![wanted(1, 0), wanted(2, 0), wanted(3, 1)]
        );
        // The node is asked again once the period is over.
        assigner.release(&root, 2, [hash(1), hash(2), hash(3)]);
        assert_eq!(assigner.assign(&root, 1, b"a", 5, now + Duration::from_secs(30)), vec![wanted(1, 0), wanted(2, 0)]);
    }

    #[test]
    fn a_rank_the_node_answered_with_some_present_bit_is_asked_again() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);
        // A mixed answer only releases the absent block and does not stop the rank.
        assigner.release(&root, 1, [hash(2)]);

        assert_eq!(assigner.assign(&root, 1, b"a", 5, now), vec![wanted(2, 0)]);
    }

    #[test]
    fn a_saved_block_is_not_wanted_and_an_update_drops_stale_assignments() {
        let (mut assigner, root, now) = assigner_with((1..=100).map(|value| wanted(value, 0)).collect());
        assigner.assign(&root, 1, b"a", 3, now);

        assigner.complete(&root, &hash(1), now);
        assigner.update_wanted(&root, vec![wanted(3, 1), wanted(4, 1)], now);
        // Two blocks are the tail, so the block another Session holds gets its second copy.
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now), vec![wanted(3, 1), wanted(4, 1)]);

        assigner.retain_roots(&HashSet::new());
        assert!(assigner.assign(&root, 2, b"b", 5, now).is_empty());
    }

    #[test]
    fn a_block_in_the_tail_is_assigned_to_two_sessions_and_completed_by_the_first() {
        let (mut assigner, root, now) = assigner_with(vec![wanted(1, 0), wanted(2, 0)]);

        assert_eq!(assigner.assign(&root, 1, b"a", 5, now), vec![wanted(1, 0), wanted(2, 0)]);
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now), vec![wanted(1, 0), wanted(2, 0)]);
        assert!(assigner.assign(&root, 3, b"c", 5, now).is_empty());

        // The first stored copy completes the block, so the other Session is no longer expected to answer.
        assert!(assigner.is_assigned(&root, 2, &hash(1)));
        assigner.complete(&root, &hash(1), now);
        assert!(!assigner.is_assigned(&root, 2, &hash(1)));
        assert!(assigner.is_assigned(&root, 2, &hash(2)));
    }

    #[test]
    fn the_tail_also_starts_when_nothing_is_saved_for_the_stall_time() {
        let (mut assigner, root, now) = assigner_with((1..=200).map(|value| wanted(value, 0)).collect());
        // Nothing saved yet, but the stall time has not passed, so the second Session gets another block.
        let first = assigner.assign(&root, 1, b"a", 1, now + Duration::from_secs(14));
        let second = assigner.assign(&root, 2, b"b", 1, now + Duration::from_secs(14));
        assert_ne!(first, second);

        // After the stall the same block is offered to the second Session as the second copy.
        assert_eq!(assigner.assign(&root, 2, b"b", 1, now + Duration::from_secs(15)), first);

        // A saved block moves the tail back to the count of the remaining blocks.
        assigner.complete(&root, &first[0].block_hash, now + Duration::from_secs(15));
        let next = assigner.assign(&root, 2, b"b", 1, now + Duration::from_secs(16));
        assert_ne!(Some(&next[0]), None);
    }
}
