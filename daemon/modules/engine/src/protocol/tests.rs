pub(crate) mod legacy;

use std::{collections::HashMap, fmt::Debug, sync::Arc};

use enumflags2::BitFlags;
use omnius_core_omnikit::{
    generated::{
        omni_hash::{OmniHash, OmniHashAlgorithmType},
        omni_sign::{OmniCert, OmniSignType},
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
const PROFILE_HEX: &str = "a200420102018261616162";

#[cfg(test)]
fn hash() -> OmniHash {
    OmniHash {
        typ: OmniHashAlgorithmType::None,
        value: vec![],
    }
}
#[cfg(test)]
fn profile() -> NodeProfile {
    NodeProfile::new(vec![1, 2], vec![OmniAddr::new("a"), OmniAddr::new("b")])
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
    legacy::NodeProfile::new(value.public_key().to_vec(), value.addrs.clone())
}

#[cfg(test)]
pub(crate) fn legacy_data(value: &domain_node::DataMessage) -> legacy::DataMessage {
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
    acceptance_matrix(&legacy_profile(&profile()), |b| NodeProfileCodec::decode(b).is_ok())?;
    acceptance_matrix(
        &legacy::session::HelloMessage {
            version: legacy::session::SessionVersion::V1,
        },
        |b| super::session::HelloMessageCodec::decode(b).is_ok(),
    )?;
    acceptance_matrix(&legacy::session::V1ChallengeMessage { nonce: [0; 32] }, |b| V1ChallengeMessageCodec::decode(b).is_ok())?;
    let cert = OmniCert {
        typ: OmniSignType::None,
        name: "n".into(),
        public_key: vec![1],
        value: vec![2],
    };
    acceptance_matrix(&legacy::session::V1SignatureMessage { cert }, |b| V1SignatureMessageCodec::decode(b).is_ok())?;
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
    acceptance_matrix(
        &legacy::node::HelloMessage {
            version: BitFlags::from(legacy::node::NodeFinderVersion::V1),
        },
        |b| super::node::HelloMessageCodec::decode(b).is_ok(),
    )?;
    acceptance_matrix(
        &legacy::node::ProfileMessage {
            node_profile: legacy_profile(&profile()),
        },
        |b| ProfileMessageCodec::decode(b).is_ok(),
    )?;
    acceptance_matrix(&legacy_data(&domain_node::DataMessage::default()), |b| DataMessageCodec::decode(b).is_ok())
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
fn challenge_uses_bytes_and_preserves_nonce() -> TestResult {
    let nonce = [7; 32];
    let new = domain_session::V1ChallengeMessage { nonce };
    compatible::<V1ChallengeMessageCodec, _>(&legacy::session::V1ChallengeMessage { nonce }, &new, true)?;
    let mut expected = hex::decode("a1005820")?;
    expected.extend(nonce);
    assert_eq!(V1ChallengeMessageCodec::encode(&new)?, expected);
    Ok(())
}

#[test]
fn signature_fixed_bytes_and_dependency() -> TestResult {
    let cert = OmniCert {
        typ: OmniSignType::None,
        name: "n".into(),
        public_key: vec![1],
        value: vec![2],
    };
    let new = domain_session::V1SignatureMessage { cert: cert.clone() };
    compatible::<V1SignatureMessageCodec, _>(&legacy::session::V1SignatureMessage { cert }, &new, true)?;
    assert_eq!(V1SignatureMessageCodec::encode(&new)?, hex::decode("a100a401a101a002616e034101044102")?);
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
fn node_hello_fixed_bytes_and_unknown_bits() -> TestResult {
    let old = legacy::node::HelloMessage {
        version: BitFlags::from(legacy::node::NodeFinderVersion::V1),
    };
    let new = domain_node::HelloMessage {
        version: BitFlags::from(domain_node::NodeFinderVersion::V1),
    };
    compatible::<super::node::HelloMessageCodec, _>(&old, &new, true)?;
    assert_eq!(super::node::HelloMessageCodec::encode(&new)?, hex::decode("a10001")?);
    for raw in [0, 1, 2, 3, u32::MAX] {
        let bytes = axus::node::HelloMessage { version: raw }.export()?;
        assert_eq!(
            legacy::node::HelloMessage::import(&bytes)?.version.bits(),
            super::node::HelloMessageCodec::decode(&bytes)?.version.bits()
        );
    }
    Ok(())
}

#[test]
fn profile_message_fixed_bytes() -> TestResult {
    let value = profile();
    let new = domain_node::ProfileMessage { node_profile: value.clone() };
    compatible::<ProfileMessageCodec, _>(
        &legacy::node::ProfileMessage {
            node_profile: legacy_profile(&value),
        },
        &new,
        true,
    )?;
    assert_eq!(ProfileMessageCodec::encode(&new)?, hex::decode(format!("a100{PROFILE_HEX}"))?);
    Ok(())
}

#[test]
fn data_message_empty_and_single_maps_have_fixed_bytes() -> TestResult {
    let empty = domain_node::DataMessage::default();
    compatible::<DataMessageCodec, _>(&legacy_data(&empty), &empty, true)?;
    assert_eq!(DataMessageCodec::encode(&empty)?, hex::decode("a40080018002a003a0")?);
    let key = Arc::new(key());
    let profile = Arc::new(profile());
    let locations = HashMap::from([(key.clone(), vec![profile.clone()])]);
    let value = domain_node::DataMessage {
        push_node_profiles: vec![profile],
        want_asset_keys: vec![key],
        give_asset_key_locations: locations.clone(),
        push_asset_key_locations: locations,
    };
    compatible::<DataMessageCodec, _>(&legacy_data(&value), &value, true)?;
    assert_eq!(
        DataMessageCodec::encode(&value)?,
        hex::decode(format!("a40081{PROFILE_HEX}0181{KEY_HEX}02a1{KEY_HEX}81{PROFILE_HEX}03a1{KEY_HEX}81{PROFILE_HEX}"))?
    );
    Ok(())
}

#[test]
fn multiple_maps_are_mutually_decodable_with_equal_contents() -> TestResult {
    for count in [2, 3, 32] {
        let locations: HashMap<_, _> = (0..count)
            .map(|index| {
                let key = Arc::new(AssetKey {
                    typ: format!("key-{index}"),
                    hash: hash(),
                });
                (key, vec![Arc::new(profile())])
            })
            .collect();
        let value = domain_node::DataMessage {
            give_asset_key_locations: locations.clone(),
            push_asset_key_locations: locations,
            ..Default::default()
        };
        compatible::<DataMessageCodec, _>(&legacy_data(&value), &value, false)?;
        let wire = DataMessageCodec::to_wire(&value);
        assert_eq!(DataMessageCodec::from_wire(wire)?, value);
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
fn nonce_acceptance_is_unchanged_including_duplicates() -> TestResult {
    for lengths in [vec![31], vec![32], vec![33], vec![31, 32], vec![32, 31]] {
        let mut bytes = Vec::new();
        let mut encoder = RocketPackBytesEncoder::new(&mut bytes);
        encoder.write_map(lengths.len())?;
        for length in lengths {
            encoder.write_u64(0)?;
            encoder.write_bytes(&vec![7; length])?;
        }
        assert_eq!(legacy::session::V1ChallengeMessage::import(&bytes).is_ok(), V1ChallengeMessageCodec::decode(&bytes).is_ok());
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
    let new = NodeProfile::new(vec![4; 256], vec![OmniAddr::new(format!("{}aa", "日".repeat(170)))]);
    compatible::<NodeProfileCodec, _>(&legacy_profile(&new), &new, true)
}

#[test]
fn duplicate_map_keys_keep_the_last_value() -> TestResult {
    let first = legacy_profile(&profile());
    let last = legacy::NodeProfile::new(vec![9], vec![]);
    let key = key();
    for field in [2, 3] {
        let mut bytes = Vec::new();
        let mut encoder = RocketPackBytesEncoder::new(&mut bytes);
        encoder.write_map(4)?;
        encoder.write_u64(0)?;
        encoder.write_array(0)?;
        encoder.write_u64(1)?;
        encoder.write_array(0)?;
        for current in [2, 3] {
            encoder.write_u64(current)?;
            encoder.write_map(if field == current { 2 } else { 0 })?;
            if field == current {
                for profile in [&first, &last] {
                    encoder.write_struct(&key)?;
                    encoder.write_array(1)?;
                    encoder.write_struct(profile)?;
                }
            }
        }
        let old = legacy::DataMessage::import(&bytes)?;
        let new = DataMessageCodec::decode(&bytes)?;
        assert_eq!(legacy_data(&new), old);
        let map = if field == 2 { new.give_asset_key_locations } else { new.push_asset_key_locations };
        assert_eq!(map[&key].as_slice()[0].public_key(), &[9]);
    }
    Ok(())
}

#[test]
fn byte_length_limits_reject_old_oversized_fields() -> TestResult {
    for (public_key, addr, accepted) in [
        (vec![0; 256], "a".repeat(512), true),
        (vec![0; 257], "a".to_string(), false),
        (vec![0], "a".repeat(513), false),
        (vec![0], format!("{}aa", "日".repeat(170)), true),
        (vec![0], "日".repeat(171), false),
    ] {
        let profile = NodeProfile::new(public_key, vec![OmniAddr::new(addr)]);
        let old = legacy_profile(&profile).export()?;
        assert_eq!(NodeProfileCodec::encode(&profile).is_ok(), accepted);
        assert_eq!(NodeProfileCodec::decode(&old).is_ok(), accepted);
        assert_eq!(axus::model::NodeProfile::import(&old).is_ok(), accepted);
        if accepted {
            assert_eq!(NodeProfileCodec::decode(&old)?, profile);
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
        // 上限ちょうどまで未知 field で埋めた、構造上は有効な V1 message。
        let mut bytes = DataMessageCodec::encode(&domain_node::DataMessage::default())?;
        bytes[0] = 0xa5;
        let padding = size - bytes.len() - 6;
        let mut encoder = RocketPackBytesEncoder::new(&mut bytes);
        encoder.write_u64(4)?;
        encoder.write_bytes(&vec![0; padding])?;
        assert_eq!(bytes.len(), size);
        if size == limit {
            assert_eq!(DataMessageCodec::decode(&bytes)?, domain_node::DataMessage::default());
        } else {
            assert!(matches!(
                DataMessageCodec::decode(&bytes),
                Err(RocketPackDecoderError::LengthOutOfRange { context: "DataMessage", .. })
            ));
        }
    }
    // 不正な構造でも、巨大入力は field を読む前にサイズ違反となる。
    let bytes = vec![0xff; limit + 1];
    assert!(matches!(
        DataMessageCodec::decode(&bytes),
        Err(RocketPackDecoderError::LengthOutOfRange { context: "DataMessage", .. })
    ));
    Ok(())
}
