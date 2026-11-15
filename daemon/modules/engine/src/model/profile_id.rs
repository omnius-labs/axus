use std::{fmt, str::FromStr};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use omnius_core_omnikit::generated::{
    omni_hash::{OmniHash, OmniHashAlgorithmType},
    omni_sign::OmniSigner,
};

use crate::{generated::axus::model as wire, prelude::*, result::Result};

/// The immutable identity of a Profile: a 1-32 byte display name and a key-derived suffix.
///
/// The textual form is `name@u<base64url(SHA3-256(DER public key))>` and is at most 80 bytes.
/// Parsing rejects an empty or invalid name and a suffix that is not canonical base64url.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProfileId(String);

impl ProfileId {
    /// The maximum encoded length of a Profile ID.
    pub const MAX_LENGTH: usize = wire::MAX_PROFILE_ID_LENGTH as usize;

    /// The maximum byte length of the display name.
    pub const MAX_NAME_LENGTH: usize = 32;

    /// Builds the ID of `name` with the suffix derived from `public_key`.
    pub fn new(name: &str, public_key: &[u8]) -> Result<Self> {
        Self::validate_name(name)?;
        let id = format!("{name}@{}", Self::suffix_for_public_key(public_key));
        Self::parse(&id)
    }

    /// Builds the ID of the signer from its current name and public key.
    pub fn from_signer(signer: &OmniSigner) -> Result<Self> {
        let public_key = signer.sign(&[])?.public_key;
        Self::new(&signer.name, &public_key)
    }

    /// Parses and validates a Profile ID in its textual form.
    pub fn parse(text: &str) -> Result<Self> {
        Self::validate_text(text)?;
        Ok(Self(text.to_owned()))
    }

    /// The display part before `@`.
    pub fn name(&self) -> &str {
        self.0.split_once('@').expect("a Profile ID always contains @").0
    }

    /// The key-derived suffix in its textual form.
    pub fn suffix(&self) -> &str {
        self.0.split_once('@').expect("a Profile ID always contains @").1
    }

    /// Returns an error when `public_key` does not produce this ID.
    pub fn verify_public_key(&self, public_key: &[u8]) -> Result<()> {
        if self.suffix() != Self::suffix_for_public_key(public_key) {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile id does not match the public key"));
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate_name(name: &str) -> Result<()> {
        OmniSigner::validate_name(name)?;
        if name.is_empty() {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile id name must not be empty"));
        }
        Ok(())
    }

    fn validate_text(text: &str) -> Result<()> {
        if text.len() > Self::MAX_LENGTH {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile id exceeds 80 bytes"));
        }

        let (name, suffix) = text
            .split_once('@')
            .ok_or_else(|| Error::new(ErrorKind::InvalidFormat).with_message("profile id must contain @"))?;
        Self::validate_name(name)?;

        let encoded = suffix
            .strip_prefix('u')
            .ok_or_else(|| Error::new(ErrorKind::InvalidFormat).with_message("profile id suffix must start with u"))?;
        let value = URL_SAFE_NO_PAD.decode(encoded)?;
        if value.len() != 32 {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile id suffix must encode a SHA3-256 digest"));
        }
        if URL_SAFE_NO_PAD.encode(&value) != encoded {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile id suffix is not canonical base64url"));
        }

        Ok(())
    }

    fn suffix_for_public_key(public_key: &[u8]) -> String {
        let hash = OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, public_key);
        format!("u{}", URL_SAFE_NO_PAD.encode(hash.value))
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ProfileId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for ProfileId {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        Self::parse(text)
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use testresult::TestResult;

    use super::ProfileId;

    const KNOWN_PUBLIC_KEY: &[u8] = b"known-public-key";
    const KNOWN_ID: &str = "Anonymous@ucW39unYyT0GeZHaykut0Y8CqBDZ7pz7rcex8vKE7fRU";

    #[test]
    fn a_known_public_key_produces_a_known_id() -> TestResult {
        let id = ProfileId::new("Anonymous", KNOWN_PUBLIC_KEY)?;

        assert_eq!(id.as_str(), KNOWN_ID);
        assert_eq!(id.name(), "Anonymous");
        assert_eq!(id.suffix(), "ucW39unYyT0GeZHaykut0Y8CqBDZ7pz7rcex8vKE7fRU");
        assert_eq!(id.to_string(), KNOWN_ID);
        assert_eq!(KNOWN_ID.parse::<ProfileId>()?, id);
        id.verify_public_key(KNOWN_PUBLIC_KEY)?;
        Ok(())
    }

    #[test]
    fn an_empty_name_is_rejected() {
        assert!(ProfileId::new("", KNOWN_PUBLIC_KEY).is_err());
        assert!(format!("@{KNOWN_ID}").parse::<ProfileId>().is_err());
        assert!(KNOWN_ID.split_once('@').unwrap().1.parse::<ProfileId>().is_err());
    }

    #[test]
    fn an_invalid_name_is_rejected() {
        let suffix = KNOWN_ID.split_once('@').unwrap().1;
        for name in ["bad name", "bad@name", "bad/name", "日本語"] {
            let text = format!("{name}@{suffix}");
            assert!(text.parse::<ProfileId>().is_err(), "{name}");
        }

        let long = format!("{}@{suffix}", "a".repeat(ProfileId::MAX_NAME_LENGTH + 1));
        assert!(long.parse::<ProfileId>().is_err());
    }

    #[test]
    fn a_tampered_suffix_is_rejected_by_the_public_key_check() -> TestResult {
        let id = ProfileId::new("Anonymous", KNOWN_PUBLIC_KEY)?;
        let tampered = format!("{}A", &KNOWN_ID[..KNOWN_ID.len() - 1]).parse::<ProfileId>()?;

        assert_ne!(tampered, id);
        assert!(tampered.verify_public_key(KNOWN_PUBLIC_KEY).is_err());
        assert!(id.verify_public_key(b"another-public-key").is_err());
        Ok(())
    }

    #[test]
    fn a_malformed_suffix_is_rejected() {
        let suffix = KNOWN_ID.split_once('@').unwrap().1;
        let mut suffixes = vec!["".to_string(), "u".to_string(), "uAAAA".to_string(), "AAAA".to_string(), "uC".to_string()];
        suffixes.push(format!("{suffix}="));
        for suffix in suffixes {
            let text = format!("Anonymous@{suffix}");
            assert!(text.parse::<ProfileId>().is_err(), "{suffix}");
        }

        let non_canonical = format!("{}W", &KNOWN_ID[..KNOWN_ID.len() - 1]);
        assert!(non_canonical.parse::<ProfileId>().is_err());
    }

    #[test]
    fn an_id_over_the_length_limit_is_rejected() {
        let text = "a".repeat(ProfileId::MAX_LENGTH + 1);
        assert!(text.parse::<ProfileId>().is_err());
    }

    #[test]
    fn a_signer_produces_an_id_for_its_public_key() -> TestResult {
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "Anonymous")?;
        let id = ProfileId::from_signer(&signer)?;
        let public_key = signer.sign(&[])?.public_key;

        assert_eq!(id.name(), "Anonymous");
        assert_eq!(id.to_string(), signer.to_string());
        id.verify_public_key(&public_key)?;
        Ok(())
    }
}
