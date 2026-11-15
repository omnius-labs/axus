use super::MessageCodec;
use crate::{generated::axus::profile as wire, model::Profile, prelude::*};

/// The largest frame a ProfileExchanger message may be: the largest Profile plus the response envelope.
pub const MAX_FRAME_LENGTH: usize = Profile::MAX_ENCODED_LENGTH + 256;

/// The ProfileExchanger message is a generated type as it is, so the codec only fixes the wire it travels on.
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

    fn encode(value: &Self::Message) -> std::result::Result<Vec<u8>, RocketPackEncoderError> {
        value.export()
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use testresult::TestResult;

    use crate::{
        generated::axus::profile::{ExchangeMessage, MAX_PROFILE_ID_LENGTH},
        model::{Profile, ProfileDraft},
        protocol::MessageCodec as _,
    };

    use super::{ExchangeMessageCodec, MAX_FRAME_LENGTH};

    fn profile() -> Profile {
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "Anonymous").unwrap();
        Profile::sign(&signer, 1, DateTime::from_timestamp(100, 0).unwrap(), ProfileDraft::default()).unwrap()
    }

    #[test]
    fn every_message_round_trips() -> TestResult {
        let profile = profile();
        let messages = [
            ExchangeMessage::Request {
                profile_id: profile.id().to_string(),
            },
            ExchangeMessage::Response { profile: profile.to_wire() },
            ExchangeMessage::NotFound {
                profile_id: profile.id().to_string(),
            },
        ];
        for message in messages {
            assert_eq!(ExchangeMessageCodec::decode(&ExchangeMessageCodec::encode(&message)?)?, message);
        }
        Ok(())
    }

    #[test]
    fn a_request_over_the_id_limit_is_rejected() -> TestResult {
        let over = ExchangeMessage::Request {
            profile_id: "a".repeat(MAX_PROFILE_ID_LENGTH as usize + 1),
        };
        assert!(ExchangeMessageCodec::encode(&over).is_err());
        Ok(())
    }

    #[test]
    fn the_frame_limit_is_the_profile_limit_plus_the_envelope() {
        assert_eq!(MAX_FRAME_LENGTH, Profile::MAX_ENCODED_LENGTH + 256);
    }
}
