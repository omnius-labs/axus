use std::str::FromStr;

use omnius_core_base::ensure_err;
use omnius_core_rocketpack::{RocketMessage, RocketMessageWriter};

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
    pub fn with_message(mut self, message: &str) {
        self.message = Some(message.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiErrorKind {
    /// Unknown error.
    Unknown,

    /// InvalidArgument indicates client specified an invalid argument.
    InvalidArgument,

    /// DeadlineExceeded means operation expired before completion.
    DeadlineExceeded,

    /// NotFound means some requested entity (e.g., file or directory) was not found.
    NotFound,

    /// AlreadyExists means an attempt to create an entity failed because one already exists.
    AlreadyExists,

    /// OutOfRange means operation was attempted past the valid range.
    OutOfRange,

    /// Unimplemented indicates operation is not implemented or not supported/enabled in this service.
    Unimplemented,

    /// Unauthenticated indicates the request does not have valid authentication credentials for the operation.
    Unauthenticated,

    /// Internal errors. Means some invariants expected by underlying system has been broken. If you see one of these errors,
    Internal,
}

impl std::fmt::Display for ApiErrorKind {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiErrorKind::Unknown => write!(fmt, "unknown"),
            ApiErrorKind::InvalidArgument => write!(fmt, "invalid_argument"),
            ApiErrorKind::DeadlineExceeded => write!(fmt, "deadline_exceeded"),
            ApiErrorKind::NotFound => write!(fmt, "not_found"),
            ApiErrorKind::AlreadyExists => write!(fmt, "already_exists"),
            ApiErrorKind::OutOfRange => write!(fmt, "out_of_range"),
            ApiErrorKind::Unimplemented => write!(fmt, "unimplemented"),
            ApiErrorKind::Unauthenticated => write!(fmt, "unauthenticated"),
            ApiErrorKind::Internal => write!(fmt, "internal"),
        }
    }
}

impl std::str::FromStr for ApiErrorKind {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "unknown" => ApiErrorKind::Unknown,
            "invalid_argument" => ApiErrorKind::InvalidArgument,
            "deadline_exceeded" => ApiErrorKind::DeadlineExceeded,
            "not_found" => ApiErrorKind::NotFound,
            "already_exists" => ApiErrorKind::AlreadyExists,
            "out_of_range" => ApiErrorKind::OutOfRange,
            "unimplemented" => ApiErrorKind::Unimplemented,
            "unauthenticated" => ApiErrorKind::Unauthenticated,
            "internal" => ApiErrorKind::Internal,
            _ => ApiErrorKind::Unknown,
        })
    }
}

impl ApiError {
    pub const MAX_KIND_LENGTH: usize = 256;
    pub const MAX_MESSAGE_LENGTH: usize = 1024;
}

impl RocketMessage for ApiError {
    fn pack(writer: &mut RocketMessageWriter, value: &Self, _depth: u32) -> RocketPackResult<()> {
        let get_too_large_err = || RocketPackError::new(RocketPackErrorKind::TooLarge).with_message("len too large");

        writer.put_u32(1);
        writer.put_str(value.kind.to_string().as_str());

        if let Some(message) = &value.message {
            writer.put_u32(2);
            let len = message.len();
            ensure_err!(len > Self::MAX_MESSAGE_LENGTH, get_too_large_err);
            writer.put_str(message.as_str());
        }

        writer.put_u32(0);

        Ok(())
    }

    fn unpack(reader: &mut RocketMessageReader, _depth: u32) -> RocketPackResult<Self>
    where
        Self: Sized,
    {
        let mut kind: Option<ApiErrorKind> = None;
        let mut message: Option<String> = None;

        loop {
            let field_id = reader.get_u32()?;
            if field_id == 0 {
                break;
            }

            #[allow(clippy::single_match)]
            match field_id {
                1 => {
                    kind = Some(ApiErrorKind::from_str(reader.get_string(2014)?.as_str())?);
                }
                2 => message = Some(reader.get_string(Self::MAX_KIND_LENGTH)?),
                _ => {}
            }
        }

        Ok(Self {
            kind: kind.ok_or_else(|| RocketPackError::new(RocketPackErrorKind::InvalidFormat))?,
            message,
        })
    }
}
