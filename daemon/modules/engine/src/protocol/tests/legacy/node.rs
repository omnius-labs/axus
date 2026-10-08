use super::NodeProfile;
use crate::prelude::*;
use enumflags2::BitFlags;
#[repr(u32)]
#[enumflags2::bitflags]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum NodeFinderVersion {
    V1 = 1,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HelloMessage {
    pub version: BitFlags<NodeFinderVersion>,
}

impl RocketPackStruct for HelloMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_u32(value.version.bits())?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut version: Option<BitFlags<NodeFinderVersion>> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => version = Some(BitFlags::<NodeFinderVersion>::from_bits_truncate(decoder.read_u32()?)),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            version: version.ok_or(RocketPackDecoderError::Other("missing field: version"))?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProfileMessage {
    pub node_profile: NodeProfile,
}

impl RocketPackStruct for ProfileMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_struct(&value.node_profile)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut node_profile: Option<NodeProfile> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => node_profile = Some(decoder.read_struct::<NodeProfile>()?),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            node_profile: node_profile.ok_or(RocketPackDecoderError::Other("missing field: node_profile"))?,
        })
    }
}
