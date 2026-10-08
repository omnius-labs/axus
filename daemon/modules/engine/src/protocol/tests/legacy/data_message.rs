use std::{collections::HashMap, sync::Arc};

use crate::{
    prelude::*,
    protocol::tests::legacy::{AssetKey, NodeProfile},
};

#[derive(Debug, PartialEq, Eq)]
pub struct DataMessage {
    pub push_node_profiles: Vec<Arc<NodeProfile>>,
    pub want_asset_keys: Vec<Arc<AssetKey>>,
    pub give_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
    pub push_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
}

impl DataMessage {
    pub const MAX_PUSH_NODE_PROFILES: usize = 32;
    pub const MAX_WANT_ASSET_KEYS: usize = 1024;
    pub const MAX_GIVE_ASSET_KEY_LOCATIONS: usize = 1024;
    pub const MAX_PUSH_ASSET_KEY_LOCATIONS: usize = 1024;
    pub const MAX_LOCATION_NODE_PROFILES: usize = 8;

    pub fn new() -> Self {
        Self {
            push_node_profiles: vec![],
            want_asset_keys: vec![],
            give_asset_key_locations: HashMap::new(),
            push_asset_key_locations: HashMap::new(),
        }
    }
}

impl Default for DataMessage {
    fn default() -> Self {
        Self::new()
    }
}

impl RocketPackStruct for DataMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(4)?;

        encoder.write_u64(0)?;
        encoder.write_array(value.push_node_profiles.len())?;
        for profile in value.push_node_profiles.iter() {
            encoder.write_struct(profile.as_ref())?;
        }

        encoder.write_u64(1)?;
        encoder.write_array(value.want_asset_keys.len())?;
        for asset_key in value.want_asset_keys.iter() {
            encoder.write_struct(asset_key.as_ref())?;
        }

        encoder.write_u64(2)?;
        encoder.write_map(value.give_asset_key_locations.len())?;
        for (asset_key, profiles) in value.give_asset_key_locations.iter() {
            encoder.write_struct(asset_key.as_ref())?;
            encoder.write_array(profiles.len())?;
            for profile in profiles.iter() {
                encoder.write_struct(profile.as_ref())?;
            }
        }

        encoder.write_u64(3)?;
        encoder.write_map(value.push_asset_key_locations.len())?;
        for (asset_key, profiles) in value.push_asset_key_locations.iter() {
            encoder.write_struct(asset_key.as_ref())?;
            encoder.write_array(profiles.len())?;
            for profile in profiles.iter() {
                encoder.write_struct(profile.as_ref())?;
            }
        }

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut push_node_profiles: Option<Vec<Arc<NodeProfile>>> = None;
        let mut want_asset_keys: Option<Vec<Arc<AssetKey>>> = None;
        let mut give_asset_key_locations: Option<HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>> = None;
        let mut push_asset_key_locations: Option<HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => {
                    let count = decoder.read_array_bounded("push_node_profiles", 0, Self::MAX_PUSH_NODE_PROFILES as u64)?;
                    let mut profiles = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        profiles.push(Arc::new(decoder.read_struct::<NodeProfile>()?));
                    }
                    push_node_profiles = Some(profiles);
                }
                1 => {
                    let count = decoder.read_array_bounded("want_asset_keys", 0, Self::MAX_WANT_ASSET_KEYS as u64)?;
                    let mut asset_keys = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        asset_keys.push(Arc::new(decoder.read_struct::<AssetKey>()?));
                    }
                    want_asset_keys = Some(asset_keys);
                }
                2 => {
                    let count = decoder.read_map_bounded("give_asset_key_locations", 0, Self::MAX_GIVE_ASSET_KEY_LOCATIONS as u64)?;
                    let mut map = HashMap::with_capacity(count as usize);
                    for _ in 0..count {
                        let key = Arc::new(decoder.read_struct::<AssetKey>()?);
                        let count = decoder.read_array_bounded("give_asset_key_locations.node_profiles", 0, Self::MAX_LOCATION_NODE_PROFILES as u64)?;
                        let mut profiles = Vec::with_capacity(count as usize);
                        for _ in 0..count {
                            profiles.push(Arc::new(decoder.read_struct::<NodeProfile>()?));
                        }
                        map.insert(key, profiles);
                    }
                    give_asset_key_locations = Some(map);
                }
                3 => {
                    let count = decoder.read_map_bounded("push_asset_key_locations", 0, Self::MAX_PUSH_ASSET_KEY_LOCATIONS as u64)?;
                    let mut map = HashMap::with_capacity(count as usize);
                    for _ in 0..count {
                        let key = Arc::new(decoder.read_struct::<AssetKey>()?);
                        let count = decoder.read_array_bounded("push_asset_key_locations.node_profiles", 0, Self::MAX_LOCATION_NODE_PROFILES as u64)?;
                        let mut profiles = Vec::with_capacity(count as usize);
                        for _ in 0..count {
                            profiles.push(Arc::new(decoder.read_struct::<NodeProfile>()?));
                        }
                        map.insert(key, profiles);
                    }
                    push_asset_key_locations = Some(map);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            push_node_profiles: push_node_profiles.ok_or(RocketPackDecoderError::Other("missing field: push_node_profiles"))?,
            want_asset_keys: want_asset_keys.ok_or(RocketPackDecoderError::Other("missing field: want_asset_keys"))?,
            give_asset_key_locations: give_asset_key_locations.ok_or(RocketPackDecoderError::Other("missing field: give_asset_key_locations"))?,
            push_asset_key_locations: push_asset_key_locations.ok_or(RocketPackDecoderError::Other("missing field: push_asset_key_locations"))?,
        })
    }
}
