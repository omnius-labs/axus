use super::MessageCodec;
use crate::{core::session::message as domain, generated::axus::session as wire, prelude::*};
mod decoder;
use decoder::{ScalarDecoder, ScalarPolicy};

pub(crate) struct HelloMessageCodec;
impl MessageCodec for HelloMessageCodec {
    type Message = domain::HelloMessage;
    type Wire = wire::HelloMessage;
    fn to_wire(value: &Self::Message) -> Self::Wire {
        Self::Wire { version: value.version as u32 }
    }
    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Ok(Self::Message {
            version: domain::SessionVersion::from_repr(value.version).ok_or(RocketPackDecoderError::Other("invalid session version"))?,
        })
    }
    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        let value = Self::Wire::unpack(&mut ScalarDecoder::new(decoder, ScalarPolicy::Version))?;
        if decoder.remaining() != 0 {
            return Err(RocketPackDecoderError::Other("trailing session message bytes"));
        }
        Self::from_wire(value)
    }
}

pub(crate) struct V2RequestMessageCodec;
impl MessageCodec for V2RequestMessageCodec {
    type Message = domain::V2RequestMessage;
    type Wire = wire::V2RequestMessage;
    fn to_wire(value: &Self::Message) -> Self::Wire {
        Self::Wire {
            request_type: value.request_type as u32,
        }
    }
    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Ok(Self::Message {
            request_type: domain::V2RequestType::from_repr(value.request_type).ok_or(RocketPackDecoderError::Other("invalid request type"))?,
        })
    }
    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        let value = Self::Wire::unpack(&mut ScalarDecoder::new(decoder, ScalarPolicy::Request))?;
        if decoder.remaining() != 0 {
            return Err(RocketPackDecoderError::Other("trailing session message bytes"));
        }
        Self::from_wire(value)
    }
}

pub(crate) struct V2ResultMessageCodec;
impl MessageCodec for V2ResultMessageCodec {
    type Message = domain::V2ResultMessage;
    type Wire = wire::V2ResultMessage;
    fn to_wire(value: &Self::Message) -> Self::Wire {
        Self::Wire {
            result_type: value.result_type as u32,
        }
    }
    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Ok(Self::Message {
            result_type: domain::V2ResultType::from_repr(value.result_type).ok_or(RocketPackDecoderError::Other("invalid result type"))?,
        })
    }
    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        let value = Self::Wire::unpack(&mut ScalarDecoder::new(decoder, ScalarPolicy::Result))?;
        if decoder.remaining() != 0 {
            return Err(RocketPackDecoderError::Other("trailing session message bytes"));
        }
        Self::from_wire(value)
    }
}
