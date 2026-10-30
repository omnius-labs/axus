use omnius_core_rocketpack::{RocketPackBytesEncoder, RocketPackStruct as _};
use std::{collections::HashMap, sync::Arc};

use crate::{
    core::negotiator::node::{AnnounceLocation, NodeMessage},
    generated::axus::node as wire,
    model::AssetKey,
    protocol::{EncodedSize, MessageCodec, NodeProfileCodec},
};

/// Narrows the locations of one advertisement to what fits in one frame.
pub(crate) struct AnnounceLocationsBudget;

impl AnnounceLocationsBudget {
    pub fn select(locations: HashMap<Arc<AssetKey>, AnnounceLocation>) -> HashMap<Arc<AssetKey>, AnnounceLocation> {
        if locations.is_empty() {
            return locations;
        }

        // struct map と variant の tag に余裕を取り、key と entry の大きさを足していく
        let mut remaining = crate::generated::axus::node::MAX_MESSAGE_LENGTH as usize - 32;
        let mut selected = HashMap::new();
        for (asset_key, location) in locations {
            if selected.len() >= NodeMessage::MAX_ANNOUNCE_ASSET_KEYS {
                break;
            }
            let Ok(size) = Self::encoded_size(&asset_key, &location) else {
                continue;
            };
            if size > remaining {
                continue;
            }
            remaining -= size;
            selected.insert(asset_key, location);
        }
        selected
    }

    fn encoded_size(asset_key: &Arc<AssetKey>, location: &AnnounceLocation) -> std::result::Result<usize, omnius_core_rocketpack::RocketPackEncoderError> {
        let mut counter = EncodedSize::default();
        let mut encoder = RocketPackBytesEncoder::new(&mut counter);
        AssetKey::pack(&mut encoder, asset_key)?;
        wire::LocationEntry::pack(
            &mut encoder,
            &wire::LocationEntry {
                hops: location.hops,
                node_profiles: location.node_profiles.iter().map(|profile| NodeProfileCodec::to_wire(profile)).collect(),
            },
        )?;
        Ok(counter.len)
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use omnius_core_omnikit::{
        generated::omni_hash::{OmniHash, OmniHashAlgorithmType},
        model::omni_addr::OmniAddr,
    };

    use super::*;
    use crate::{
        model::NodeProfile,
        protocol::node::{MAX_FRAME_LENGTH, NodeMessageCodec},
    };

    fn profile() -> Arc<NodeProfile> {
        Arc::new(NodeProfile::random_for_test(vec![OmniAddr::new("a".repeat(512)); NodeProfile::MAX_WIRE_ADDRS]))
    }

    fn key(i: usize) -> Arc<AssetKey> {
        Arc::new(AssetKey {
            typ: format!("{i:064}"),
            hash: OmniHash {
                typ: OmniHashAlgorithmType::Sha3_256,
                value: vec![1; 64],
            },
        })
    }

    #[test]
    fn the_selected_locations_fit_in_one_frame() -> TestResult {
        let locations: HashMap<_, _> = (0..NodeMessage::MAX_ANNOUNCE_ASSET_KEYS)
            .map(|i| {
                (
                    key(i),
                    AnnounceLocation {
                        hops: 3,
                        node_profiles: vec![profile(); crate::generated::axus::node::MAX_LOCATION_NODE_PROFILES as usize],
                    },
                )
            })
            .collect();

        let selected = AnnounceLocationsBudget::select(locations);
        assert!(!selected.is_empty());
        assert!(selected.len() < NodeMessage::MAX_ANNOUNCE_ASSET_KEYS, "the byte budget binds before the count limit");

        let bytes = NodeMessageCodec::encode(&NodeMessage::AnnounceLocations { locations: selected })?;
        assert!(bytes.len() <= MAX_FRAME_LENGTH);
        Ok(())
    }
}
