pub(crate) mod node;
mod node_profile;
pub(crate) mod session;
pub(crate) mod uri;

pub(crate) use node_profile::NodeProfileCodec;

use omnius_core_rocketpack::RocketPackBytesDecoder;

use crate::prelude::*;

/// Domain values cross the wire only through an explicit codec.
pub(crate) trait MessageCodec {
    type Message;
    type Wire: RocketPackStruct;

    fn to_wire(value: &Self::Message) -> Self::Wire;
    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError>;

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Self::from_wire(Self::Wire::unpack(decoder)?)
    }

    fn encode(value: &Self::Message) -> std::result::Result<Vec<u8>, RocketPackEncoderError> {
        Self::to_wire(value).export()
    }

    fn decode(bytes: &[u8]) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Self::unpack(&mut RocketPackBytesDecoder::new(bytes))
    }
}

#[cfg(test)]
pub(crate) mod tests;
