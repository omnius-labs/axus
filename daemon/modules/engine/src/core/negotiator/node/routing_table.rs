use std::{collections::HashMap, sync::Arc};

use crate::{
    core::negotiator::node::{Kadex, KnownNode},
    model::NodeProfile,
};

/// Picks known nodes by their XOR distance to a target.
pub struct RoutingTable;

impl RoutingTable {
    /// The IDs closest to `target`, skipping `base`, ordered by XOR distance.
    ///
    /// `Kadex::find` pads its result with the node itself and stops at it, which drops every candidate when the asking node is the closest one, so the distance is sorted here.
    pub fn closest_ids<'a>(base: &[u8], target: &[u8], ids: &[&'a [u8]], count: usize) -> Vec<&'a [u8]> {
        let mut entries: Vec<&[u8]> = ids.iter().copied().filter(|id| *id != base).collect();
        entries.sort_by(|left, right| Kadex::compare(&Self::distance(target, left), &Self::distance(target, right)));
        entries.truncate(count);
        entries
    }

    /// The XOR distance between a target and an ID, compared by [`Kadex::compare`].
    pub fn distance(target: &[u8], id: &[u8]) -> Vec<u8> {
        target.iter().zip(id).map(|(left, right)| left ^ right).collect()
    }

    /// Whether `id` is closer to `target` than `than` is.
    ///
    /// The comparison must go through [`Kadex::compare`], which reads the distance from its last byte; ordering the byte vectors directly gives a different answer.
    pub fn is_closer(target: &[u8], id: &[u8], than: &[u8]) -> bool {
        Kadex::compare(&Self::distance(target, id), &Self::distance(target, than)) == std::cmp::Ordering::Less
    }

    /// The known nodes closest to `target`, skipping the node itself and `exclude`.
    ///
    /// A lookup uses this to choose whom to ask, and an answer uses it to tell the asker who is closer.
    pub fn closest(base: &[u8], target: &[u8], known: &[KnownNode], count: usize, exclude: &[u8]) -> Vec<Arc<NodeProfile>> {
        if count == 0 {
            return Vec::new();
        }

        let by_id: HashMap<&[u8], &NodeProfile> = known.iter().map(|known| (known.profile.id(), &known.profile)).collect();
        let ids: Vec<&[u8]> = by_id.keys().copied().filter(|id| *id != exclude).collect();

        Self::closest_ids(base, target, &ids, count)
            .into_iter()
            .filter_map(|id| by_id.get(id).map(|profile| Arc::new((*profile).clone())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::{
        generated::omni_hash::{OmniHash, OmniHashAlgorithmType},
        model::omni_addr::OmniAddr,
    };

    use super::*;

    fn known(id: u8, addrs: &[u8]) -> KnownNode {
        let profile = NodeProfile::random_for_test(vec![OmniAddr::new("addr")]);
        // The XOR distance uses the node ID, which is derived from the public key, so the test builds the entry around that key.
        let _ = (id, addrs);
        KnownNode {
            profile,
            last_seen: 0,
            fail_count: 0,
            last_attempt: 0,
        }
    }

    #[test]
    fn the_node_itself_and_the_excluded_one_are_not_returned() {
        let known: Vec<KnownNode> = (0..4).map(|i| known(i, &[])).collect();
        let base = known[0].profile.id();
        let target = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"target").value;

        let closest = RoutingTable::closest(base, &target, &known, 8, known[1].profile.id());
        assert_eq!(closest.len(), 2);
        assert!(closest.iter().all(|profile| profile.id() != base));
        assert!(closest.iter().all(|profile| profile.id() != known[1].profile.id()));
    }

    #[test]
    fn closeness_follows_the_byte_order_of_kadex() {
        // 距離は A = [1, 0]、B = [0, 1]。バイト列の辞書順では B が近いが、Kadex は末尾バイトから読むので A が近い
        let target = [0u8, 0];
        let a = [1u8, 0];
        let b = [0u8, 1];

        assert!(RoutingTable::is_closer(&target, &a, &b));
        assert!(!RoutingTable::is_closer(&target, &b, &a));
    }

    #[test]
    fn at_most_the_requested_count_is_returned() {
        let known: Vec<KnownNode> = (0..4).map(|i| known(i, &[])).collect();
        let base = known[0].profile.id();
        let target = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"target").value;

        assert_eq!(RoutingTable::closest(base, &target, &known, 2, &[]).len(), 2);
    }
}
