use crate::{interface::adapter::error::ApiError, prelude::*};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiResult<T>
where
    T: RocketPackStruct + Send + Sync + 'static,
{
    Ok(T),
    Err(ApiError),
}

impl<T> RocketPackStruct for ApiResult<T>
where
    T: RocketPackStruct + Send + Sync + 'static,
{
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(1)?;

        match value {
            Self::Ok(v) => {
                encoder.write_u64(0)?;

                encoder.write_map(1)?;
                encoder.write_u64(0)?;
                encoder.write_struct(v)?;
            }
            Self::Err(e) => {
                encoder.write_u64(1)?;

                encoder.write_map(1)?;
                encoder.write_u64(0)?;
                encoder.write_struct(e)?;
            }
        }

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut result = Self::Err(ApiError::new(super::ApiErrorKind::Unknown));

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => {
                    let count = decoder.read_u64()?;
                    for _ in 0..count {
                        match decoder.read_u64()? {
                            0 => result = Self::Ok(decoder.read_struct()?),
                            _ => decoder.skip_field()?,
                        }
                    }
                }
                1 => {
                    let count = decoder.read_u64()?;
                    for _ in 0..count {
                        match decoder.read_u64()? {
                            0 => result = Self::Err(decoder.read_struct()?),
                            _ => decoder.skip_field()?,
                        }
                    }
                }
                _ => decoder.skip_field()?,
            }
        }

        Ok(result)
    }
}
