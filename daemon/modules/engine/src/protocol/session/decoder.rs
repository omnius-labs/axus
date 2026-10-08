use crate::{
    core::session::message::{SessionVersion, V1RequestType, V1ResultType},
    prelude::*,
};
type DecodeResult<T> = std::result::Result<T, RocketPackDecoderError>;
pub(super) enum ScalarPolicy {
    Version,
    Request,
    Result,
}
impl ScalarPolicy {
    fn validate(&self, value: u32) -> DecodeResult<()> {
        match self {
            Self::Version if SessionVersion::from_repr(value).is_some() => Ok(()),
            Self::Request if V1RequestType::from_repr(value).is_some() => Ok(()),
            Self::Result if V1ResultType::from_repr(value).is_some() => Ok(()),
            Self::Version => Err(RocketPackDecoderError::Other("invalid session version")),
            Self::Request => Err(RocketPackDecoderError::Other("invalid request type")),
            Self::Result => Err(RocketPackDecoderError::Other("invalid result type")),
        }
    }
}
// Validate every occurrence, including fields overwritten by a later duplicate.
pub(super) struct ScalarDecoder<'a, D> {
    inner: &'a mut D,
    policy: ScalarPolicy,
}
impl<'a, D: RocketPackDecoder> ScalarDecoder<'a, D> {
    pub(super) fn new(inner: &'a mut D, policy: ScalarPolicy) -> Self {
        Self { inner, policy }
    }
}
impl<D: RocketPackDecoder> RocketPackDecoder for ScalarDecoder<'_, D> {
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
    fn position(&self) -> usize {
        self.inner.position()
    }
    fn current_type(&self) -> DecodeResult<FieldType> {
        self.inner.current_type()
    }
    fn read_u32(&mut self) -> DecodeResult<u32> {
        let value = self.inner.read_u32()?;
        self.policy.validate(value)?;
        Ok(value)
    }
    fn read_bool(&mut self) -> DecodeResult<bool> {
        self.inner.read_bool()
    }
    fn read_u8(&mut self) -> DecodeResult<u8> {
        self.inner.read_u8()
    }
    fn read_u16(&mut self) -> DecodeResult<u16> {
        self.inner.read_u16()
    }
    fn read_u64(&mut self) -> DecodeResult<u64> {
        self.inner.read_u64()
    }
    fn read_i8(&mut self) -> DecodeResult<i8> {
        self.inner.read_i8()
    }
    fn read_i16(&mut self) -> DecodeResult<i16> {
        self.inner.read_i16()
    }
    fn read_i32(&mut self) -> DecodeResult<i32> {
        self.inner.read_i32()
    }
    fn read_i64(&mut self) -> DecodeResult<i64> {
        self.inner.read_i64()
    }
    fn read_f32(&mut self) -> DecodeResult<f32> {
        self.inner.read_f32()
    }
    fn read_f64(&mut self) -> DecodeResult<f64> {
        self.inner.read_f64()
    }
    fn read_bytes(&mut self) -> DecodeResult<&[u8]> {
        self.inner.read_bytes()
    }
    fn read_bytes_vec(&mut self) -> DecodeResult<Vec<u8>> {
        self.inner.read_bytes_vec()
    }
    fn read_string(&mut self) -> DecodeResult<String> {
        self.inner.read_string()
    }
    fn read_array(&mut self) -> DecodeResult<u64> {
        self.inner.read_array()
    }
    fn read_map(&mut self) -> DecodeResult<u64> {
        self.inner.read_map()
    }
    fn read_null(&mut self) -> DecodeResult<()> {
        self.inner.read_null()
    }
    fn skip_field(&mut self) -> DecodeResult<()> {
        self.inner.skip_field()
    }
    fn read_bytes_bounded(&mut self, context: &'static str, min: u64, max: u64) -> DecodeResult<Vec<u8>> {
        self.inner.read_bytes_bounded(context, min, max)
    }
    fn read_string_bounded(&mut self, context: &'static str, min: u64, max: u64) -> DecodeResult<String> {
        self.inner.read_string_bounded(context, min, max)
    }
    fn read_struct<T: RocketPackStruct>(&mut self) -> DecodeResult<T> {
        T::unpack(self)
    }
}
