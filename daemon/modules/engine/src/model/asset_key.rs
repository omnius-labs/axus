pub use crate::generated::axus::model::AssetKey;
use omnius_core_omnikit::generated::omni_hash::OmniHashAlgorithmType;
use std::cmp::Ordering;
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
    fn algorithm_order(value: &OmniHashAlgorithmType) -> u8 {
        match value {
            OmniHashAlgorithmType::None => 0,
            OmniHashAlgorithmType::Sha3_256 => 1,
            OmniHashAlgorithmType::Blake3_256 => 2,
        }
    }
}
