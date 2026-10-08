use std::sync::OnceLock;

use omnius_core_omnikit::{
    generated::omni_hash::{OmniHash, OmniHashAlgorithmType},
    model::omni_addr::OmniAddr,
};

/// node の公開鍵と到達先の組。node ID は公開鍵から導出するため保持しない
#[derive(Debug, Clone)]
pub struct NodeProfile {
    public_key: Vec<u8>,
    pub addrs: Vec<OmniAddr>,
    id: OnceLock<Vec<u8>>,
}

impl NodeProfile {
    pub fn new(public_key: Vec<u8>, addrs: Vec<OmniAddr>) -> Self {
        Self {
            public_key,
            addrs,
            id: OnceLock::new(),
        }
    }

    /// Session の cert と同じ DER 表現の公開鍵
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// 公開鍵の SHA3-256 hash。初回の呼び出しで計算し、以降は保持した値を返す
    pub fn id(&self) -> &[u8] {
        self.id.get_or_init(|| OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, &self.public_key).value)
    }
}

// id は public_key から決まる cache なので、比較と hash の対象から外す
impl PartialEq for NodeProfile {
    fn eq(&self, other: &Self) -> bool {
        self.public_key == other.public_key && self.addrs == other.addrs
    }
}

impl Eq for NodeProfile {}

impl std::hash::Hash for NodeProfile {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.public_key.hash(state);
        self.addrs.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

    use super::NodeProfile;
    use crate::protocol::{MessageCodec, NodeProfileCodec};

    #[test]
    fn id_is_the_hash_of_the_public_key() {
        let node_profile = NodeProfile::new(vec![1, 2, 3], vec![]);
        let expected = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [1u8, 2, 3]).value;

        assert_eq!(node_profile.id(), expected.as_slice());
        assert_eq!(node_profile.id(), expected.as_slice());
    }

    #[test]
    fn cached_id_does_not_affect_equality() {
        let cached = NodeProfile::new(vec![1, 2, 3], vec![]);
        let _ = cached.id();
        let fresh = NodeProfile::new(vec![1, 2, 3], vec![]);

        assert_eq!(cached, fresh);
    }

    #[test]
    fn wire_accepts_eight_addresses_and_rejects_nine() -> testresult::TestResult {
        use crate::prelude::*;
        use omnius_core_omnikit::model::omni_addr::OmniAddr;
        for count in [NodeProfile::MAX_WIRE_ADDRS, NodeProfile::MAX_WIRE_ADDRS + 1] {
            let profile = NodeProfile::new(vec![1, 2, 3], (0..count).map(|i| OmniAddr::new(format!("addr-{i}"))).collect());
            let legacy = crate::protocol::tests::legacy::NodeProfile::new(profile.public_key().to_vec(), profile.addrs.clone());
            let decoded = NodeProfileCodec::decode(&legacy.export()?);
            if count == NodeProfile::MAX_WIRE_ADDRS {
                assert_eq!(decoded?, profile);
            } else {
                assert!(matches!(
                    decoded,
                    Err(RocketPackDecoderError::LengthOutOfRange {
                        context: "NodeProfile.addrs",
                        actual: 9,
                        ..
                    })
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn wire_rejects_the_address_count_before_reading_addresses() -> testresult::TestResult {
        use crate::prelude::*;
        use omnius_core_rocketpack::{RocketPackBytesDecoder, RocketPackBytesEncoder};
        let mut bytes = Vec::new();
        let mut encoder = RocketPackBytesEncoder::new(&mut bytes);
        encoder.write_map(1)?;
        encoder.write_u64(1)?;
        encoder.write_array(NodeProfile::MAX_WIRE_ADDRS + 1)?;
        let addresses_start = bytes.len();
        bytes.resize(addresses_start + NodeProfile::MAX_WIRE_ADDRS + 1, 0xf6);
        let mut decoder = RocketPackBytesDecoder::new(&bytes);
        assert!(matches!(
            NodeProfileCodec::unpack(&mut decoder),
            Err(RocketPackDecoderError::LengthOutOfRange { context: "NodeProfile.addrs", .. })
        ));
        assert_eq!(decoder.position(), addresses_start);
        Ok(())
    }

    #[test]
    fn uri_accepts_eight_addresses_and_rejects_legacy_nine() -> testresult::TestResult {
        use crate::{
            prelude::*,
            protocol::{tests::legacy, uri::UriConverter},
        };
        use omnius_core_omnikit::model::omni_addr::OmniAddr;
        for count in [8, 9, 12] {
            let addrs = (0..count).map(|i| OmniAddr::new(format!("addr-{i}"))).collect();
            let old = legacy::NodeProfile::new(vec![1, 2, 3], addrs);
            let uri = UriConverter::encode("node", &old)?;
            let decoded = uri.parse::<NodeProfile>();
            if count == 8 {
                let profile = decoded?;
                assert_eq!(profile.to_uri()?, uri);
            } else {
                assert!(decoded.is_err());
                let profile = NodeProfile::new(old.public_key.clone(), old.addrs.clone());
                assert!(profile.to_uri().is_err());
                assert!(matches!(NodeProfileCodec::encode(&profile), Err(RocketPackEncoderError::LengthOutOfRange { actual, max: 8, .. }) if actual == count));
            }
        }
        Ok(())
    }
}
