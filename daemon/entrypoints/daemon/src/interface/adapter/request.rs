use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest<T>
where
    T: RocketPackStruct + Send + Sync + 'static,
{
    pub header: ApiRequestHeader,
    pub body: T,
}

impl<T> RocketPackStruct for ApiRequest<T>
where
    T: RocketPackStruct + Send + Sync + 'static,
{
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        encoder.write_map(2)?;

        encoder.write_u64(0)?;
        encoder.write_struct(&value.header)?;

        encoder.write_u64(1)?;
        encoder.write_struct(&value.body)?;

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut header: Option<ApiRequestHeader> = None;
        let mut body: Option<T> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => header = Some(decoder.read_struct::<ApiRequestHeader>()?),
                1 => body = Some(decoder.read_struct::<T>()?),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            header: header.ok_or(RocketPackDecoderError::Other("missing field: header"))?,
            body: body.ok_or(RocketPackDecoderError::Other("missing field: body"))?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApiRequestHeader {
    request_id: Option<String>,
}

impl RocketPackStruct for ApiRequestHeader {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        let mut count = 0;
        if value.request_id.is_some() {
            count += 1;
        }

        encoder.write_map(count)?;

        if let Some(request_id) = &value.request_id {
            encoder.write_u64(0)?;
            encoder.write_string(request_id)?;
        }

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut request_id: Option<String> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => request_id = Some(decoder.read_string()?),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self { request_id })
    }
}
