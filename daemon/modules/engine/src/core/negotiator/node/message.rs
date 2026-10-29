use std::{collections::HashMap, sync::Arc};

use enumflags2::BitFlags;

use crate::model::{AssetKey, NodeProfile};

/// How many hops an advertisement may still travel. The node that starts it sends it with this value.
pub(crate) const MAX_HOPS: u32 = crate::generated::axus::node::MAX_HOPS;

#[repr(u32)]
#[enumflags2::bitflags]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub(crate) enum NodeFinderVersion {
    V1 = 1,
}

/// The nodes that advertise an AssetKey and how far the advertisement may still travel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnnounceLocation {
    pub hops: u32,
    pub node_profiles: Vec<Arc<NodeProfile>>,
}

/// One message per purpose.
///
/// Hello and Profile precede the established phase; a Status keeps the Session alive once nothing else is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeMessage {
    Hello {
        version: BitFlags<NodeFinderVersion>,
    },
    Profile {
        node_profile: NodeProfile,
    },
    Status,
    AnnounceProfiles {
        node_profiles: Vec<Arc<NodeProfile>>,
    },
    AnnounceLocations {
        locations: HashMap<Arc<AssetKey>, AnnounceLocation>,
    },
    LookupRequest {
        lookup_id: u64,
        asset_key: AssetKey,
    },
    LookupResponse {
        lookup_id: u64,
        locations: Vec<Arc<NodeProfile>>,
        closer_nodes: Vec<Arc<NodeProfile>>,
    },
}

impl NodeMessage {
    pub const MAX_ANNOUNCE_NODE_PROFILES: usize = crate::generated::axus::node::MAX_ANNOUNCE_NODE_PROFILES as usize;
    pub const MAX_CLOSER_NODES: usize = crate::generated::axus::node::MAX_CLOSER_NODES as usize;
    pub const MAX_ANNOUNCE_ASSET_KEYS: usize = crate::generated::axus::node::MAX_ANNOUNCE_ASSET_KEYS as usize;
}
