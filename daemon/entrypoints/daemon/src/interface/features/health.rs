use omnius_core_rocketpack::{EmptyRocketMessage, RocketMessage, RocketMessageReader, RocketMessageWriter};

use crate::{prelude::*, shared::AppState};

pub async fn health(state: &AppState, _: EmptyRocketMessage) -> HealthOutput {
    let res = HealthOutput {
        git_tag: state.info.git_tag.to_string(),
    };
    res
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthOutput {
    pub git_tag: String,
}

impl HealthOutput {
    pub const MAX_GIH_TAG_LENGTH: usize = 256;
}

impl RocketMessage for HealthOutput {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
        writer.put_u32(1);
        writer.put_str(&value.git_tag);

        writer.put_u32(0);

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let mut git_tag: Option<String> = None;

        loop {
            let field_id = reader.get_u32()?;
            if field_id == 0 {
                break;
            }

            #[allow(clippy::single_match)]
            match field_id {
                1 => {
                    git_tag = Some(reader.get_string(Self::MAX_GIH_TAG_LENGTH)?);
                }
                _ => {}
            }
        }

        Ok(Self {
            git_tag: git_tag.ok_or_else(|| RocketPackError::builder().kind(RocketPackErrorKind::InvalidFormat).build())?,
        })
    }
}
