use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use enumflags2::BitFlags;

use crate::{
    core::negotiator::node::{DataMessage, HelloMessage, NodeFinderVersion, ProfileMessage},
    generated::axus::model::NodeProfile as WireNodeProfile,
    generated::axus::node as wire,
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::{MessageCodec, NodeProfileCodec};

type WireLocations = BTreeMap<AssetKey, Vec<WireNodeProfile>>;
type DomainLocations = HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>;

pub(crate) struct HelloMessageCodec;

impl MessageCodec for HelloMessageCodec {
    type Message = HelloMessage;
    type Wire = wire::HelloMessage;

    fn to_wire(value: &HelloMessage) -> Self::Wire {
        Self::Wire { version: value.version.bits() }
    }

    fn from_wire(value: Self::Wire) -> std::result::Result<HelloMessage, RocketPackDecoderError> {
        Ok(HelloMessage {
            version: BitFlags::<NodeFinderVersion>::from_bits_truncate(value.version),
        })
    }
}

pub(crate) struct ProfileMessageCodec;

impl MessageCodec for ProfileMessageCodec {
    type Message = ProfileMessage;
    type Wire = wire::ProfileMessage;

    fn to_wire(value: &ProfileMessage) -> Self::Wire {
        Self::Wire {
            node_profile: NodeProfileCodec::to_wire(&value.node_profile),
        }
    }

    fn from_wire(value: Self::Wire) -> std::result::Result<ProfileMessage, RocketPackDecoderError> {
        Ok(ProfileMessage {
            node_profile: NodeProfileCodec::from_wire(value.node_profile)?,
        })
    }
}

pub(crate) struct DataMessageCodec;

impl MessageCodec for DataMessageCodec {
    type Message = DataMessage;
    type Wire = wire::DataMessage;

    fn to_wire(value: &DataMessage) -> Self::Wire {
        Self::Wire {
            push_node_profiles: value.push_node_profiles.iter().map(|profile| NodeProfileCodec::to_wire(profile)).collect(),
            want_asset_keys: value.want_asset_keys.iter().map(|key| key.as_ref().clone()).collect(),
            give_asset_key_locations: value
                .give_asset_key_locations
                .iter()
                .map(|(key, profiles)| (key.as_ref().clone(), profiles.iter().map(|profile| NodeProfileCodec::to_wire(profile)).collect()))
                .collect(),
            push_asset_key_locations: value
                .push_asset_key_locations
                .iter()
                .map(|(key, profiles)| (key.as_ref().clone(), profiles.iter().map(|profile| NodeProfileCodec::to_wire(profile)).collect()))
                .collect(),
        }
    }

    fn from_wire(value: Self::Wire) -> std::result::Result<DataMessage, RocketPackDecoderError> {
        Ok(DataMessage {
            push_node_profiles: Self::profiles(value.push_node_profiles)?,
            want_asset_keys: value.want_asset_keys.into_iter().map(Arc::new).collect(),
            give_asset_key_locations: Self::locations(value.give_asset_key_locations)?,
            push_asset_key_locations: Self::locations(value.push_asset_key_locations)?,
        })
    }
}

impl DataMessageCodec {
    fn profiles(values: Vec<WireNodeProfile>) -> std::result::Result<Vec<Arc<NodeProfile>>, RocketPackDecoderError> {
        values.into_iter().map(|value| NodeProfileCodec::from_wire(value).map(Arc::new)).collect()
    }

    fn locations(values: WireLocations) -> std::result::Result<DomainLocations, RocketPackDecoderError> {
        values.into_iter().map(|(key, profiles)| Ok((Arc::new(key), Self::profiles(profiles)?))).collect()
    }
}

impl DataMessage {
    pub const MAX_PUSH_NODE_PROFILES: usize = wire::MAX_PUSH_NODE_PROFILES as usize;
    pub const MAX_WANT_ASSET_KEYS: usize = wire::MAX_WANT_ASSET_KEYS as usize;
    pub const MAX_GIVE_ASSET_KEY_LOCATIONS: usize = wire::MAX_GIVE_ASSET_KEY_LOCATIONS as usize;
    pub const MAX_PUSH_ASSET_KEY_LOCATIONS: usize = wire::MAX_PUSH_ASSET_KEY_LOCATIONS as usize;
    pub const MAX_LOCATION_NODE_PROFILES: usize = wire::MAX_LOCATION_NODE_PROFILES as usize;
}
