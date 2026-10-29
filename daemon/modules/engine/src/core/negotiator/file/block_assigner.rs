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

/// Decides which block each Session requests, so that one block is requested from one Session at a time.
/// It keeps the state per root hash and does no I/O; the Sessions only send and receive.
pub struct BlockAssigner {
    rank_absent_retry: Duration,
    roots: HashMap<OmniHash, RootAssignments>,
}

struct RootAssignments {
    wanted: Vec<WantedBlock>,
    assigned: HashMap<OmniHash, u64>,
    /// The ranks the node answered with no present bit, which it is not asked for again for a while.
    rank_absent: Expirations<(Vec<u8>, u32)>,
}

impl BlockAssigner {
    pub fn new(rank_absent_retry: Duration) -> Self {
        Self {
            rank_absent_retry,
            roots: HashMap::new(),
        }
    }

    /// Replaces the blocks to fetch. An assignment of a block that is no longer wanted is dropped.
    pub fn update_wanted(&mut self, root_hash: &OmniHash, wanted: Vec<WantedBlock>) {
        let root = self.roots.entry(root_hash.clone()).or_insert_with(RootAssignments::new);
        let wanted_set: HashSet<&OmniHash> = wanted.iter().map(|wanted| &wanted.block_hash).collect();
        root.assigned.retain(|hash, _| wanted_set.contains(hash));
        root.wanted = wanted;
    }

    /// Forgets the root hashes that are not fetched anymore.
    pub fn retain_roots(&mut self, root_hashes: &HashSet<OmniHash>) {
        self.roots.retain(|root_hash, _| root_hashes.contains(root_hash));
    }

    /// Assigns up to `limit` unassigned blocks to the Session, skipping the ranks the node answered it does not have.
    pub fn assign(&mut self, root_hash: &OmniHash, session_id: u64, node_id: &[u8], limit: usize, now: Instant) -> Vec<WantedBlock> {
        let Some(root) = self.roots.get_mut(root_hash) else {
            return Vec::new();
        };
        let mut assigned = Vec::new();
        for wanted in &root.wanted {
            if assigned.len() >= limit {
                break;
            }
            if root.assigned.contains_key(&wanted.block_hash) || root.rank_absent.contains(&(node_id.to_vec(), wanted.rank), now) {
                continue;
            }
            assigned.push(wanted.clone());
        }
        for wanted in &assigned {
            root.assigned.insert(wanted.block_hash.clone(), session_id);
        }
        assigned
    }

    /// The block is saved, so it is not requested anymore.
    pub fn complete(&mut self, root_hash: &OmniHash, block_hash: &OmniHash) {
        if let Some(root) = self.roots.get_mut(root_hash) {
            root.assigned.remove(block_hash);
            root.wanted.retain(|wanted| &wanted.block_hash != block_hash);
        }
    }

    /// Returns the blocks to unassigned, so that another Session can request them.
    pub fn release(&mut self, root_hash: &OmniHash, block_hashes: impl IntoIterator<Item = OmniHash>) {
        if let Some(root) = self.roots.get_mut(root_hash) {
            for hash in block_hashes {
                root.assigned.remove(&hash);
            }
        }
    }

    pub fn release_session(&mut self, root_hash: &OmniHash, session_id: u64) {
        if let Some(root) = self.roots.get_mut(root_hash) {
            root.assigned.retain(|_, assigned_to| *assigned_to != session_id);
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
    fn new() -> Self {
        Self {
            wanted: Vec::new(),
            assigned: HashMap::new(),
            rank_absent: Expirations::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Duration};

    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
    use tokio::time::Instant;

    use super::{BlockAssigner, WantedBlock};

    fn hash(value: u8) -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [value])
    }

    fn wanted(value: u8, rank: u32) -> WantedBlock {
        WantedBlock { block_hash: hash(value), rank }
    }

    fn assigner() -> (BlockAssigner, OmniHash, Instant) {
        let mut assigner = BlockAssigner::new(Duration::from_secs(30));
        let root = hash(0);
        assigner.update_wanted(&root, vec![wanted(1, 0), wanted(2, 0), wanted(3, 1)]);
        (assigner, root, Instant::now())
    }

    #[test]
    fn a_block_is_assigned_to_only_one_session() {
        let (mut assigner, root, now) = assigner();

        assert_eq!(assigner.assign(&root, 1, b"a", 2, now), vec![wanted(1, 0), wanted(2, 0)]);
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now), vec![wanted(3, 1)]);
        assert!(assigner.assign(&root, 3, b"c", 5, now).is_empty());
    }

    #[test]
    fn released_and_closed_session_blocks_are_assigned_again() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);

        assigner.release(&root, [hash(2)]);
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now), vec![wanted(2, 0)]);

        assigner.release_session(&root, 1);
        assert_eq!(assigner.assign(&root, 3, b"c", 5, now), vec![wanted(1, 0), wanted(3, 1)]);
    }

    #[test]
    fn a_rank_the_node_answered_with_no_present_bit_is_not_asked_until_the_retry_time() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);
        assigner.mark_rank_absent(&root, b"a", [0], now);
        assigner.release(&root, [hash(1), hash(2)]);

        // The rank the node lacks is not assigned to it...
        assert!(assigner.assign(&root, 1, b"a", 5, now + Duration::from_secs(29)).is_empty());
        // ...and stays unasked even after the other ranks are done.
        assigner.complete(&root, &hash(3));
        assert!(assigner.assign(&root, 1, b"a", 5, now + Duration::from_secs(29)).is_empty());
        // Another node is asked for the same rank.
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now + Duration::from_secs(29)), vec![wanted(1, 0), wanted(2, 0)]);
        // The node is asked again once the period is over.
        assigner.release(&root, [hash(1), hash(2)]);
        assert_eq!(assigner.assign(&root, 1, b"a", 5, now + Duration::from_secs(30)), vec![wanted(1, 0), wanted(2, 0)]);
    }

    #[test]
    fn a_rank_the_node_answered_with_some_present_bit_is_asked_again() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);
        // A mixed answer only releases the absent block and does not stop the rank.
        assigner.release(&root, [hash(2)]);

        assert_eq!(assigner.assign(&root, 1, b"a", 5, now), vec![wanted(2, 0)]);
    }

    #[test]
    fn a_saved_block_is_not_wanted_and_an_update_drops_stale_assignments() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);

        assigner.complete(&root, &hash(1));
        assigner.update_wanted(&root, vec![wanted(3, 1), wanted(4, 1)]);
        // The assignment of the block that is still wanted is kept.
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now), vec![wanted(4, 1)]);

        assigner.retain_roots(&HashSet::new());
        assert!(assigner.assign(&root, 2, b"b", 5, now).is_empty());
    }
}
