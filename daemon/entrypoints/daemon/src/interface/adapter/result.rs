use omnius_core_rocketpack::{RocketMessage, RocketMessageWriter};

use crate::{interface::adapter::error::ApiError, prelude::*};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiResult<T>
where
    T: RocketMessage + Send + Sync + 'static,
{
    Ok(T),
    Err(ApiError),
}

impl<T> RocketMessage for ApiResult<T>
where
    T: RocketMessage + Send + Sync + 'static,
{
    fn pack(writer: &mut RocketMessageWriter, value: &Self, depth: u32) -> RocketPackResult<()> {
        match value {
            ApiResult::Ok(dir) => {
                writer.put_u32(1);
                T::pack(writer, dir, depth + 1)?;
            }
            ApiResult::Err(file) => {
                writer.put_u32(2);
                ApiError::pack(writer, file, depth + 1)?;
            }
        }

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let typ = reader.get_u32()?;

        match typ {
            1 => Ok(Self::Ok(T::unpack(reader, depth + 1)?)),
            2 => Ok(Self::Err(ApiError::unpack(reader, depth + 1)?)),
            _ => Err(RocketPackError::new(RocketPackErrorKind::InvalidFormat)),
        }
    }
}
