use std::{sync::OnceLock, time::Duration};

use chrono::{DateTime, Utc};
use omnius_core_base::error::OmniError as _;
use omnius_core_omnikit::{
    generated::{
        omni_hash::{OmniHash, OmniHashAlgorithmType},
        omni_sign::{OmniCert, OmniSignType, OmniSigner},
    },
    model::omni_addr::OmniAddr,
};

use crate::{
    error::{Error, ErrorKind},
    generated::axus::model as wire,
    prelude::*,
    result::Result,
};

/// 署名を作る側が用途を取り違えないよう、署名対象の先頭に付ける固定の接頭辞。署名対象の形を変えるときは version を上げる
const SIGNING_PREFIX: &[u8] = b"axus.node-profile.v1\0";

/// node の公開鍵と到達先の組。node 本人の署名を持ち、署名の検証を通った値だけが存在する。node ID は公開鍵から導出するため保持しない
#[derive(Debug, Clone)]
pub struct NodeProfile {
    public_key: Vec<u8>,
    addrs: Vec<OmniAddr>,
    updated_time: i64,
    signature: Vec<u8>,
    id: OnceLock<Vec<u8>>,
}

impl NodeProfile {
    /// 更新時刻が受信時刻より進んでいてよい幅。Session の handshake が許す時計の差と同じ値にする
    pub const MAX_FUTURE_UPDATE_SKEW: Duration = Duration::from_secs(5 * 60);

    /// `signer` の鍵で、到達先と更新時刻に署名した NodeProfile を作る
    pub fn sign(signer: &OmniSigner, addrs: Vec<OmniAddr>, updated_time: DateTime<Utc>) -> Result<Self> {
        // 署名後は到達先を変えられないため、wire の上限を超える値は署名の前に拒否する
        if addrs.len() > wire::MAX_WIRE_ADDRS as usize {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("node profile has too many addresses"));
        }
        Self::validate_addr_byte_lengths(&addrs)?;
        let updated_time = updated_time.timestamp();
        let public_key = signer.sign(&[])?.public_key;
        let message = Self::signing_message(&public_key, &addrs, updated_time)?;
        let signature = signer.sign(&message)?.value;

        Ok(Self {
            public_key,
            addrs,
            updated_time,
            signature,
            id: OnceLock::new(),
        })
    }

    /// 署名を検証し、本人が署名した値だけを NodeProfile にする
    pub(crate) fn verified(public_key: Vec<u8>, addrs: Vec<OmniAddr>, updated_time: i64, signature: Vec<u8>) -> Result<Self> {
        let message = Self::signing_message(&public_key, &addrs, updated_time)?;
        let cert = OmniCert {
            typ: OmniSignType::Ed25519_Sha3_256_Base64Url,
            name: String::new(),
            public_key,
            value: signature,
        };
        cert.verify(&message)
            .map_err(|e| Error::from_error(e, ErrorKind::InvalidFormat).with_message("node profile signature is invalid"))?;

        Ok(Self {
            public_key: cert.public_key,
            addrs,
            updated_time,
            signature: cert.value,
            id: OnceLock::new(),
        })
    }

    /// 署名対象のバイト列。署名を除いた値を符号化し、接頭辞を付ける
    fn signing_message(public_key: &[u8], addrs: &[OmniAddr], updated_time: i64) -> Result<Vec<u8>> {
        let payload = wire::NodeProfileSigningPayload {
            public_key: public_key.to_vec(),
            addrs: addrs.iter().map(|addr| addr.as_str().to_owned()).collect(),
            updated_time,
        };
        let mut message = SIGNING_PREFIX.to_vec();
        message.extend(payload.export()?);
        Ok(message)
    }

    /// Session の cert と同じ DER 表現の公開鍵
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    pub fn addrs(&self) -> &[OmniAddr] {
        &self.addrs
    }

    /// 署名した時刻（unix 秒）。同じ node ID の NodeProfile のうち、大きいほうが新しい
    pub fn updated_time(&self) -> i64 {
        self.updated_time
    }

    pub(crate) fn signature(&self) -> &[u8] {
        &self.signature
    }

    /// 更新時刻が `now` より `MAX_FUTURE_UPDATE_SKEW` を超えて進んでいないか
    pub fn is_updated_within_skew(&self, now: DateTime<Utc>) -> bool {
        let limit = now.timestamp().saturating_add(Self::MAX_FUTURE_UPDATE_SKEW.as_secs() as i64);
        self.updated_time <= limit
    }

    pub(crate) fn validate_addr_byte_lengths(addrs: &[OmniAddr]) -> crate::result::Result<()> {
        if addrs.iter().any(|addr| addr.as_str().len() > wire::MAX_ADDR_LENGTH as usize) {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("advertised node address exceeds byte length limit"));
        }
        Ok(())
    }

    /// 公開鍵の SHA3-256 hash。初回の呼び出しで計算し、以降は保持した値を返す
    pub fn id(&self) -> &[u8] {
        self.id.get_or_init(|| OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, &self.public_key).value)
    }
}

// id は public_key から決まる cache なので、比較と hash の対象から外す
impl PartialEq for NodeProfile {
    fn eq(&self, other: &Self) -> bool {
        self.public_key == other.public_key && self.addrs == other.addrs && self.updated_time == other.updated_time && self.signature == other.signature
    }
}

impl Eq for NodeProfile {}

impl std::hash::Hash for NodeProfile {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.public_key.hash(state);
        self.addrs.hash(state);
        self.updated_time.hash(state);
        self.signature.hash(state);
    }
}

#[cfg(test)]
impl NodeProfile {
    /// test 用。新しい鍵で署名した NodeProfile を作る
    pub(crate) fn random_for_test(addrs: Vec<OmniAddr>) -> Self {
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test").unwrap();
        Self::sign(&signer, addrs, Utc::now()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use omnius_core_omnikit::{
        generated::omni_hash::{OmniHash, OmniHashAlgorithmType},
        model::omni_addr::OmniAddr,
    };
    use testresult::TestResult;

    use super::NodeProfile;
    use crate::{
        prelude::*,
        protocol::{
            MessageCodec, NodeProfileCodec,
            tests::{PROFILE_UPDATED_TIME, legacy, legacy_profile, signer},
            uri::UriConverter,
        },
    };

    fn at(seconds: i64) -> DateTime<chrono::Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    #[test]
    fn id_is_the_hash_of_the_public_key() {
        let node_profile = NodeProfile::random_for_test(vec![]);
        let expected = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, node_profile.public_key()).value;

        assert_eq!(node_profile.id(), expected.as_slice());
        assert_eq!(node_profile.id(), expected.as_slice());
    }

    #[test]
    fn cached_id_does_not_affect_equality() {
        let cached = NodeProfile::random_for_test(vec![]);
        let fresh = cached.clone();
        let _ = cached.id();

        assert_eq!(cached, fresh);
    }

    #[test]
    fn signing_message_is_fixed() -> TestResult {
        let public_key = signer().sign(&[])?.public_key;
        let message = NodeProfile::signing_message(&public_key, &[OmniAddr::new("a"), OmniAddr::new("b")], PROFILE_UPDATED_TIME)?;

        // "axus.node-profile.v1\0" の後に、署名を除いた NodeProfile を符号化した bytes が続く
        let expected = format!("617875732e6e6f64652d70726f66696c652e763100a300582c{}018261616162021a6553f100", hex::encode(&public_key));
        assert_eq!(hex::encode(message), expected);
        Ok(())
    }

    #[test]
    fn signed_profile_is_verified_after_a_wire_roundtrip() -> TestResult {
        let profile = NodeProfile::sign(&signer(), vec![OmniAddr::new("a")], at(PROFILE_UPDATED_TIME))?;

        assert_eq!(NodeProfileCodec::decode(&NodeProfileCodec::encode(&profile)?)?, profile);
        assert_eq!(profile.to_uri()?.parse::<NodeProfile>()?, profile);
        assert_eq!(profile.updated_time(), PROFILE_UPDATED_TIME);
        Ok(())
    }

    #[test]
    fn verification_rejects_any_changed_field() -> TestResult {
        let profile = NodeProfile::sign(&signer(), vec![OmniAddr::new("a")], at(PROFILE_UPDATED_TIME))?;
        let other_key = NodeProfile::random_for_test(vec![]).public_key().to_vec();
        let mut flipped = profile.signature().to_vec();
        flipped[0] ^= 1;

        let verify = |public_key: Vec<u8>, addrs: Vec<OmniAddr>, updated_time: i64, signature: Vec<u8>| NodeProfile::verified(public_key, addrs, updated_time, signature);
        let (key, addrs, time, sig) = (
            profile.public_key().to_vec(),
            profile.addrs().to_vec(),
            profile.updated_time(),
            profile.signature().to_vec(),
        );
        assert!(verify(key.clone(), addrs.clone(), time, sig.clone()).is_ok());
        assert!(verify(key.clone(), vec![OmniAddr::new("b")], time, sig.clone()).is_err());
        assert!(verify(key.clone(), vec![], time, sig.clone()).is_err());
        assert!(verify(key.clone(), addrs.clone(), time + 1, sig.clone()).is_err());
        assert!(verify(other_key, addrs.clone(), time, sig.clone()).is_err());
        assert!(verify(key.clone(), addrs.clone(), time, flipped).is_err());
        assert!(verify(key.clone(), addrs.clone(), time, vec![]).is_err());
        assert!(verify(vec![1, 2, 3], addrs, time, sig).is_err());
        Ok(())
    }

    #[test]
    fn verification_rejects_a_signature_without_the_domain_prefix() -> TestResult {
        use crate::generated::axus::model as wire;
        let public_key = signer().sign(&[])?.public_key;
        let payload = wire::NodeProfileSigningPayload {
            public_key: public_key.clone(),
            addrs: vec!["a".to_owned()],
            updated_time: PROFILE_UPDATED_TIME,
        };
        let signature = signer().sign(&payload.export()?)?.value;

        assert!(NodeProfile::verified(public_key, vec![OmniAddr::new("a")], PROFILE_UPDATED_TIME, signature).is_err());
        Ok(())
    }

    #[test]
    fn sign_rejects_values_that_the_wire_cannot_carry() {
        let too_many = (0..=NodeProfile::MAX_WIRE_ADDRS).map(|i| OmniAddr::new(format!("addr-{i}"))).collect();
        assert!(NodeProfile::sign(&signer(), too_many, at(PROFILE_UPDATED_TIME)).is_err());
        assert!(NodeProfile::sign(&signer(), vec![OmniAddr::new("a".repeat(513))], at(PROFILE_UPDATED_TIME)).is_err());
        assert!(NodeProfile::sign(&signer(), vec![OmniAddr::new("a".repeat(512)); NodeProfile::MAX_WIRE_ADDRS], at(PROFILE_UPDATED_TIME)).is_ok());
    }

    #[test]
    fn update_time_may_be_ahead_of_now_by_the_allowed_skew_only() {
        let now = at(1_000_000);
        let skew = NodeProfile::MAX_FUTURE_UPDATE_SKEW.as_secs() as i64;
        let signed_at = |seconds: i64| NodeProfile::sign(&signer(), vec![], at(seconds)).unwrap();

        assert!(signed_at(1_000_000 - 86_400 * 365).is_updated_within_skew(now));
        assert!(signed_at(1_000_000 + skew).is_updated_within_skew(now));
        assert!(!signed_at(1_000_000 + skew + 1).is_updated_within_skew(now));
    }

    #[test]
    fn wire_rejects_an_invalid_signature() -> TestResult {
        let profile = NodeProfile::random_for_test(vec![]);
        let forged = legacy::NodeProfile::new(profile.public_key().to_vec(), vec![], profile.updated_time(), vec![0; 64]);

        assert!(matches!(
            NodeProfileCodec::decode(&forged.export()?),
            Err(RocketPackDecoderError::Other("invalid node profile signature"))
        ));
        Ok(())
    }

    #[test]
    fn wire_accepts_eight_addresses_and_rejects_nine() -> TestResult {
        for count in [NodeProfile::MAX_WIRE_ADDRS, NodeProfile::MAX_WIRE_ADDRS + 1] {
            let addrs: Vec<_> = (0..count).map(|i| OmniAddr::new(format!("addr-{i}"))).collect();
            if count == NodeProfile::MAX_WIRE_ADDRS {
                let profile = NodeProfile::sign(&signer(), addrs, at(PROFILE_UPDATED_TIME))?;
                assert_eq!(NodeProfileCodec::decode(&legacy_profile(&profile).export()?)?, profile);
            } else {
                let oversized = legacy::NodeProfile::new(signer().sign(&[])?.public_key, addrs, PROFILE_UPDATED_TIME, vec![0; 64]);
                assert!(matches!(
                    NodeProfileCodec::decode(&oversized.export()?),
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
    fn wire_rejects_the_address_count_before_reading_addresses() -> TestResult {
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
    fn uri_accepts_eight_addresses_and_rejects_nine() -> TestResult {
        for count in [NodeProfile::MAX_WIRE_ADDRS, NodeProfile::MAX_WIRE_ADDRS + 1] {
            let addrs: Vec<_> = (0..count).map(|i| OmniAddr::new(format!("addr-{i}"))).collect();
            if count == NodeProfile::MAX_WIRE_ADDRS {
                let profile = NodeProfile::sign(&signer(), addrs, at(PROFILE_UPDATED_TIME))?;
                assert_eq!(profile.to_uri()?.parse::<NodeProfile>()?, profile);
            } else {
                let oversized = legacy::NodeProfile::new(signer().sign(&[])?.public_key, addrs, PROFILE_UPDATED_TIME, vec![0; 64]);
                assert!(UriConverter::encode("node", &oversized)?.parse::<NodeProfile>().is_err());
            }
        }
        Ok(())
    }

    #[test]
    fn uri_with_a_tampered_signature_is_rejected() -> TestResult {
        let profile = NodeProfile::sign(&signer(), vec![OmniAddr::new("a")], at(PROFILE_UPDATED_TIME))?;
        let mut signature = profile.signature().to_vec();
        signature[0] ^= 1;
        let forged = legacy::NodeProfile::new(profile.public_key().to_vec(), profile.addrs().to_vec(), profile.updated_time(), signature);

        assert!(UriConverter::encode("node", &forged)?.parse::<NodeProfile>().is_err());
        Ok(())
    }
}
