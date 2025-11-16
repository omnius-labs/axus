use omnius_core_omnikit::model::OmniHash;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetKey {
    pub typ: String,
    pub hash: OmniHash,
}

impl RocketPackStruct for AssetKey {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        todo!()
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        todo!()
    }
}
