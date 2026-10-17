use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};

use crate::model::{AssetKey, NodeProfile};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DataMessage {
    pub push_node_profiles: Vec<Arc<NodeProfile>>,
    pub want_asset_keys: Vec<Arc<AssetKey>>,
    pub give_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
    pub push_asset_key_locations: HashMap<Arc<AssetKey>, Vec<Arc<NodeProfile>>>,
}

impl DataMessage {
    pub fn new() -> Self {
        Self {
            push_node_profiles: vec![],
            want_asset_keys: vec![],
            give_asset_key_locations: HashMap::new(),
            push_asset_key_locations: HashMap::new(),
        }
    }

    /// 更新時刻が `now` より許容幅を超えて進んでいる NodeProfile を取り除き、取り除いた件数を返す。
    /// 署名は本人のものなので Session は閉じず、その NodeProfile だけを捨てる
    pub fn remove_future_node_profiles(&mut self, now: DateTime<Utc>) -> usize {
        let mut removed = 0;
        let mut retain = |profiles: &mut Vec<Arc<NodeProfile>>| {
            let before = profiles.len();
            profiles.retain(|profile| profile.is_updated_within_skew(now));
            removed += before - profiles.len();
        };
        retain(&mut self.push_node_profiles);
        for profiles in self.give_asset_key_locations.values_mut().chain(self.push_asset_key_locations.values_mut()) {
            retain(profiles);
        }
        self.give_asset_key_locations.retain(|_, profiles| !profiles.is_empty());
        self.push_asset_key_locations.retain(|_, profiles| !profiles.is_empty());
        removed
    }
}

impl Default for DataMessage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_rocketpack::{RocketPackBytesDecoder, RocketPackBytesEncoder};
    use testresult::TestResult;

    use super::*;
    use crate::prelude::*;
    use crate::protocol::{MessageCodec, node::DataMessageCodec};

    #[test]
    fn future_node_profiles_are_removed_from_every_category() {
        use omnius_core_omnikit::generated::{
            omni_hash::{OmniHash, OmniHashAlgorithmType},
            omni_sign::{OmniSignType, OmniSigner},
        };

        let now = DateTime::from_timestamp(1_000_000, 0).unwrap();
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test").unwrap();
        let skew = NodeProfile::MAX_FUTURE_UPDATE_SKEW.as_secs() as i64;
        let signed_at = |seconds: i64| Arc::new(NodeProfile::sign(&signer, vec![], DateTime::from_timestamp(seconds, 0).unwrap()).unwrap());
        let (current, edge, future) = (signed_at(1_000_000), signed_at(1_000_000 + skew), signed_at(1_000_000 + skew + 1));
        let key = |typ: &str| {
            Arc::new(AssetKey {
                typ: typ.to_owned(),
                hash: OmniHash {
                    typ: OmniHashAlgorithmType::None,
                    value: vec![],
                },
            })
        };
        let mut message = DataMessage {
            push_node_profiles: vec![current.clone(), future.clone(), edge.clone()],
            give_asset_key_locations: HashMap::from([(key("kept"), vec![future.clone(), current.clone()]), (key("emptied"), vec![future.clone()])]),
            push_asset_key_locations: HashMap::from([(key("emptied"), vec![future])]),
            ..DataMessage::new()
        };

        assert_eq!(message.remove_future_node_profiles(now), 4);
        assert_eq!(message.push_node_profiles, vec![current.clone(), edge]);
        assert_eq!(message.give_asset_key_locations, HashMap::from([(key("kept"), vec![current])]));
        assert!(message.push_asset_key_locations.is_empty());
    }

    #[test]
    fn oversized_counts_are_rejected_before_reading_elements() -> TestResult {
        for (field, max, context) in [
            (0, DataMessage::MAX_PUSH_NODE_PROFILES, "DataMessage.push_node_profiles"),
            (1, DataMessage::MAX_WANT_ASSET_KEYS, "DataMessage.want_asset_keys"),
            (2, DataMessage::MAX_GIVE_ASSET_KEY_LOCATIONS, "DataMessage.give_asset_key_locations"),
            (3, DataMessage::MAX_PUSH_ASSET_KEY_LOCATIONS, "DataMessage.push_asset_key_locations"),
        ] {
            let mut bytes = Vec::new();
            let mut encoder = RocketPackBytesEncoder::new(&mut bytes);
            encoder.write_map(1)?;
            encoder.write_u64(field)?;
            if field < 2 {
                encoder.write_array(max + 1)?;
            } else {
                encoder.write_map(max + 1)?;
            }
            let elements_start = bytes.len();
            // 要素は decode できない null とし、件数だけで拒否することを確かめる
            bytes.resize(elements_start + (max + 1) * 2, 0xf6);
            let mut decoder = RocketPackBytesDecoder::new(&bytes);
            let error = DataMessageCodec::unpack(&mut decoder).unwrap_err();
            assert!(matches!(error, RocketPackDecoderError::LengthOutOfRange { context: actual, .. } if actual == context));
            assert_eq!(decoder.position(), elements_start);
        }
        Ok(())
    }

    #[test]
    fn oversized_location_profile_counts_are_rejected_before_reading_profiles() -> TestResult {
        for (field, context) in [(2, "DataMessage.give_asset_key_locations.value"), (3, "DataMessage.push_asset_key_locations.value")] {
            let mut bytes = Vec::new();
            let mut encoder = RocketPackBytesEncoder::new(&mut bytes);
            encoder.write_map(1)?;
            encoder.write_u64(field)?;
            encoder.write_map(1)?;
            encoder.write_struct(&AssetKey {
                typ: "test".to_string(),
                hash: omnius_core_omnikit::generated::omni_hash::OmniHash::compute_hash(omnius_core_omnikit::generated::omni_hash::OmniHashAlgorithmType::Sha3_256, b"asset"),
            })?;
            encoder.write_array(DataMessage::MAX_LOCATION_NODE_PROFILES + 1)?;
            let profiles_start = bytes.len();
            bytes.resize(profiles_start + DataMessage::MAX_LOCATION_NODE_PROFILES + 1, 0xf6);
            let mut decoder = RocketPackBytesDecoder::new(&bytes);
            let error = DataMessageCodec::unpack(&mut decoder).unwrap_err();
            assert!(matches!(error, RocketPackDecoderError::LengthOutOfRange { context: actual, .. } if actual == context));
            assert_eq!(decoder.position(), profiles_start);
        }
        Ok(())
    }
}
