use std::str::FromStr;

use crate::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub kind: ApiErrorKind,
    pub message: Option<String>,
}

impl ApiError {
    pub fn new(kind: ApiErrorKind) -> Self {
        Self { kind, message: None }
    }

    #[allow(unused)]
    pub fn with_message<S: Into<String>>(mut self, message: S) -> Self {
        self.message = Some(message.into());
        self
    }
}

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, strum::EnumString, strum::AsRefStr, strum::Display)]
pub enum ApiErrorKind {
    /// Unknown error.
    #[default]
    #[strum(serialize = "unknown")]
    Unknown,

    /// InvalidInput indicates client specified an invalid input.
    #[strum(serialize = "invalid_input")]
    InvalidInput,

    /// DeadlineExceeded means operation expired before completion.
    #[strum(serialize = "deadline_exceeded")]
    DeadlineExceeded,

    /// NotFound means some requested entity (e.g., file or directory) was not found.
    #[strum(serialize = "not_found")]
    NotFound,

    /// AlreadyExists means an attempt to create an entity failed because one already exists.
    #[strum(serialize = "already_exists")]
    AlreadyExists,

    /// OutOfRange means operation was attempted past the valid range.
    #[strum(serialize = "out_of_range")]
    OutOfRange,

    /// Unimplemented indicates operation is not implemented or not supported/enabled in this service.
    #[strum(serialize = "unimplemented")]
    Unimplemented,

    /// Unauthenticated indicates the request does not have valid authentication credentials for the operation.
    #[strum(serialize = "unauthenticated")]
    Unauthenticated,

    /// Internal errors. Means some invariants expected by underlying system has been broken. If you see one of these errors,
    #[strum(serialize = "internal")]
    Internal,
}

impl RocketPackStruct for ApiError {
    fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
        let mut count = 1;
        if value.message.is_some() {
            count += 1;
        }

        encoder.write_map(count)?;

        encoder.write_u64(0)?;
        encoder.write_string(value.kind.as_ref())?;

        if let Some(message) = &value.message {
            encoder.write_u64(1)?;
            encoder.write_string(message)?;
        }

        Ok(())
    }

    fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
    where
        Self: Sized,
    {
        let mut kind: Option<ApiErrorKind> = None;
        let mut message: Option<String> = None;

        let count = decoder.read_map()?;

        for _ in 0..count {
            match decoder.read_u64()? {
                0 => kind = Some(ApiErrorKind::from_str(&decoder.read_string()?).map_err(|_| RocketPackDecoderError::Other("parse error"))?),
                1 => message = Some(decoder.read_string()?),
                _ => decoder.skip_field()?,
            }
        }

        Ok(Self {
            kind: kind.ok_or(RocketPackDecoderError::Other("missing field: kind"))?,
            message,
        })
    }
}
