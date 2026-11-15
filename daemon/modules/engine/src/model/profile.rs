use std::{collections::HashSet, time::Duration};

use chrono::{DateTime, Utc};
use omnius_core_omnikit::generated::omni_sign::{OmniCert, OmniSignType, OmniSigner};

use crate::{generated::axus::model as wire, model::ProfileId, prelude::*, result::Result};

/// The fields an owner signs when creating or updating a Profile.
///
/// An update always supplies the complete draft; the version and update time are added by `Profile::sign`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileDraft {
    pub trusted: Vec<ProfileId>,
    pub blocked: Vec<ProfileId>,
    pub file_refs: Vec<wire::FileRef>,
    pub messages: Vec<wire::ProfileMessage>,
}

/// A signed snapshot of one Profile identity and the lists it carries.
///
/// A value exists only after its signature, ID, limits, and trust/block rules were verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    id: ProfileId,
    public_key: Vec<u8>,
    version: u64,
    updated_time: i64,
    trusted: Vec<ProfileId>,
    blocked: Vec<ProfileId>,
    file_refs: Vec<wire::FileRef>,
    messages: Vec<wire::ProfileMessage>,
    signature: Vec<u8>,
}

/// The fixed prefix that separates a Profile signature from a NodeProfile signature.
const SIGNING_PREFIX: &[u8] = b"axus.profile.v1\0";

impl Profile {
    /// The tolerance for an update time that is ahead of the receiver's clock.
    pub const MAX_FUTURE_UPDATE_SKEW: Duration = Duration::from_secs(5 * 60);

    pub const MAX_TRUSTED: usize = wire::MAX_PROFILE_TRUSTED as usize;
    pub const MAX_BLOCKED: usize = wire::MAX_PROFILE_BLOCKED as usize;
    pub const MAX_FILE_REFS: usize = wire::MAX_PROFILE_FILE_REFS as usize;
    pub const MAX_MESSAGES: usize = wire::MAX_PROFILE_MESSAGES as usize;
    pub const MAX_INLINE_MESSAGE_BODY_LENGTH: usize = wire::MAX_INLINE_MESSAGE_BODY_LENGTH as usize;
    pub const MAX_ENCODED_LENGTH: usize = wire::MAX_ENCODED_PROFILE_LENGTH as usize;

    /// Signs a complete draft with the Profile key.
    ///
    /// The Profile ID is derived from the signer name and public key.
    pub fn sign(signer: &OmniSigner, version: u64, updated_time: DateTime<Utc>, draft: ProfileDraft) -> Result<Self> {
        let public_key = signer.sign(&[])?.public_key;
        let id = ProfileId::new(&signer.name, &public_key)?;
        let updated_time = updated_time.timestamp();
        let payload = wire::ProfileSigningPayload {
            id: id.to_string(),
            public_key: public_key.clone(),
            version,
            updated_time,
            trusted: draft.trusted.iter().map(ToString::to_string).collect(),
            blocked: draft.blocked.iter().map(ToString::to_string).collect(),
            file_refs: draft.file_refs.clone(),
            messages: draft.messages.clone(),
        };
        let message = Self::signing_message(&payload)?;
        let signature = signer.sign(&message)?.value;

        let profile = Self {
            id,
            public_key,
            version,
            updated_time,
            trusted: draft.trusted,
            blocked: draft.blocked,
            file_refs: draft.file_refs,
            messages: draft.messages,
            signature,
        };
        profile.validate()?;

        Ok(profile)
    }

    /// Verifies the wire body and returns it as a Profile.
    pub(crate) fn verified(value: wire::Profile) -> Result<Self> {
        let id = ProfileId::parse(&value.id)?;
        let payload = wire::ProfileSigningPayload {
            id: value.id.clone(),
            public_key: value.public_key.clone(),
            version: value.version,
            updated_time: value.updated_time,
            trusted: value.trusted.clone(),
            blocked: value.blocked.clone(),
            file_refs: value.file_refs.clone(),
            messages: value.messages.clone(),
        };
        let message = Self::signing_message(&payload)?;
        let cert = OmniCert {
            typ: OmniSignType::Ed25519_Sha3_256_Base64Url,
            name: id.name().to_owned(),
            public_key: value.public_key.clone(),
            value: value.signature.clone(),
        };
        cert.verify(&message)
            .map_err(|e| Error::from_error(e, ErrorKind::InvalidFormat).with_message("profile signature is invalid"))?;

        let trusted = value.trusted.iter().map(|entry| ProfileId::parse(entry)).collect::<Result<Vec<_>>>()?;
        let blocked = value.blocked.iter().map(|entry| ProfileId::parse(entry)).collect::<Result<Vec<_>>>()?;
        let profile = Self {
            id,
            public_key: value.public_key,
            version: value.version,
            updated_time: value.updated_time,
            trusted,
            blocked,
            file_refs: value.file_refs,
            messages: value.messages,
            signature: value.signature,
        };
        profile.validate()?;

        Ok(profile)
    }

    pub fn export(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(self.to_wire().export()?)
    }

    pub fn import(bytes: &[u8]) -> Result<Self> {
        Self::verified(wire::Profile::import(bytes)?)
    }

    pub fn id(&self) -> &ProfileId {
        &self.id
    }

    /// The DER public key carried in the body.
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// The signing time as unix seconds.
    pub fn updated_time(&self) -> i64 {
        self.updated_time
    }

    pub fn trusted(&self) -> &[ProfileId] {
        &self.trusted
    }

    pub fn blocked(&self) -> &[ProfileId] {
        &self.blocked
    }

    pub fn file_refs(&self) -> &[wire::FileRef] {
        &self.file_refs
    }

    pub fn messages(&self) -> &[wire::ProfileMessage] {
        &self.messages
    }

    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    /// Whether the update time is not more than the allowed skew ahead of `now`.
    pub fn is_updated_within_skew(&self, now: DateTime<Utc>) -> bool {
        let limit = now.timestamp().saturating_add(Self::MAX_FUTURE_UPDATE_SKEW.as_secs() as i64);
        self.updated_time <= limit
    }

    fn validate(&self) -> Result<()> {
        if self.version == 0 {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile version must be at least 1"));
        }
        self.id.verify_public_key(&self.public_key)?;

        if self.trusted.len() > Self::MAX_TRUSTED {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile has too many trust entries"));
        }
        if self.blocked.len() > Self::MAX_BLOCKED {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile has too many block entries"));
        }
        if self.file_refs.len() > Self::MAX_FILE_REFS {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile has too many file references"));
        }
        if self.messages.len() > Self::MAX_MESSAGES {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile has too many messages"));
        }

        let mut seen = HashSet::new();
        for entry in &self.trusted {
            if entry == &self.id {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile cannot trust itself"));
            }
            seen.insert(entry);
        }
        for entry in &self.blocked {
            if entry == &self.id {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile cannot block itself"));
            }
            if !seen.insert(entry) {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile id appears in both trust and block lists"));
            }
        }

        for message in &self.messages {
            if let wire::ProfileMessageBody::Inline { value } = &message.body
                && value.len() > Self::MAX_INLINE_MESSAGE_BODY_LENGTH
            {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("inline message body exceeds 64 KiB"));
            }
        }
        if !self.messages.windows(2).all(|pair| pair[0].posted_time <= pair[1].posted_time) {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile messages are not ordered by posted time"));
        }

        let encoded = self.to_wire().export()?;
        if encoded.len() > Self::MAX_ENCODED_LENGTH {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("encoded profile exceeds 1 MiB"));
        }

        Ok(())
    }

    fn signing_message(payload: &wire::ProfileSigningPayload) -> Result<Vec<u8>> {
        let mut message = SIGNING_PREFIX.to_vec();
        message.extend(payload.export()?);
        Ok(message)
    }

    fn to_wire(&self) -> wire::Profile {
        wire::Profile {
            id: self.id.to_string(),
            public_key: self.public_key.clone(),
            version: self.version,
            updated_time: self.updated_time,
            trusted: self.trusted.iter().map(ToString::to_string).collect(),
            blocked: self.blocked.iter().map(ToString::to_string).collect(),
            file_refs: self.file_refs.clone(),
            messages: self.messages.clone(),
            signature: self.signature.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use omnius_core_omnikit::generated::{
        omni_hash::{OmniHash, OmniHashAlgorithmType},
        omni_sign::{OmniSignType, OmniSigner},
    };
    use testresult::TestResult;

    use super::{Profile, ProfileDraft};
    use crate::{generated::axus::model as wire, model::ProfileId, prelude::*};

    fn at(seconds: i64) -> DateTime<chrono::Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn signer() -> OmniSigner {
        OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "Anonymous").unwrap()
    }

    fn other_id(name: &str) -> ProfileId {
        ProfileId::new(name, format!("public-key-of-{name}").as_bytes()).unwrap()
    }

    fn file_ref(name: &str) -> wire::FileRef {
        wire::FileRef {
            name: name.to_owned(),
            hash: OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, name.as_bytes()),
        }
    }

    fn message(posted_time: i64, value: &[u8]) -> wire::ProfileMessage {
        wire::ProfileMessage {
            posted_time,
            body: wire::ProfileMessageBody::Inline { value: value.to_vec() },
        }
    }

    fn wire_profile() -> wire::Profile {
        wire::Profile {
            id: "Anonymous@uAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
            public_key: vec![1; 32],
            version: 1,
            updated_time: 100,
            trusted: vec![],
            blocked: vec![],
            file_refs: vec![file_ref("file.txt")],
            messages: vec![
                message(100, b"inline"),
                wire::ProfileMessage {
                    posted_time: 200,
                    body: wire::ProfileMessageBody::FileRef {
                        value: file_ref("attachment.bin"),
                    },
                },
            ],
            signature: vec![0; 64],
        }
    }

    #[test]
    fn profile_model_wire_profile_round_trips() -> TestResult {
        let value = wire_profile();
        let decoded = wire::Profile::import(&value.export()?)?;

        assert_eq!(decoded, value);
        Ok(())
    }

    #[test]
    fn profile_model_wire_rejects_values_outside_the_limits() -> TestResult {
        let mut too_long_id = wire_profile();
        too_long_id.id = "a".to_owned() + &"b".repeat(wire::MAX_PROFILE_ID_LENGTH as usize);
        assert!(too_long_id.export().is_err());

        let mut too_many_trusted = wire_profile();
        too_many_trusted.trusted = vec!["entry".to_owned(); wire::MAX_PROFILE_TRUSTED as usize + 1];
        assert!(too_many_trusted.export().is_err());

        let mut too_many_blocked = wire_profile();
        too_many_blocked.blocked = vec!["entry".to_owned(); wire::MAX_PROFILE_BLOCKED as usize + 1];
        assert!(too_many_blocked.export().is_err());

        let mut too_many_file_refs = wire_profile();
        too_many_file_refs.file_refs = vec![file_ref("file"); wire::MAX_PROFILE_FILE_REFS as usize + 1];
        assert!(too_many_file_refs.export().is_err());

        let mut too_many_messages = wire_profile();
        too_many_messages.messages = vec![message(0, b""); wire::MAX_PROFILE_MESSAGES as usize + 1];
        assert!(too_many_messages.export().is_err());

        let mut too_large_inline = wire_profile();
        too_large_inline.messages = vec![message(0, &vec![0; wire::MAX_INLINE_MESSAGE_BODY_LENGTH as usize + 1])];
        assert!(too_large_inline.export().is_err());
        Ok(())
    }

    #[test]
    fn profile_model_signed_profile_is_verified_after_a_wire_roundtrip() -> TestResult {
        let profile = Profile::sign(&signer(), 1, at(100), ProfileDraft::default())?;

        assert_eq!(Profile::import(&profile.export()?)?, profile);
        assert_eq!(profile.version(), 1);
        assert_eq!(profile.updated_time(), 100);
        assert_eq!(profile.id().name(), "Anonymous");
        Ok(())
    }

    #[test]
    fn profile_model_signature_rejects_any_changed_field() -> TestResult {
        let profile = Profile::sign(&signer(), 1, at(100), ProfileDraft::default())?;
        let other = other_id("Other");
        let verify_mutated = |mutate: &dyn Fn(&mut wire::Profile)| {
            let mut value = profile.to_wire();
            mutate(&mut value);
            Profile::verified(value).is_ok()
        };

        assert!(verify_mutated(&|_| {}));
        assert!(!verify_mutated(&|value| value.id = other.to_string()));
        assert!(!verify_mutated(&|value| value.public_key[0] ^= 1));
        assert!(!verify_mutated(&|value| value.version += 1));
        assert!(!verify_mutated(&|value| value.updated_time += 1));
        assert!(!verify_mutated(&|value| value.trusted.push(other.to_string())));
        assert!(!verify_mutated(&|value| value.blocked.push(other.to_string())));
        assert!(!verify_mutated(&|value| value.file_refs.push(file_ref("other"))));
        assert!(!verify_mutated(&|value| value.messages.push(message(200, b"other"))));
        assert!(!verify_mutated(&|value| value.signature[0] ^= 1));
        Ok(())
    }

    #[test]
    fn profile_model_rejects_an_id_that_does_not_match_the_public_key() -> TestResult {
        let signer = signer();
        let public_key = signer.sign(&[])?.public_key;
        let other = other_id("Other");
        let payload = wire::ProfileSigningPayload {
            id: other.to_string(),
            public_key: public_key.clone(),
            version: 1,
            updated_time: 100,
            trusted: vec![],
            blocked: vec![],
            file_refs: vec![],
            messages: vec![],
        };
        let mut message = super::SIGNING_PREFIX.to_vec();
        message.extend(payload.export()?);
        let signature = signer.sign(&message)?.value;
        let value = wire::Profile {
            id: other.to_string(),
            public_key,
            version: 1,
            updated_time: 100,
            trusted: vec![],
            blocked: vec![],
            file_refs: vec![],
            messages: vec![],
            signature,
        };

        assert!(Profile::verified(value).is_err());
        Ok(())
    }

    #[test]
    fn profile_model_self_trust_self_block_and_overlap_are_rejected() -> TestResult {
        let signer = signer();
        let own = Profile::sign(&signer, 1, at(100), ProfileDraft::default())?.id().clone();
        let other = other_id("Other");

        assert!(
            Profile::sign(
                &signer,
                2,
                at(101),
                ProfileDraft {
                    trusted: vec![own.clone()],
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            Profile::sign(
                &signer,
                2,
                at(101),
                ProfileDraft {
                    blocked: vec![own],
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            Profile::sign(
                &signer,
                2,
                at(101),
                ProfileDraft {
                    trusted: vec![other.clone()],
                    blocked: vec![other],
                    ..Default::default()
                }
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn profile_model_limits_are_enforced() -> TestResult {
        let signer = signer();
        let other = other_id("Other");
        let sign = |draft: ProfileDraft| Profile::sign(&signer, 1, at(100), draft);

        assert!(
            sign(ProfileDraft {
                trusted: vec![other.clone(); Profile::MAX_TRUSTED + 1],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            sign(ProfileDraft {
                blocked: vec![other.clone(); Profile::MAX_BLOCKED + 1],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            sign(ProfileDraft {
                file_refs: vec![file_ref("file"); Profile::MAX_FILE_REFS + 1],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            sign(ProfileDraft {
                messages: vec![message(0, b""); Profile::MAX_MESSAGES + 1],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            sign(ProfileDraft {
                messages: vec![message(0, &vec![0; Profile::MAX_INLINE_MESSAGE_BODY_LENGTH + 1])],
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            sign(ProfileDraft {
                messages: vec![message(200, b"later"), message(100, b"earlier")],
                ..Default::default()
            })
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn profile_model_an_encoded_profile_over_one_mib_is_rejected() -> TestResult {
        let file_refs = vec![file_ref(&"a".repeat(1024)); Profile::MAX_FILE_REFS];
        let draft = ProfileDraft { file_refs, ..Default::default() };

        assert!(Profile::sign(&signer(), 1, at(100), draft).is_err());
        Ok(())
    }

    #[test]
    fn profile_model_version_zero_is_rejected() {
        assert!(Profile::sign(&signer(), 0, at(100), ProfileDraft::default()).is_err());
    }

    #[test]
    fn profile_model_update_time_may_be_ahead_of_now_by_the_allowed_skew_only() -> TestResult {
        let now = at(1_000_000);
        let skew = Profile::MAX_FUTURE_UPDATE_SKEW.as_secs() as i64;
        let signed_at = |seconds: i64| Profile::sign(&signer(), 1, at(seconds), ProfileDraft::default()).unwrap();

        assert!(signed_at(1_000_000 - 86_400 * 365).is_updated_within_skew(now));
        assert!(signed_at(1_000_000 + skew).is_updated_within_skew(now));
        assert!(!signed_at(1_000_000 + skew + 1).is_updated_within_skew(now));
        Ok(())
    }
}
