use std::sync::OnceLock;

use omnius_core_omnikit::{
    generated::omni_hash::{OmniHash, OmniHashAlgorithmType},
    model::omni_addr::OmniAddr,
};

use crate::{model::converter::UriConverter, prelude::*};

/// node の公開鍵と到達先の組。node ID は公開鍵から導出するため保持しない
#[derive(Debug, Clone)]
pub struct NodeProfile {
    public_key: Vec<u8>,
    pub addrs: Vec<OmniAddr>,
    id: OnceLock<Vec<u8>>,
}

impl NodeProfile {
    pub const MAX_WIRE_ADDRS: usize = 8;

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

    fn unpack_with_max_addrs(decoder: &mut impl RocketPackDecoder, max_addrs: usize) -> std::result::Result<Self, RocketPackDecoderError> {
        let mut public_key: Option<Vec<u8>> = None;
        let mut addrs: Option<Vec<OmniAddr>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => public_key = Some(decoder.read_bytes_vec()?),
                1 => {
                    let count = decoder.read_array_bounded("NodeProfile.addrs", 0, max_addrs as u64)?;
                    let mut items: Vec<OmniAddr> = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        items.push(OmniAddr::from(decoder.read_string()?));
                    }
                    addrs = Some(items);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self::new(
            public_key.ok_or(RocketPackDecoderError::Other("missing field: public_key"))?,
            addrs.ok_or(RocketPackDecoderError::Other("missing field: addrs"))?,
        ))
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

impl std::fmt::Display for NodeProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = UriConverter::encode("node", self).map_err(|_| std::fmt::Error)?;
        write!(f, "{s}")
    }
}

impl std::str::FromStr for NodeProfile {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        UriConverter::decode::<NodeProfileUri>("node", s).map(|value| value.0)
    }
}

impl RocketPackStruct for NodeProfile {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(2)?;

        encoder.write_u64(0)?;
        encoder.write_bytes(value.public_key.as_slice())?;

        encoder.write_u64(1)?;
        encoder.write_array(value.addrs.len())?;
        for addr in value.addrs.iter() {
            encoder.write_string(addr.as_str())?;
        }

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        Self::unpack_with_max_addrs(decoder, Self::MAX_WIRE_ADDRS)
    }
}

// URI は保存と設定にも使うため、wire の件数制約を適用しない
struct NodeProfileUri(NodeProfile);

impl RocketPackStruct for NodeProfileUri {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        NodeProfile::pack(encoder, &value.0)
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError> {
        NodeProfile::unpack_with_max_addrs(decoder, usize::MAX).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

    use super::NodeProfile;

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
            let decoded = NodeProfile::import(&profile.export()?);
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
            NodeProfile::unpack(&mut decoder),
            Err(RocketPackDecoderError::LengthOutOfRange { context: "NodeProfile.addrs", .. })
        ));
        assert_eq!(decoder.position(), addresses_start);
        Ok(())
    }

    #[test]
    fn uri_roundtrip_preserves_more_than_eight_addresses() -> testresult::TestResult {
        use omnius_core_omnikit::model::omni_addr::OmniAddr;
        let profile = NodeProfile::new(vec![1, 2, 3], (0..12).map(|i| OmniAddr::new(format!("addr-{i}"))).collect());
        assert_eq!(profile.to_string().parse::<NodeProfile>()?, profile);
        Ok(())
    }
}
