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
        todo!()
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        todo!()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1ChallengeMessage {
    pub nonce: [u8; 32],
}

impl RocketPackStruct for V1ChallengeMessage {
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

#[derive(Debug, PartialEq, Eq)]
pub struct V1SignatureMessage {
    pub cert: OmniCert,
}

impl RocketPackStruct for V1SignatureMessage {
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
        todo!()
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        todo!()
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
        todo!()
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        todo!()
    }
}
