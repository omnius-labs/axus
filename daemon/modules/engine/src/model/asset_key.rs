use omnius_core_omnikit::model::OmniHash;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetKey {
    pub typ: String,
    pub hash: OmniHash,
}

impl RocketPackStruct for AssetKey {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(2)?;

        encoder.write_u64(0)?;
        encoder.write_string(value.typ.as_str())?;

        encoder.write_u64(1)?;
        encoder.write_struct(&value.hash)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut typ: Option<String> = None;
        let mut hash: Option<OmniHash> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => typ = Some(decoder.read_string()?),
                1 => hash = Some(decoder.read_struct::<OmniHash>()?),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            typ: typ.ok_or(RocketPackDecoderError::Other("missing field: typ"))?,
            hash: hash.ok_or(RocketPackDecoderError::Other("missing field: hash"))?,
        })
    }
}
