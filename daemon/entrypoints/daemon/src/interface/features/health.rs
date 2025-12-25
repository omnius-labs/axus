use omnius_core_rocketpack::EmptyRocketMessage;

use crate::{
    interface::adapter::{ApiRequest, ApiResult},
    prelude::*,
    shared::AppState,
};

pub async fn health(state: &AppState, _: ApiRequest<EmptyRocketMessage>) -> ApiResult<HealthResult> {
    ApiResult::Ok(HealthResult {
        git_tag: state.info.git_tag.to_string(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HealthResult {
    pub git_tag: String,
}

impl RocketPackStruct for HealthResult {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        encoder.write_u64(0)?;
        encoder.write_string(&value.git_tag)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut git_tag: Option<String> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => git_tag = Some(decoder.read_string()?),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            git_tag: git_tag.ok_or(RocketPackDecoderError::Other("missing field: git_tag"))?,
        })
    }
}
