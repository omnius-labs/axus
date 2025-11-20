use omnius_core_omnikit::model::OmniHash;

use crate::prelude::*;

pub struct MerkleLayer {
    pub rank: u32,
    pub hashes: Vec<OmniHash>,
}

impl RocketPackStruct for MerkleLayer {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(2)?;

        encoder.write_u64(0)?;
        encoder.write_u32(value.rank)?;

        encoder.write_u64(1)?;
        encoder.write_array(value.hashes.len())?;

        for v in value.hashes.iter() {
            encoder.write_struct(v)?;
        }

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut rank: Option<u32> = None;
        let mut hashes: Option<Vec<OmniHash>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => rank = Some(decoder.read_u32()?),
                1 => {
                    let count = decoder.read_array()?;
                    let mut vs: Vec<OmniHash> = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        vs.push(decoder.read_struct()?);
                    }
                    hashes = Some(vs);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            rank: rank.ok_or(RocketPackDecoderError::Other("missing field: rank"))?,
            hashes: hashes.ok_or(RocketPackDecoderError::Other("missing field: hashes"))?,
        })
    }
}
