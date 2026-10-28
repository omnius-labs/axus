use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use omnius_core_omnikit::generated::omni_hash::OmniHash;
use tokio::time::Instant;

use super::expirations::Expirations;

/// Decides which block each Session requests, so that one block is requested from one Session at a time.
/// It keeps the state per root hash and does no I/O; the Sessions only send and receive.
pub struct BlockAssigner {
    not_have_retry: Duration,
    roots: HashMap<OmniHash, RootAssignments>,
}

struct RootAssignments {
    wanted: Vec<OmniHash>,
    assigned: HashMap<OmniHash, u64>,
    not_have: Expirations<(Vec<u8>, OmniHash)>,
}

impl BlockAssigner {
    pub fn new(not_have_retry: Duration) -> Self {
        Self {
            not_have_retry,
            roots: HashMap::new(),
        }
    }

    /// Replaces the blocks to fetch. An assignment of a block that is no longer wanted is dropped.
    pub fn update_wanted(&mut self, root_hash: &OmniHash, wanted: Vec<OmniHash>) {
        let root = self.roots.entry(root_hash.clone()).or_insert_with(RootAssignments::new);
        let wanted_set: HashSet<&OmniHash> = wanted.iter().collect();
        root.assigned.retain(|hash, _| wanted_set.contains(hash));
        root.wanted = wanted;
    }

    /// Forgets the root hashes that are not fetched anymore.
    pub fn retain_roots(&mut self, root_hashes: &HashSet<OmniHash>) {
        self.roots.retain(|root_hash, _| root_hashes.contains(root_hash));
    }

    /// Assigns up to `limit` unassigned blocks to the Session, skipping the ones the node answered it does not have.
    pub fn assign(&mut self, root_hash: &OmniHash, session_id: u64, node_id: &[u8], limit: usize, now: Instant) -> Vec<OmniHash> {
        let Some(root) = self.roots.get_mut(root_hash) else {
            return Vec::new();
        };
        let mut assigned = Vec::new();
        for hash in &root.wanted {
            if assigned.len() >= limit {
                break;
            }
            if root.assigned.contains_key(hash) || root.not_have.contains(&(node_id.to_vec(), hash.clone()), now) {
                continue;
            }
            assigned.push(hash.clone());
        }
        for hash in &assigned {
            root.assigned.insert(hash.clone(), session_id);
        }
        assigned
    }

    /// The block is saved, so it is not requested anymore.
    pub fn complete(&mut self, root_hash: &OmniHash, block_hash: &OmniHash) {
        if let Some(root) = self.roots.get_mut(root_hash) {
            root.assigned.remove(block_hash);
            root.wanted.retain(|hash| hash != block_hash);
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

    /// The node does not hold the block, so it is not asked for it again for a while.
    pub fn mark_not_have(&mut self, root_hash: &OmniHash, node_id: &[u8], block_hash: &OmniHash, now: Instant) {
        if let Some(root) = self.roots.get_mut(root_hash) {
            root.assigned.remove(block_hash);
            root.not_have.insert((node_id.to_vec(), block_hash.clone()), now + self.not_have_retry);
        }
    }
}

impl RootAssignments {
    fn new() -> Self {
        Self {
            wanted: Vec::new(),
            assigned: HashMap::new(),
            not_have: Expirations::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, time::Duration};

    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
    use tokio::time::Instant;

    use super::BlockAssigner;

    fn hash(value: u8) -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [value])
    }

    fn assigner() -> (BlockAssigner, OmniHash, Instant) {
        let mut assigner = BlockAssigner::new(Duration::from_secs(30));
        let root = hash(0);
        assigner.update_wanted(&root, vec![hash(1), hash(2), hash(3)]);
        (assigner, root, Instant::now())
    }

    #[test]
    fn a_block_is_assigned_to_only_one_session() {
        let (mut assigner, root, now) = assigner();

        assert_eq!(assigner.assign(&root, 1, b"a", 2, now), vec![hash(1), hash(2)]);
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now), vec![hash(3)]);
        assert!(assigner.assign(&root, 3, b"c", 5, now).is_empty());
    }

    #[test]
    fn released_and_closed_session_blocks_are_assigned_again() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);

        assigner.release(&root, [hash(2)]);
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now), vec![hash(2)]);

        assigner.release_session(&root, 1);
        assert_eq!(assigner.assign(&root, 3, b"c", 5, now), vec![hash(1), hash(3)]);
    }

    #[test]
    fn a_not_have_responder_is_not_asked_for_the_block_until_the_retry_time() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);
        assigner.mark_not_have(&root, b"a", &hash(1), now);

        assert!(assigner.assign(&root, 1, b"a", 5, now + Duration::from_secs(29)).is_empty());
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now + Duration::from_secs(29)), vec![hash(1)]);
        assigner.release(&root, [hash(1)]);
        assert_eq!(assigner.assign(&root, 1, b"a", 5, now + Duration::from_secs(30)), vec![hash(1)]);
    }

    #[test]
    fn a_saved_block_is_not_wanted_and_an_update_drops_stale_assignments() {
        let (mut assigner, root, now) = assigner();
        assigner.assign(&root, 1, b"a", 3, now);

        assigner.complete(&root, &hash(1));
        assigner.update_wanted(&root, vec![hash(3), hash(4)]);
        assert_eq!(assigner.assign(&root, 2, b"b", 5, now), vec![hash(4)]);

        assigner.retain_roots(&HashSet::new());
        assert!(assigner.assign(&root, 2, b"b", 5, now).is_empty());
    }
}
