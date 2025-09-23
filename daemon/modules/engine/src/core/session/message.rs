use bitflags::bitflags;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive, ToPrimitive};

use omnius_core_omnikit::model::OmniCert;

use crate::prelude::*;

bitflags! {
    #[derive(Debug, PartialEq, Eq)]
    pub struct SessionVersion: u32 {
        const V1 = 1;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct HelloMessage {
    pub version: SessionVersion,
}

impl RocketMessage for HelloMessage {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
        writer.put_u32(value.version.bits());

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let version = SessionVersion::from_bits(reader.get_u32()?).ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat).with_message("invalid version"))?;

        Ok(Self { version })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1ChallengeMessage {
    pub nonce: [u8; 32],
}

impl RocketMessage for V1ChallengeMessage {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
        writer.put_bytes(value.nonce.as_slice());

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let nonce: [u8; 32] = reader.get_bytes(32)?.as_slice().try_into()?;

        Ok(Self { nonce })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1SignatureMessage {
    pub cert: OmniCert,
}

impl RocketMessage for V1SignatureMessage {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, depth: u32) -> RocketPackResult<()> {
        OmniCert::pack(writer, &value.cert, depth + 1)?;

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let cert = OmniCert::unpack(reader, depth + 1)?;

        Ok(Self { cert })
    }
}

#[derive(Debug, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum V1RequestType {
    Unknown = 0,
    NodeFinder = 1,
    FileExchanger = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1RequestMessage {
    pub request_type: V1RequestType,
}

impl RocketMessage for V1RequestMessage {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
        writer.put_u32(
            value
                .request_type
                .to_u32()
                .ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat).with_message("invalid request_type"))?,
        );

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let request_type: V1RequestType =
            FromPrimitive::from_u32(reader.get_u32()?).ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat).with_message("invalid request_type"))?;

        Ok(Self { request_type })
    }
}

#[derive(Debug, PartialEq, Eq, FromPrimitive, ToPrimitive)]
pub enum V1ResultType {
    Unknown,
    Accept,
    Reject,
}

#[derive(Debug, PartialEq, Eq)]
pub struct V1ResultMessage {
    pub result_type: V1ResultType,
}

impl RocketMessage for V1ResultMessage {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
        writer.put_u32(
            value
                .result_type
                .to_u32()
                .ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat).with_message("invalid request_type"))?,
        );

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let result_type: V1ResultType =
            FromPrimitive::from_u32(reader.get_u32()?).ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat).with_message("invalid request_type"))?;

        Ok(Self { result_type })
    }
}
