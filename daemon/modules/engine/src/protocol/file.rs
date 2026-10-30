use super::MessageCodec;
use crate::{generated::axus::file as wire, prelude::*};

/// The largest frame a FileExchanger message may be: the largest block plus the hash and the tags a block message carries.
pub const MAX_FRAME_LENGTH: usize = wire::MAX_BLOCK_LENGTH as usize + 256;

/// The FileExchanger message is a generated type as it is, so the codec only fixes the wire it travels on.
pub(crate) struct ExchangeMessageCodec;

impl MessageCodec for ExchangeMessageCodec {
    type Message = wire::ExchangeMessage;
    type Wire = wire::ExchangeMessage;

    fn to_wire(value: &Self::Message) -> Self::Wire {
        value.clone()
    }

    fn from_wire(value: Self::Wire) -> std::result::Result<Self::Message, RocketPackDecoderError> {
        Ok(value)
    }

    /// Encodes without copying the block body into a wire value first.
    fn encode(value: &Self::Message) -> std::result::Result<Vec<u8>, RocketPackEncoderError> {
        value.export()
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
    use testresult::TestResult;

    use crate::{
        generated::axus::file::{ExchangeMessage, MAX_BLOCK_LENGTH, MAX_REQUEST_BLOCK_HASHES},
        protocol::MessageCodec as _,
    };

    use super::{ExchangeMessageCodec, MAX_FRAME_LENGTH};

    fn hash(value: u8) -> OmniHash {
        OmniHash {
            typ: OmniHashAlgorithmType::Sha3_256,
            value: vec![value; 64],
        }
    }

    #[test]
    fn every_message_round_trips() -> TestResult {
        let messages = [
            ExchangeMessage::Hello { version: 1 },
            ExchangeMessage::Open { root_hash: hash(1) },
            ExchangeMessage::Accept,
            ExchangeMessage::Reject,
            ExchangeMessage::Status { want: true, choke: false },
            ExchangeMessage::Request {
                block_hashes: vec![hash(2), hash(3)],
            },
            ExchangeMessage::Block {
                block_hash: hash(4),
                value: vec![5; 100],
            },
            ExchangeMessage::HaveMap {
                bits: vec![0b0000_0101, 0b0000_0011],
            },
            ExchangeMessage::Cancel {
                block_hashes: vec![hash(7), hash(8)],
            },
        ];
        for message in messages {
            assert_eq!(ExchangeMessageCodec::decode(&ExchangeMessageCodec::encode(&message)?)?, message);
        }

        Ok(())
    }

    #[test]
    fn the_frame_limit_fits_the_largest_messages_and_rejects_longer_blocks() -> TestResult {
        let largest_block = ExchangeMessage::Block {
            block_hash: hash(1),
            value: vec![0; MAX_BLOCK_LENGTH as usize],
        };
        let largest_request = ExchangeMessage::Request {
            block_hashes: vec![hash(1); MAX_REQUEST_BLOCK_HASHES as usize],
        };
        assert!(ExchangeMessageCodec::encode(&largest_block)?.len() <= MAX_FRAME_LENGTH);
        assert!(ExchangeMessageCodec::encode(&largest_request)?.len() <= MAX_FRAME_LENGTH);
        assert!(
            ExchangeMessageCodec::encode(&ExchangeMessage::Block {
                block_hash: hash(1),
                value: vec![0; MAX_BLOCK_LENGTH as usize + 1],
            })
            .is_err()
        );

        Ok(())
    }

    #[test]
    fn a_request_over_the_hash_limit_is_rejected_at_decode() -> TestResult {
        let over = ExchangeMessage::Request {
            block_hashes: vec![hash(1); MAX_REQUEST_BLOCK_HASHES as usize + 1],
        };
        assert!(ExchangeMessageCodec::encode(&over).is_err());

        Ok(())
    }
}
