use omnius_core_rocketpack::{RocketMessage, RocketMessageWriter};

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest<T>
where
    T: RocketMessage + Send + Sync + 'static,
{
    pub header: ApiRequestHeader,
    pub body: T,
}

impl<T> RocketMessage for ApiRequest<T>
where
    T: RocketMessage + Send + Sync + 'static,
{
    fn pack(writer: &mut RocketMessageWriter, value: &Self, depth: u32) -> RocketPackResult<()> {
        writer.put_u32(1);
        ApiRequestHeader::pack(writer, &value.header, depth + 1)?;

        writer.put_u32(2);
        T::pack(writer, &value.body, depth + 1)?;

        writer.put_u32(0);

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let mut header: Option<ApiRequestHeader> = None;
        let mut body: Option<T> = None;

        loop {
            let field_id = reader.get_u32()?;
            if field_id == 0 {
                break;
            }

            match field_id {
                1 => {
                    header = Some(ApiRequestHeader::unpack(reader, depth + 1)?);
                }
                2 => {
                    body = Some(T::unpack(reader, depth + 1)?);
                }
                _ => {}
            }
        }

        Ok(Self {
            header: header.ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat))?,
            body: body.ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat))?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequestHeader {
    request_id: String,
}

impl RocketMessage for ApiRequestHeader {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
        writer.put_u32(1);
        writer.put_str(&value.request_id);

        writer.put_u32(0);

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let mut request_id: Option<String> = None;

        loop {
            let field_id = reader.get_u32()?;
            if field_id == 0 {
                break;
            }

            #[allow(clippy::single_match)]
            match field_id {
                1 => {
                    request_id = Some(reader.get_string(1024)?);
                }
                _ => {}
            }
        }

        Ok(Self {
            request_id: request_id.ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat))?,
        })
    }
}
