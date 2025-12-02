use omnius_core_omnikit::model::OmniCert;

use crate::prelude::*;

#[repr(u32)]
#[enumflags2::bitflags]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum SessionVersion {
    V1 = 1,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HelloMessage {
    pub version: SessionVersion,
}

impl RocketPackStruct for HelloMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_u32(value.version as u32)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut version: Option<SessionVersion> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => {
                    let raw = decoder.read_u32()?;
                    version = Some(SessionVersion::from_repr(raw).ok_or(RocketPackDecoderError::Other("invalid session version"))?);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            version: version.ok_or(RocketPackDecoderError::Other("missing field: version"))?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1ChallengeMessage {
    pub nonce: [u8; 32],
}

impl RocketPackStruct for V1ChallengeMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_bytes(&value.nonce)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut nonce: Option<[u8; 32]> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => {
                    let bytes = decoder.read_bytes_vec()?;
                    nonce = Some(bytes.try_into().map_err(|_| RocketPackDecoderError::Other("invalid nonce length"))?);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            nonce: nonce.ok_or(RocketPackDecoderError::Other("missing field: nonce"))?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1SignatureMessage {
    pub cert: OmniCert,
}

impl RocketPackStruct for V1SignatureMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_struct(&value.cert)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut cert: Option<OmniCert> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => cert = Some(decoder.read_struct::<OmniCert>()?),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            cert: cert.ok_or(RocketPackDecoderError::Other("missing field: cert"))?,
        })
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum V1RequestType {
    Unknown = 0,
    NodeFinder = 1,
    FileExchanger = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1RequestMessage {
    pub request_type: V1RequestType,
}

impl RocketPackStruct for V1RequestMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_u32(value.request_type as u32)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut request_type: Option<V1RequestType> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => {
                    let raw = decoder.read_u32()?;
                    request_type = Some(V1RequestType::from_repr(raw).ok_or(RocketPackDecoderError::Other("invalid request type"))?);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            request_type: request_type.ok_or(RocketPackDecoderError::Other("missing field: request_type"))?,
        })
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumString, strum::AsRefStr, strum::Display, strum::FromRepr)]
pub enum V1ResultType {
    Unknown,
    Accept,
    Reject,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1ResultMessage {
    pub result_type: V1ResultType,
}

impl RocketPackStruct for V1ResultMessage {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_u32(value.result_type as u32)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut result_type: Option<V1ResultType> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => {
                    let raw = decoder.read_u32()?;
                    result_type = Some(V1ResultType::from_repr(raw).ok_or(RocketPackDecoderError::Other("invalid result type"))?);
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            result_type: result_type.ok_or(RocketPackDecoderError::Other("missing field: result_type"))?,
        })
    }
}
