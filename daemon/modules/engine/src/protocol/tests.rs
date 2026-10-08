pub(crate) mod legacy;
use crate::{
    core::negotiator::node::DataMessage,
    model::{AssetKey, NodeProfile},
};
use std::{collections::HashMap, sync::Arc};
#[cfg(test)]
pub(crate) fn legacy_profile(value: &NodeProfile) -> legacy::NodeProfile {
    legacy::NodeProfile::new(value.public_key().to_vec(), value.addrs.clone())
}

#[cfg(test)]
pub(crate) fn legacy_data(value: &DataMessage) -> legacy::DataMessage {
    let convert_key = |key: &AssetKey| legacy::AssetKey {
        typ: key.typ.clone(),
        hash: key.hash.clone(),
    };
    let locations = |values: &HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>| {
        values
            .iter()
            .map(|(key, profiles)| (Arc::new(convert_key(key)), profiles.iter().map(|p| Arc::new(legacy_profile(p))).collect()))
            .collect()
    };
    legacy::DataMessage {
        push_node_profiles: value.push_node_profiles.iter().map(|p| Arc::new(legacy_profile(p))).collect(),
        want_asset_keys: value.want_asset_keys.iter().map(|key| Arc::new(convert_key(key))).collect(),
        give_asset_key_locations: locations(&value.give_asset_key_locations),
        push_asset_key_locations: locations(&value.push_asset_key_locations),
    }
}
