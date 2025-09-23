use std::backtrace::Backtrace;

use omnius_core_base::error::OmniError;

pub struct Error {
    kind: ErrorKind,
    message: Option<String>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    backtrace: Backtrace,
}

impl OmniError for Error {
    type ErrorKind = ErrorKind;

    fn new(kind: Self::ErrorKind) -> Self {
        Self {
            kind,
            message: None,
            source: None,
            backtrace: Backtrace::capture(),
        }
    }

    fn from_error<E: Into<Box<dyn std::error::Error + Send + Sync>>>(source: E, kind: Self::ErrorKind) -> Self {
        Self {
            kind,
            message: None,
            source: Some(source.into()),
            backtrace: Backtrace::disabled(),
        }
    }

    fn kind(&self) -> &Self::ErrorKind {
        &self.kind
    }

    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }

    fn with_message<S: Into<String>>(mut self, message: S) -> Self {
        self.message = Some(message.into());
        self
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|s| &**s as &(dyn std::error::Error + 'static))
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        OmniError::fmt(self, f)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        OmniError::fmt(self, f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    Unknown,
    IoError,
    TimeError,
    SerdeError,
    DatabaseError,
    HttpClientError,
    CryptoError,
    UpnpError,
    NetworkError,
    TaskError,
    UnexpectedError,

    InvalidFormat,
    EndOfStream,
    UnsupportedType,
    Reject,
    NotFound,
    AlreadyExists,
    RateLimitExceeded,
    AlreadyConnected,
    NotConnected,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::Unknown => write!(fmt, "unknown"),
            ErrorKind::IoError => write!(fmt, "io error"),
            ErrorKind::TimeError => write!(fmt, "time conversion error"),
            ErrorKind::SerdeError => write!(fmt, "serde error"),
            ErrorKind::DatabaseError => write!(fmt, "database error"),
            ErrorKind::HttpClientError => write!(fmt, "http client error"),
            ErrorKind::CryptoError => write!(fmt, "crypto error"),
            ErrorKind::UpnpError => write!(fmt, "upnp error"),
            ErrorKind::NetworkError => write!(fmt, "network error"),
            ErrorKind::TaskError => write!(fmt, "task error"),
            ErrorKind::UnexpectedError => write!(fmt, "unexpected error"),

            ErrorKind::InvalidFormat => write!(fmt, "invalid format"),
            ErrorKind::EndOfStream => write!(fmt, "end of stream"),
            ErrorKind::UnsupportedType => write!(fmt, "unsupported type"),
            ErrorKind::Reject => write!(fmt, "reject"),
            ErrorKind::NotFound => write!(fmt, "not found"),
            ErrorKind::AlreadyExists => write!(fmt, "already exists"),
            ErrorKind::RateLimitExceeded => write!(fmt, "rate limit exceeded"),
            ErrorKind::AlreadyConnected => write!(fmt, "already connected"),
            ErrorKind::NotConnected => write!(fmt, "not_connected"),
        }
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(_: std::convert::Infallible) -> Self {
        Error::new(ErrorKind::Unknown)
    }
}

impl From<std::array::TryFromSliceError> for Error {
    fn from(e: std::array::TryFromSliceError) -> Self {
        Error::from_error(e, ErrorKind::InvalidFormat).with_message("failed to convert slice to array")
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::from_error(e, ErrorKind::IoError).with_message("io error")
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::from_error(e, ErrorKind::DatabaseError).with_message("Database operation failed")
    }
}

impl From<ed25519_dalek::pkcs8::Error> for Error {
    fn from(_: ed25519_dalek::pkcs8::Error) -> Self {
        Error::new(ErrorKind::InvalidFormat).with_message("pkcs8 error")
    }
}

impl From<ed25519_dalek::pkcs8::spki::Error> for Error {
    fn from(_: ed25519_dalek::pkcs8::spki::Error) -> Self {
        Error::new(ErrorKind::InvalidFormat).with_message("pkcs8 spki error")
    }
}

impl<T> From<nom::Err<nom::error::Error<T>>> for Error {
    fn from(e: nom::Err<nom::error::Error<T>>) -> Error {
        match e {
            nom::Err::Incomplete(_) => Error::new(ErrorKind::InvalidFormat).with_message("nom incomplete"),
            nom::Err::Error(e) => Error::new(ErrorKind::InvalidFormat).with_message(format!("nom error: {:?}", e.code)),
            nom::Err::Failure(e) => Error::new(ErrorKind::InvalidFormat).with_message(format!("nom failure: {:?}", e.code)),
        }
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Error {
        Error::from_error(e, ErrorKind::InvalidFormat).with_message("int parse error")
    }
}

impl From<std::net::AddrParseError> for Error {
    fn from(e: std::net::AddrParseError) -> Error {
        Error::from_error(e, ErrorKind::InvalidFormat).with_message("addr parse error")
    }
}

impl From<hex::FromHexError> for Error {
    fn from(e: hex::FromHexError) -> Self {
        Error::from_error(e, ErrorKind::InvalidFormat).with_message("hex decode error")
    }
}

impl From<base64::DecodeError> for Error {
    fn from(e: base64::DecodeError) -> Self {
        Error::from_error(e, ErrorKind::InvalidFormat).with_message("base64 decode error")
    }
}

impl From<omnius_core_rocketpack::Error> for Error {
    fn from(e: omnius_core_rocketpack::Error) -> Error {
        Error::from_error(e, ErrorKind::SerdeError).with_message("rocket pack error")
    }
}

impl From<omnius_core_migration::Error> for Error {
    fn from(e: omnius_core_migration::Error) -> Self {
        match e.kind() {
            omnius_core_migration::ErrorKind::Unknown => Error::from_error(e, ErrorKind::Unknown),
            omnius_core_migration::ErrorKind::IoError => Error::from_error(e, ErrorKind::IoError),
            omnius_core_migration::ErrorKind::DatabaseError => Error::from_error(e, ErrorKind::TimeError),
            omnius_core_migration::ErrorKind::InvalidFormat => Error::from_error(e, ErrorKind::InvalidFormat),
        }
    }
}

impl From<omnius_core_omnikit::Error> for Error {
    fn from(e: omnius_core_omnikit::Error) -> Self {
        match e.kind() {
            omnius_core_omnikit::ErrorKind::Unknown => Error::from_error(e, ErrorKind::Unknown),
            omnius_core_omnikit::ErrorKind::SerdeError => Error::from_error(e, ErrorKind::SerdeError),
            omnius_core_omnikit::ErrorKind::IoError => Error::from_error(e, ErrorKind::IoError),
            omnius_core_omnikit::ErrorKind::UnexpectedError => Error::from_error(e, ErrorKind::UnexpectedError),

            omnius_core_omnikit::ErrorKind::InvalidFormat => Error::from_error(e, ErrorKind::InvalidFormat),
            omnius_core_omnikit::ErrorKind::EndOfStream => Error::from_error(e, ErrorKind::EndOfStream),
            omnius_core_omnikit::ErrorKind::UnsupportedType => Error::from_error(e, ErrorKind::UnsupportedType),
            omnius_core_omnikit::ErrorKind::AlreadyConnected => Error::from_error(e, ErrorKind::AlreadyConnected),
            omnius_core_omnikit::ErrorKind::NotConnected => Error::from_error(e, ErrorKind::NotConnected),
        }
    }
}

impl From<std::num::TryFromIntError> for Error {
    fn from(e: std::num::TryFromIntError) -> Self {
        Error::from_error(e, ErrorKind::InvalidFormat).with_message("Integer conversion error")
    }
}

impl From<rupnp::Error> for Error {
    fn from(e: rupnp::Error) -> Self {
        Error::from_error(e, ErrorKind::UpnpError).with_message("UPnP operation failed")
    }
}

impl From<local_ip_address::Error> for Error {
    fn from(e: local_ip_address::Error) -> Self {
        Error::from_error(e, ErrorKind::NetworkError).with_message("Failed to get local IP address")
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::from_error(e, ErrorKind::HttpClientError).with_message("HTTP request failed")
    }
}

impl From<rocksdb::Error> for Error {
    fn from(e: rocksdb::Error) -> Self {
        Error::from_error(e, ErrorKind::DatabaseError).with_message("RocksDB operation failed")
    }
}

impl From<fast_socks5::SocksError> for Error {
    fn from(e: fast_socks5::SocksError) -> Self {
        Error::from_error(e, ErrorKind::NetworkError).with_message("Socks operation failed")
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Self {
        Error::from_error(e, ErrorKind::TaskError).with_message("join failed")
    }
}
