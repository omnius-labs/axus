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
        Self::from_wire(Self::Wire::unpack(&mut ScalarDecoder::new(decoder, ScalarPolicy::Version))?)
    }
}

pub(crate) struct V1ChallengeMessageCodec;
impl MessageCodec for V1ChallengeMessageCodec {
    type Message = domain::V1ChallengeMessage;
    type Wire = wire::V1ChallengeMessage;
    fn to_wire(value: &Self::Message) -> Self::Wire {
        Self::Wire { nonce: value.nonce.to_vec() }
    }
    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Ok(Self::Message {
            nonce: value.nonce.try_into().map_err(|_| RocketPackDecoderError::Other("invalid nonce length"))?,
        })
    }
}

pub(crate) struct V1SignatureMessageCodec;
impl MessageCodec for V1SignatureMessageCodec {
    type Message = domain::V1SignatureMessage;
    type Wire = wire::V1SignatureMessage;
    fn to_wire(value: &Self::Message) -> Self::Wire {
        Self::Wire { cert: value.cert.clone() }
    }
    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Ok(Self::Message { cert: value.cert })
    }
}

pub(crate) struct V1RequestMessageCodec;
impl MessageCodec for V1RequestMessageCodec {
    type Message = domain::V1RequestMessage;
    type Wire = wire::V1RequestMessage;
    fn to_wire(value: &Self::Message) -> Self::Wire {
        Self::Wire {
            request_type: value.request_type as u32,
        }
    }
    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Ok(Self::Message {
            request_type: domain::V1RequestType::from_repr(value.request_type).ok_or(RocketPackDecoderError::Other("invalid request type"))?,
        })
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Self::from_wire(Self::Wire::unpack(&mut ScalarDecoder::new(decoder, ScalarPolicy::Request))?)
    }
}

pub(crate) struct V1ResultMessageCodec;
impl MessageCodec for V1ResultMessageCodec {
    type Message = domain::V1ResultMessage;
    type Wire = wire::V1ResultMessage;
    fn to_wire(value: &Self::Message) -> Self::Wire {
        Self::Wire {
            result_type: value.result_type as u32,
        }
    }
    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Ok(Self::Message {
            result_type: domain::V1ResultType::from_repr(value.result_type).ok_or(RocketPackDecoderError::Other("invalid result type"))?,
        })
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Self::from_wire(Self::Wire::unpack(&mut ScalarDecoder::new(decoder, ScalarPolicy::Result))?)
    }
}
