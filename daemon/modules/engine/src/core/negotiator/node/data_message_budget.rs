use omnius_core_rocketpack::RocketPackBytesEncoder;
use std::sync::Arc;

use rand::seq::SliceRandom as _;

use crate::{
    model::{AssetKey, NodeProfile},
    prelude::*,
    protocol::{EncodedSize, MessageCodec, NodeProfileCodec},
};

use super::SendingDataMessage;

/// 件数で絞った候補から、全体の byte 予算に収まる情報を選ぶ。
pub(super) struct DataMessageBudget;

impl DataMessageBudget {
    pub fn select(message: SendingDataMessage, rng: &mut (impl rand::Rng + ?Sized)) -> SendingDataMessage {
        let mut queues: [Vec<Candidate>; 4] = [
            message.push_node_profiles.into_iter().map(Candidate::Profile).collect(),
            message.want_asset_keys.into_iter().map(Candidate::Want).collect(),
            message.give_asset_key_locations.into_iter().map(|(key, profiles)| Candidate::Give(key, profiles)).collect(),
            message.push_asset_key_locations.into_iter().map(|(key, profiles)| Candidate::Push(key, profiles)).collect(),
        ];
        for queue in &mut queues {
            queue.shuffle(rng);
        }
        let mut order = [0, 1, 2, 3];
        order.shuffle(rng);

        // struct map と field 番号に 5 byte、4 collection の header に各 3 byte。
        // 件数上限は 1024 以下なので、この余裕は空から最大件数までを覆う。
        let mut remaining = crate::generated::axus::node::MAX_MESSAGE_LENGTH as usize - 17;
        let mut selected = SendingDataMessage::default();
        while queues.iter().any(|queue| !queue.is_empty()) {
            for &index in &order {
                let Some(candidate) = queues[index].pop() else { continue };
                let size = match candidate.encoded_size() {
                    Ok(size) => size,
                    Err(error) => {
                        warn!(error_message = error.to_string(), "skipping invalid node message candidate");
                        continue;
                    }
                };
                if size > remaining {
                    continue;
                }
                remaining -= size;
                candidate.insert(&mut selected);
            }
        }
        selected
    }
}

enum Candidate {
    Profile(Arc<NodeProfile>),
    Want(Arc<AssetKey>),
    Give(Arc<AssetKey>, Vec<Arc<NodeProfile>>),
    Push(Arc<AssetKey>, Vec<Arc<NodeProfile>>),
}

impl Candidate {
    fn encoded_size(&self) -> std::result::Result<usize, RocketPackEncoderError> {
        match self {
            Self::Profile(profile) => EncodedSize::of(&NodeProfileCodec::to_wire(profile)),
            Self::Want(key) => EncodedSize::of(key.as_ref()),
            Self::Give(key, profiles) | Self::Push(key, profiles) => {
                let mut counter = EncodedSize::default();
                let mut encoder = RocketPackBytesEncoder::new(&mut counter);
                AssetKey::pack(&mut encoder, key)?;
                encoder.write_array(profiles.len())?;
                for profile in profiles {
                    <NodeProfileCodec as MessageCodec>::Wire::pack(&mut encoder, &NodeProfileCodec::to_wire(profile))?;
                }
                Ok(counter.len)
            }
        }
    }

    fn insert(self, message: &mut SendingDataMessage) {
        match self {
            Self::Profile(profile) => message.push_node_profiles.push(profile),
            Self::Want(key) => message.want_asset_keys.push(key),
            Self::Give(key, profiles) => {
                message.give_asset_key_locations.insert(key, profiles);
            }
            Self::Push(key, profiles) => {
                message.push_asset_key_locations.insert(key, profiles);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{core::negotiator::node::DataMessage, generated::axus, protocol::node::DataMessageCodec};
    use omnius_core_omnikit::{
        generated::omni_hash::{OmniHash, OmniHashAlgorithmType},
        model::omni_addr::OmniAddr,
    };
    use rand::{SeedableRng, rngs::ChaCha20Rng};
    use testresult::TestResult;

    fn maximum_candidates() -> SendingDataMessage {
        let profile = Arc::new(NodeProfile::new(vec![1; 256], vec![OmniAddr::new("a".repeat(512)); NodeProfile::MAX_WIRE_ADDRS]));
        let keys: Vec<_> = (0..DataMessage::MAX_WANT_ASSET_KEYS)
            .map(|i| {
                Arc::new(AssetKey {
                    typ: format!("{i:064}"),
                    hash: OmniHash {
                        typ: OmniHashAlgorithmType::Sha3_256,
                        value: vec![1; 64],
                    },
                })
            })
            .collect();
        SendingDataMessage {
            push_node_profiles: vec![profile.clone(); DataMessage::MAX_PUSH_NODE_PROFILES],
            want_asset_keys: keys.clone(),
            give_asset_key_locations: keys
                .iter()
                .map(|key| (key.clone(), vec![profile.clone(); DataMessage::MAX_LOCATION_NODE_PROFILES]))
                .collect(),
            push_asset_key_locations: keys.into_iter().map(|key| (key, vec![profile.clone(); DataMessage::MAX_LOCATION_NODE_PROFILES])).collect(),
        }
    }

    fn message(value: SendingDataMessage) -> DataMessage {
        DataMessage {
            push_node_profiles: value.push_node_profiles,
            want_asset_keys: value.want_asset_keys,
            give_asset_key_locations: value.give_asset_key_locations,
            push_asset_key_locations: value.push_asset_key_locations,
        }
    }

    #[test]
    fn maximum_field_values_are_budgeted_across_all_categories() -> TestResult {
        let full = message(maximum_candidates());
        let encoded_size = EncodedSize::of(&DataMessageCodec::to_wire(&full))?;
        assert_eq!(encoded_size, 72_386_544);
        assert!(matches!(
            DataMessageCodec::encode(&full),
            Err(RocketPackEncoderError::LengthOutOfRange { context: "DataMessage", .. })
        ));
        for seed in 0..8 {
            let selected = message(DataMessageBudget::select(maximum_candidates(), &mut ChaCha20Rng::seed_from_u64(seed)));
            assert!(!selected.push_node_profiles.is_empty());
            assert!(!selected.want_asset_keys.is_empty());
            assert!(!selected.give_asset_key_locations.is_empty());
            assert!(!selected.push_asset_key_locations.is_empty());
            assert!(
                selected
                    .give_asset_key_locations
                    .values()
                    .chain(selected.push_asset_key_locations.values())
                    .all(|profiles| profiles.len() == DataMessage::MAX_LOCATION_NODE_PROFILES)
            );
            let bytes = DataMessageCodec::encode(&selected)?;
            assert!(bytes.len() <= axus::node::MAX_MESSAGE_LENGTH as usize);
            assert_eq!(EncodedSize::of(&DataMessageCodec::to_wire(&selected))?, bytes.len());
            assert_eq!(DataMessageCodec::decode(&bytes)?, selected);
        }
        Ok(())
    }

    #[test]
    fn invalid_candidate_does_not_block_valid_information() -> TestResult {
        let mut candidates = SendingDataMessage::default();
        candidates.push_node_profiles.push(Arc::new(NodeProfile::new(vec![0; 257], vec![])));
        candidates.push_node_profiles.push(Arc::new(NodeProfile::new(vec![1], vec![])));
        candidates.want_asset_keys.push(Arc::new(AssetKey {
            typ: "a".repeat(65),
            hash: OmniHash {
                typ: OmniHashAlgorithmType::None,
                value: vec![],
            },
        }));
        let selected = message(DataMessageBudget::select(candidates, &mut ChaCha20Rng::seed_from_u64(0)));
        assert_eq!(selected.push_node_profiles.len(), 1);
        assert!(selected.want_asset_keys.is_empty());
        assert_eq!(DataMessageCodec::decode(&DataMessageCodec::encode(&selected)?)?, selected);
        Ok(())
    }
}
