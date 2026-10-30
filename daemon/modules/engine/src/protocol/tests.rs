pub(crate) mod legacy;

use std::fmt::Debug;

use chrono::DateTime;
use omnius_core_omnikit::{
    generated::{
        omni_hash::{OmniHash, OmniHashAlgorithmType},
        omni_sign::{OmniSignType, OmniSigner},
    },
    model::omni_addr::OmniAddr,
};
use omnius_core_rocketpack::{RocketPackBytesDecoder, RocketPackBytesEncoder};
use testresult::TestResult;

use super::{MessageCodec, NodeProfileCodec, node::*, session::*};
use crate::{
    core::{negotiator::node as domain_node, session::message as domain_session},
    generated::axus,
    model::{AssetKey, NodeProfile},
    prelude::*,
};

// Literal bytes were fixed against the pre-migration codecs, not generated output.
const KEY_HEX: &str = "a200617401a201a101a00240";
// NodeProfile signed by signer() at update time 1700000000. OpenSSL's Ed25519 produces the same signature for the signing message.
const PROFILE_HEX: &str = "a400582c302a300506032b6570032100ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c018261616162021a6553f1000358405349e0b3f1f6acbb37e3804cce16f1aee9c6f35f2655273a5d0078005da1d421593ec4e952baaa974b65fc7e34cf56cd2d629cf41007f5d8ed1aeb9b75afbd02";

#[cfg(test)]
fn hash() -> OmniHash {
    OmniHash {
        typ: OmniHashAlgorithmType::None,
        value: vec![],
    }
}
#[cfg(test)]
pub(crate) fn signer() -> OmniSigner {
    // PKCS#8 Ed25519 private key with a fixed seed, so the signature bytes are fixed.
    let mut key = hex::decode("302e020100300506032b657004220420").unwrap();
    key.extend([7u8; 32]);
    OmniSigner {
        typ: OmniSignType::Ed25519_Sha3_256_Base64Url,
        name: "test".to_owned(),
        key,
    }
}
#[cfg(test)]
pub(crate) const PROFILE_UPDATED_TIME: i64 = 1_700_000_000;
#[cfg(test)]
fn profile() -> NodeProfile {
    NodeProfile::sign(
        &signer(),
        vec![OmniAddr::new("a"), OmniAddr::new("b")],
        DateTime::from_timestamp(PROFILE_UPDATED_TIME, 0).unwrap(),
    )
    .unwrap()
}
#[cfg(test)]
fn key() -> AssetKey {
    AssetKey {
        typ: "t".to_owned(),
        hash: hash(),
    }
}

#[cfg(test)]
pub(crate) fn legacy_profile(value: &NodeProfile) -> legacy::NodeProfile {
    legacy::NodeProfile::new(value.public_key().to_vec(), value.addrs().to_vec(), value.updated_time(), value.signature().to_vec())
}

#[cfg(test)]
fn fixed_generated<L, W>(old: &L, new: &W, expected: &str) -> TestResult
where
    L: RocketPackStruct + PartialEq + Debug,
    W: RocketPackStruct + PartialEq + Debug,
{
    let bytes = hex::decode(expected)?;
    assert_eq!(old.export()?, bytes);
    assert_eq!(new.export()?, bytes);
    assert_eq!(&L::import(&new.export()?)?, old);
    assert_eq!(&W::import(&old.export()?)?, new);
    Ok(())
}

#[cfg(test)]
fn compatible<C, L>(old: &L, new: &C::Message, exact: bool) -> TestResult
where
    C: MessageCodec,
    C::Message: PartialEq + Debug,
    L: RocketPackStruct + PartialEq + Debug,
{
    let old_bytes = old.export()?;
    let new_bytes = C::encode(new)?;
    if exact {
        assert_eq!(old_bytes, new_bytes);
    }
    assert_eq!(&C::decode(&old_bytes)?, new);
    assert_eq!(&L::import(&new_bytes)?, old);
    Ok(())
}

#[cfg(test)]
fn acceptance_matrix<L: RocketPackStruct>(old: &L, accepts_new: impl Fn(&[u8]) -> bool) -> TestResult {
    let bytes = old.export()?;
    let mut decoder = RocketPackBytesDecoder::new(&bytes);
    let count = decoder.read_map()?;
    let mut fields = Vec::new();
    for _ in 0..count {
        let start = decoder.position();
        decoder.read_u64()?;
        let value_start = decoder.position();
        decoder.skip_field()?;
        fields.push((bytes[start..value_start].to_vec(), bytes[value_start..decoder.position()].to_vec()));
    }
    let mut variants = vec![vec![]];
    for index in 0..fields.len() {
        let mut missing = fields.clone();
        missing.remove(index);
        variants.push(missing);
        let mut null = fields.clone();
        null[index].1 = vec![0xf6];
        variants.push(null);
    }
    let mut reversed = fields.clone();
    reversed.reverse();
    variants.push(reversed);
    let mut duplicate = fields.clone();
    duplicate.extend(fields.clone());
    variants.push(duplicate);
    let mut unknown = fields;
    unknown.insert(0, (vec![0x18, 99], vec![0x81, 0xf6]));
    variants.push(unknown);
    for fields in variants {
        let mut bytes = Vec::new();
        RocketPackBytesEncoder::new(&mut bytes).write_map(fields.len())?;
        for (key, value) in fields {
            bytes.extend(key);
            bytes.extend(value);
        }
        for trailing in [false, true] {
            if trailing {
                bytes.push(0xff);
            }
            assert_eq!(L::import(&bytes).is_ok(), accepts_new(&bytes), "acceptance differs for {}", hex::encode(&bytes));
        }
    }
    Ok(())
}

#[test]
fn all_codecs_preserve_required_unknown_null_duplicate_and_trailing_field_acceptance() -> TestResult {
    acceptance_matrix(&legacy::AssetKey { typ: "t".into(), hash: hash() }, |b| AssetKey::import(b).is_ok())?;
    acceptance_matrix(&legacy::FileRef { name: "f".into(), hash: hash() }, |b| axus::model::FileRef::import(b).is_ok())?;
    acceptance_matrix(&legacy::MerkleLayer { rank: 0, hashes: vec![hash()] }, |b| axus::file::MerkleLayer::import(b).is_ok())?;
    acceptance_matrix(
        &legacy::session::HelloMessage {
            version: legacy::session::SessionVersion::V1,
        },
        |b| super::session::HelloMessageCodec::decode(b).is_ok(),
    )?;
    acceptance_matrix(
        &legacy::session::V1RequestMessage {
            request_type: legacy::session::V1RequestType::Unknown,
        },
        |b| V1RequestMessageCodec::decode(b).is_ok(),
    )?;
    acceptance_matrix(
        &legacy::session::V1ResultMessage {
            result_type: legacy::session::V1ResultType::Unknown,
        },
        |b| V1ResultMessageCodec::decode(b).is_ok(),
    )?;
    acceptance_matrix(&axus::node::NodeMessage::Status, |b| NodeMessageCodec::decode(b).is_ok())
}

#[test]
fn asset_key_fixed_bytes() -> TestResult {
    fixed_generated(&legacy::AssetKey { typ: "t".into(), hash: hash() }, &key(), KEY_HEX)
}

#[test]
fn file_ref_fixed_bytes() -> TestResult {
    fixed_generated(
        &legacy::FileRef { name: "f".into(), hash: hash() },
        &axus::model::FileRef { name: "f".into(), hash: hash() },
        "a200616601a201a101a00240",
    )
}

#[test]
fn merkle_layer_fixed_bytes_and_content_hash() -> TestResult {
    let old = legacy::MerkleLayer { rank: 3, hashes: vec![hash()] };
    let new = axus::file::MerkleLayer { rank: 3, hashes: vec![hash()] };
    fixed_generated(&old, &new, "a200030181a201a101a00240")?;
    assert_eq!(
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, old.export()?),
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, new.export()?)
    );
    Ok(())
}

#[test]
fn node_profile_fixed_bytes_and_cache_preservation() -> TestResult {
    let value = profile();
    let id = value.id().to_vec();
    assert_eq!(NodeProfileCodec::encode(&value)?, hex::decode(PROFILE_HEX)?);
    compatible::<NodeProfileCodec, _>(&legacy_profile(&value), &value, true)?;
    assert_eq!(NodeProfileCodec::decode(&NodeProfileCodec::encode(&value)?)?.id(), id);
    Ok(())
}

#[test]
fn session_hello_fixed_bytes() -> TestResult {
    let old = legacy::session::HelloMessage {
        version: legacy::session::SessionVersion::V1,
    };
    let new = domain_session::HelloMessage {
        version: domain_session::SessionVersion::V1,
    };
    compatible::<super::session::HelloMessageCodec, _>(&old, &new, true)?;
    assert_eq!(super::session::HelloMessageCodec::encode(&new)?, hex::decode("a10001")?);
    Ok(())
}

#[test]
fn all_request_values_have_fixed_bytes() -> TestResult {
    for (old, new) in [
        (legacy::session::V1RequestType::Unknown, domain_session::V1RequestType::Unknown),
        (legacy::session::V1RequestType::NodeFinder, domain_session::V1RequestType::NodeFinder),
        (legacy::session::V1RequestType::FileExchanger, domain_session::V1RequestType::FileExchanger),
    ] {
        let value = domain_session::V1RequestMessage { request_type: new };
        compatible::<V1RequestMessageCodec, _>(&legacy::session::V1RequestMessage { request_type: old }, &value, true)?;
        assert_eq!(V1RequestMessageCodec::encode(&value)?, vec![0xa1, 0, new as u8]);
    }
    Ok(())
}

#[test]
fn all_result_values_have_fixed_bytes() -> TestResult {
    for (old, new) in [
        (legacy::session::V1ResultType::Unknown, domain_session::V1ResultType::Unknown),
        (legacy::session::V1ResultType::Accept, domain_session::V1ResultType::Accept),
        (legacy::session::V1ResultType::Reject, domain_session::V1ResultType::Reject),
    ] {
        let value = domain_session::V1ResultMessage { result_type: new };
        compatible::<V1ResultMessageCodec, _>(&legacy::session::V1ResultMessage { result_type: old }, &value, true)?;
        assert_eq!(V1ResultMessageCodec::encode(&value)?, vec![0xa1, 0, new as u8]);
    }
    Ok(())
}

#[test]
fn asset_key_order_is_consistent_with_equality() {
    let values: Vec<_> = [OmniHashAlgorithmType::None, OmniHashAlgorithmType::Sha3_256, OmniHashAlgorithmType::Blake3_256]
        .into_iter()
        .flat_map(|typ| {
            ["", "a", "b"].into_iter().flat_map(move |text| {
                let typ = typ.clone();
                [vec![], vec![0], vec![1]].into_iter().map(move |value| AssetKey {
                    typ: text.into(),
                    hash: OmniHash { typ: typ.clone(), value },
                })
            })
        })
        .collect();
    for a in &values {
        for b in &values {
            assert_eq!(a.cmp(b) == std::cmp::Ordering::Equal, a == b);
            assert_eq!(a.partial_cmp(b), Some(a.cmp(b)));
            assert_eq!(a.cmp(b), b.cmp(a).reverse());
        }
    }
}

#[test]
fn scalar_duplicates_reject_every_invalid_occurrence() -> TestResult {
    for values in [[9, 1], [1, 9], [9, 9], [1, 1]] {
        let bytes = vec![0xa2, 0, values[0], 0, values[1]];
        for (old, new) in [
            (
                legacy::session::HelloMessage::import(&bytes).is_ok(),
                super::session::HelloMessageCodec::decode(&bytes).is_ok(),
            ),
            (legacy::session::V1RequestMessage::import(&bytes).is_ok(), V1RequestMessageCodec::decode(&bytes).is_ok()),
            (legacy::session::V1ResultMessage::import(&bytes).is_ok(), V1ResultMessageCodec::decode(&bytes).is_ok()),
        ] {
            assert_eq!(old, new);
        }
    }
    Ok(())
}

#[test]
fn unknown_missing_null_and_trailing_fields_keep_acceptance() -> TestResult {
    for bytes in [
        vec![0xa0],
        vec![0xa1, 0, 0xf6],
        vec![0xa2, 0, 1, 9, 0x81, 0xf6],
        vec![0xa1, 0, 1, 0xff],
        vec![0xa2, 0, 1, 0, 1],
    ] {
        assert_eq!(
            legacy::session::HelloMessage::import(&bytes).is_ok(),
            super::session::HelloMessageCodec::decode(&bytes).is_ok()
        );
    }
    Ok(())
}

#[test]
fn bounded_fields_and_hash_variants_remain_compatible() -> TestResult {
    for typ in [OmniHashAlgorithmType::None, OmniHashAlgorithmType::Sha3_256, OmniHashAlgorithmType::Blake3_256] {
        let hash = OmniHash { typ, value: vec![3; 64] };
        for text in [String::new(), format!("{}a", "日".repeat(21))] {
            let old = legacy::AssetKey {
                typ: text.clone(),
                hash: hash.clone(),
            };
            let new = AssetKey { typ: text, hash: hash.clone() };
            assert_eq!(old.export()?, new.export()?);
            assert_eq!(legacy::AssetKey::import(&new.export()?)?, old);
            let old = legacy::MerkleLayer {
                rank: u32::MAX,
                hashes: vec![hash.clone(); 16],
            };
            let new = axus::file::MerkleLayer {
                rank: u32::MAX,
                hashes: vec![hash.clone(); 16],
            };
            assert_eq!(old.export()?, new.export()?);
        }
    }
    let new = NodeProfile::sign(
        &signer(),
        vec![OmniAddr::new(format!("{}aa", "日".repeat(170)))],
        DateTime::from_timestamp(PROFILE_UPDATED_TIME, 0).unwrap(),
    )?;
    compatible::<NodeProfileCodec, _>(&legacy_profile(&new), &new, true)
}

#[test]
fn byte_length_limits_reject_old_oversized_fields() -> TestResult {
    for (addr, accepted) in [
        ("a".repeat(512), true),
        ("a".repeat(513), false),
        (format!("{}aa", "日".repeat(170)), true),
        ("日".repeat(171), false),
    ] {
        let at = DateTime::from_timestamp(PROFILE_UPDATED_TIME, 0).unwrap();
        let signed = NodeProfile::sign(&signer(), vec![OmniAddr::new(addr.clone())], at);
        assert_eq!(signed.is_ok(), accepted);
        // 署名を検証する前に、生成型の decode が byte 長を拒否することを確かめる
        let old = legacy::NodeProfile::new(signer().sign(&[])?.public_key, vec![OmniAddr::new(addr)], PROFILE_UPDATED_TIME, vec![0; 64]).export()?;
        assert_eq!(axus::model::NodeProfile::import(&old).is_ok(), accepted);
        if let Ok(profile) = signed {
            assert_eq!(NodeProfileCodec::encode(&profile).is_ok(), accepted);
            assert_eq!(NodeProfileCodec::decode(&legacy_profile(&profile).export()?)?, profile);
        }
    }
    for text in ["a".repeat(64), "a".repeat(65), format!("{}a", "日".repeat(21)), "日".repeat(22)] {
        let accepted = text.len() <= 64;
        let old = legacy::AssetKey { typ: text.clone(), hash: hash() };
        let key = AssetKey { typ: text, hash: hash() };
        assert_eq!(key.export().is_ok(), accepted);
        assert_eq!(AssetKey::import(&old.export()?).is_ok(), accepted);
    }
    Ok(())
}

#[test]
fn message_byte_boundary_is_checked_before_decoding() -> TestResult {
    let limit = axus::node::MAX_MESSAGE_LENGTH as usize;
    for size in [limit, limit + 1] {
        // 上限ちょうどまで未知 field で埋めた、構造上は有効な message。
        let mut bytes = NodeMessageCodec::encode(&domain_node::NodeMessage::Status)?;
        bytes[0] = 0xa2;
        let padding = size - bytes.len() - 6;
        let mut encoder = RocketPackBytesEncoder::new(&mut bytes);
        encoder.write_u64(9)?;
        encoder.write_bytes(&vec![0; padding])?;
        assert_eq!(bytes.len(), size);
        if size == limit {
            assert_eq!(NodeMessageCodec::decode(&bytes)?, domain_node::NodeMessage::Status);
        } else {
            assert!(matches!(
                NodeMessageCodec::decode(&bytes),
                Err(RocketPackDecoderError::LengthOutOfRange { context: "NodeMessage", .. })
            ));
        }
    }
    // 不正な構造でも、巨大入力は field を読む前にサイズ違反となる。
    let bytes = vec![0xff; limit + 1];
    assert!(matches!(
        NodeMessageCodec::decode(&bytes),
        Err(RocketPackDecoderError::LengthOutOfRange { context: "NodeMessage", .. })
    ));
    Ok(())
}
