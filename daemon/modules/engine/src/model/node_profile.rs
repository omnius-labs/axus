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

impl std::fmt::Display for NodeProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = UriConverter::encode("node", self).map_err(|_| std::fmt::Error)?;
        write!(f, "{s}")
    }
}

impl std::str::FromStr for NodeProfile {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        UriConverter::decode("node", s)
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
        let mut public_key: Option<Vec<u8>> = None;
        let mut addrs: Option<Vec<OmniAddr>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => public_key = Some(decoder.read_bytes_vec()?),
                1 => {
                    let count = decoder.read_array()?;
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
}
