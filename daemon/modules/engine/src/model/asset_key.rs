pub use crate::generated::axus::model::AssetKey;
use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
use std::cmp::Ordering;

use crate::model::ProfileId;
impl Ord for AssetKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.typ
            .cmp(&other.typ)
            .then_with(|| Self::algorithm_order(&self.hash.typ).cmp(&Self::algorithm_order(&other.hash.typ)))
            .then_with(|| self.hash.value.cmp(&other.hash.value))
    }
}
impl PartialOrd for AssetKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl AssetKey {
    /// The type of the AssetKey that stands for a published file, whose hash is the root hash.
    const FILE_TYPE: &str = "file";
    /// The type of the AssetKey that stands for a Profile, whose hash is SHA3-256 of the Profile ID.
    const PROFILE_TYPE: &str = "profile";

    pub fn file(root_hash: OmniHash) -> Self {
        Self {
            typ: Self::FILE_TYPE.to_string(),
            hash: root_hash,
        }
    }

    pub fn profile(id: &ProfileId) -> Self {
        Self {
            typ: Self::PROFILE_TYPE.to_string(),
            hash: OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, id.as_str().as_bytes()),
        }
    }

    fn algorithm_order(value: &OmniHashAlgorithmType) -> u8 {
        match value {
            OmniHashAlgorithmType::None => 0,
            OmniHashAlgorithmType::Sha3_256 => 1,
            OmniHashAlgorithmType::Blake3_256 => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::AssetKey;
    use crate::model::ProfileId;

    #[test]
    fn a_profile_key_hashes_the_profile_id_with_sha3_256() -> TestResult {
        let id = ProfileId::new("Anonymous", b"known-public-key")?;
        let key = AssetKey::profile(&id);

        assert_eq!(key.typ, "profile");
        assert_eq!(
            key.hash,
            omnius_core_omnikit::generated::omni_hash::OmniHash::compute_hash(omnius_core_omnikit::generated::omni_hash::OmniHashAlgorithmType::Sha3_256, id.as_str().as_bytes())
        );
        assert_ne!(key.typ, AssetKey::file(key.hash.clone()).typ);
        Ok(())
    }
}
