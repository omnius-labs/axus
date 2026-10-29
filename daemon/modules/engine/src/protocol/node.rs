use std::{collections::HashMap, sync::Arc};

use enumflags2::BitFlags;

use crate::{
    core::negotiator::node::{AnnounceLocation, MAX_HOPS, NodeFinderVersion, NodeMessage},
    generated::axus::model::NodeProfile as WireNodeProfile,
    generated::axus::node as wire,
    model::NodeProfile,
    prelude::*,
};

use super::{EncodedSize, MessageCodec, NodeProfileCodec};

/// The largest frame a NodeFinder message may be.
pub const MAX_FRAME_LENGTH: usize = wire::MAX_MESSAGE_LENGTH as usize;

pub(crate) struct NodeMessageCodec;

impl MessageCodec for NodeMessageCodec {
    type Message = NodeMessage;
    type Wire = wire::NodeMessage;

    fn encode(value: &NodeMessage) -> std::result::Result<Vec<u8>, RocketPackEncoderError> {
        let wire = Self::to_wire(value);
        let size = EncodedSize::of(&wire)?;
        omnius_core_rocketpack::validate_length("NodeMessage", 0, wire::MAX_MESSAGE_LENGTH as u64, size)?;
        let bytes = wire.export()?;
        omnius_core_rocketpack::validate_length("NodeMessage", 0, wire::MAX_MESSAGE_LENGTH as u64, bytes.len())?;
        Ok(bytes)
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<NodeMessage, RocketPackDecoderError> {
        decoder.validate_length("NodeMessage", 0, wire::MAX_MESSAGE_LENGTH as u64, decoder.remaining() as u64, decoder.position())?;
        Self::from_wire(Self::Wire::unpack(decoder)?)
    }

    fn to_wire(value: &NodeMessage) -> Self::Wire {
        match value {
            NodeMessage::Hello { version } => wire::NodeMessage::Hello { version: version.bits() },
            NodeMessage::Profile { node_profile } => wire::NodeMessage::Profile {
                node_profile: NodeProfileCodec::to_wire(node_profile),
            },
            NodeMessage::Status => wire::NodeMessage::Status,
            NodeMessage::AnnounceProfiles { node_profiles } => wire::NodeMessage::AnnounceProfiles {
                node_profiles: node_profiles.iter().map(|profile| NodeProfileCodec::to_wire(profile)).collect(),
            },
            NodeMessage::AnnounceLocations { locations } => wire::NodeMessage::AnnounceLocations {
                locations: locations
                    .iter()
                    .map(|(key, location)| {
                        (
                            key.as_ref().clone(),
                            wire::LocationEntry {
                                hops: location.hops,
                                node_profiles: location.node_profiles.iter().map(|profile| NodeProfileCodec::to_wire(profile)).collect(),
                            },
                        )
                    })
                    .collect(),
            },
        }
    }

    fn from_wire(value: Self::Wire) -> std::result::Result<NodeMessage, RocketPackDecoderError> {
        let message = match value {
            wire::NodeMessage::Hello { version } => NodeMessage::Hello {
                version: BitFlags::<NodeFinderVersion>::from_bits_truncate(version),
            },
            wire::NodeMessage::Profile { node_profile } => NodeMessage::Profile {
                node_profile: NodeProfileCodec::from_wire(node_profile)?,
            },
            wire::NodeMessage::Status => NodeMessage::Status,
            wire::NodeMessage::AnnounceProfiles { node_profiles } => NodeMessage::AnnounceProfiles {
                node_profiles: Self::profiles(node_profiles)?,
            },
            wire::NodeMessage::AnnounceLocations { locations } => {
                let mut converted = HashMap::new();
                for (key, location) in locations {
                    // Reject an advertisement that claims more hops than the protocol allows, before it is relayed.
                    if location.hops > MAX_HOPS {
                        return Err(RocketPackDecoderError::Other("advertisement exceeds the hop limit"));
                    }
                    converted.insert(
                        Arc::new(key),
                        AnnounceLocation {
                            hops: location.hops,
                            node_profiles: Self::profiles(location.node_profiles)?,
                        },
                    );
                }
                NodeMessage::AnnounceLocations { locations: converted }
            }
        };

        Ok(message)
    }
}

impl NodeMessageCodec {
    fn profiles(values: Vec<WireNodeProfile>) -> std::result::Result<Vec<Arc<NodeProfile>>, RocketPackDecoderError> {
        values.into_iter().map(|value| NodeProfileCodec::from_wire(value).map(Arc::new)).collect()
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_rocketpack::{RocketPackBytesDecoder, RocketPackBytesEncoder};

    use crate::model::AssetKey;
    use omnius_core_omnikit::{
        generated::omni_hash::{OmniHash, OmniHashAlgorithmType},
        model::omni_addr::OmniAddr,
    };
    use testresult::TestResult;

    use super::*;

    fn profile() -> Arc<NodeProfile> {
        Arc::new(NodeProfile::random_for_test(vec![OmniAddr::new("addr")]))
    }

    fn asset_key(value: u8) -> Arc<AssetKey> {
        Arc::new(AssetKey {
            typ: "test".to_string(),
            hash: OmniHash {
                typ: OmniHashAlgorithmType::Sha3_256,
                value: vec![value; 32],
            },
        })
    }

    #[test]
    fn every_variant_roundtrips() -> TestResult {
        let messages = vec![
            NodeMessage::Hello {
                version: BitFlags::from(NodeFinderVersion::V1),
            },
            NodeMessage::Profile {
                node_profile: profile().as_ref().clone(),
            },
            NodeMessage::Status,
            NodeMessage::AnnounceProfiles {
                node_profiles: vec![profile(), profile()],
            },
            NodeMessage::AnnounceLocations {
                locations: HashMap::from([(
                    asset_key(1),
                    AnnounceLocation {
                        hops: MAX_HOPS,
                        node_profiles: vec![profile()],
                    },
                )]),
            },
        ];

        for message in messages {
            let bytes = NodeMessageCodec::encode(&message)?;
            assert!(bytes.len() <= MAX_FRAME_LENGTH);
            assert_eq!(NodeMessageCodec::decode(&bytes)?, message);
        }
        Ok(())
    }

    #[test]
    fn an_advertisement_over_the_hop_limit_is_rejected() -> TestResult {
        let message = wire::NodeMessage::AnnounceLocations {
            locations: [(
                (*asset_key(1)).clone(),
                wire::LocationEntry {
                    hops: MAX_HOPS + 1,
                    node_profiles: vec![],
                },
            )]
            .into_iter()
            .collect(),
        };

        assert!(NodeMessageCodec::decode(&message.export()?).is_err());
        Ok(())
    }

    #[test]
    fn oversized_counts_are_rejected_before_reading_elements() -> TestResult {
        // The outer map holds the variant tag, and the payload is a map with the field number.
        for (variant, max, context, use_map) in [
            (4u64, NodeMessage::MAX_ANNOUNCE_NODE_PROFILES, "NodeMessage.AnnounceProfiles.node_profiles", false),
            (5, NodeMessage::MAX_ANNOUNCE_ASSET_KEYS, "NodeMessage.AnnounceLocations.locations", true),
        ] {
            let mut bytes = Vec::new();
            let mut encoder = RocketPackBytesEncoder::new(&mut bytes);
            encoder.write_map(1)?;
            encoder.write_u64(variant)?;
            encoder.write_map(1)?;
            encoder.write_u64(1)?;
            if use_map {
                encoder.write_map(max + 1)?;
            } else {
                encoder.write_array(max + 1)?;
            }
            let elements_start = bytes.len();
            // 要素は decode できない null とし、件数だけで拒否することを確かめる
            bytes.resize(elements_start + (max + 1) * 2, 0xf6);
            let mut decoder = RocketPackBytesDecoder::new(&bytes);
            let error = NodeMessageCodec::unpack(&mut decoder).unwrap_err();
            assert!(matches!(error, RocketPackDecoderError::LengthOutOfRange { context: actual, .. } if actual == context));
            assert_eq!(decoder.position(), elements_start);
        }
        Ok(())
    }
}
