use std::backtrace::Backtrace;

use omnius_core_base::error::{OmniError, OmniErrorBuilder};

pub struct Error {
    kind: ErrorKind,
    message: Option<String>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    backtrace: Option<Backtrace>,
}

pub struct ErrorBuilder {
    inner: Error,
}

impl Error {
    pub fn builder() -> ErrorBuilder {
        ErrorBuilder {
            inner: Self {
                kind: ErrorKind::Unknown,
                message: None,
                source: None,
                backtrace: None,
            },
        }
    }
}

impl OmniError for Error {
    type ErrorKind = ErrorKind;

    fn kind(&self) -> &Self::ErrorKind {
        &self.kind
    }

    fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.backtrace.as_ref()
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

impl OmniErrorBuilder<Error> for ErrorBuilder {
    type ErrorKind = ErrorKind;

    fn kind(mut self, kind: Self::ErrorKind) -> Self {
        self.inner.kind = kind;
        self
    }

    fn message<S: Into<String>>(mut self, message: S) -> Self {
        self.inner.message = Some(message.into());
        self
    }

    fn source<E: Into<Box<dyn std::error::Error + Send + Sync>>>(mut self, source: E) -> Self {
        self.inner.source = Some(source.into());
        self
    }

    fn backtrace(mut self) -> Self {
        self.inner.backtrace = Some(Backtrace::capture());
        self
    }

    fn build(self) -> Error {
        self.inner
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
    UnsupportedVersion,
    UnsupportedType,
    Reject,
    NotFound,
    AlreadyConnected,
    AlreadyExists,
    RateLimitExceeded,
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
            ErrorKind::UnsupportedVersion => write!(fmt, "unsupported version"),
            ErrorKind::UnsupportedType => write!(fmt, "unsupported type"),
            ErrorKind::Reject => write!(fmt, "reject"),
            ErrorKind::NotFound => write!(fmt, "not found"),
            ErrorKind::AlreadyConnected => write!(fmt, "already connected"),
            ErrorKind::AlreadyExists => write!(fmt, "already Exists"),
            ErrorKind::RateLimitExceeded => write!(fmt, "rate limit exceeded"),
        }
    }
}

impl From<std::convert::Infallible> for Error {
    fn from(_: std::convert::Infallible) -> Self {
        Error::builder().build()
    }
}

impl From<std::array::TryFromSliceError> for Error {
    fn from(e: std::array::TryFromSliceError) -> Self {
        Error::builder()
            .kind(ErrorKind::InvalidFormat)
            .message("failed to convert slice to array")
            .source(e)
            .build()
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::builder().kind(ErrorKind::IoError).message("io error").source(e).build()
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::builder()
            .kind(ErrorKind::DatabaseError)
            .message("Database operation failed")
            .source(e)
            .build()
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Error {
        Error::builder()
            .kind(ErrorKind::InvalidFormat)
            .message("int parse error")
            .source(e)
            .build()
    }
}

impl From<std::net::AddrParseError> for Error {
    fn from(e: std::net::AddrParseError) -> Error {
        Error::builder()
            .kind(ErrorKind::InvalidFormat)
            .message("addr parse error")
            .source(e)
            .build()
    }
}

impl From<hex::FromHexError> for Error {
    fn from(e: hex::FromHexError) -> Self {
        Error::builder()
            .kind(ErrorKind::InvalidFormat)
            .message("hex decode error")
            .source(e)
            .build()
    }
}

impl From<base64::DecodeError> for Error {
    fn from(e: base64::DecodeError) -> Self {
        Error::builder()
            .kind(ErrorKind::InvalidFormat)
            .message("base64 decode error")
            .source(e)
            .build()
    }
}

impl From<std::num::TryFromIntError> for Error {
    fn from(e: std::num::TryFromIntError) -> Self {
        Error::builder()
            .kind(ErrorKind::InvalidFormat)
            .message("integer conversion error")
            .source(e)
            .build()
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::builder()
            .kind(ErrorKind::HttpClientError)
            .message("http request failed")
            .source(e)
            .build()
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Error {
        Error::builder()
            .kind(ErrorKind::SerdeError)
            .message("failed to parse toml file")
            .source(e)
            .build()
    }
}

impl From<omnius_core_rocketpack::Error> for Error {
    fn from(e: omnius_core_rocketpack::Error) -> Error {
        Error::builder()
            .kind(ErrorKind::SerdeError)
            .message("rocket pack error")
            .source(e)
            .build()
    }
}

impl From<omnius_core_omnikit::Error> for Error {
    fn from(e: omnius_core_omnikit::Error) -> Self {
        match e.kind() {
            omnius_core_omnikit::ErrorKind::Unknown => Error::builder().kind(ErrorKind::Unknown).source(e).build(),
            omnius_core_omnikit::ErrorKind::SerdeError => Error::builder().kind(ErrorKind::SerdeError).source(e).build(),
            omnius_core_omnikit::ErrorKind::IoError => Error::builder().kind(ErrorKind::IoError).source(e).build(),
            omnius_core_omnikit::ErrorKind::UnexpectedError => Error::builder().kind(ErrorKind::UnexpectedError).source(e).build(),
            omnius_core_omnikit::ErrorKind::InvalidFormat => Error::builder().kind(ErrorKind::InvalidFormat).source(e).build(),
            omnius_core_omnikit::ErrorKind::EndOfStream => Error::builder().kind(ErrorKind::EndOfStream).source(e).build(),
            omnius_core_omnikit::ErrorKind::UnsupportedVersion => Error::builder().kind(ErrorKind::UnsupportedVersion).source(e).build(),
            omnius_core_omnikit::ErrorKind::UnsupportedType => Error::builder().kind(ErrorKind::UnsupportedType).source(e).build(),
        }
    }
}

impl From<omnius_axus_engine::Error> for Error {
    fn from(e: omnius_axus_engine::Error) -> Self {
        match e.kind() {
            omnius_axus_engine::ErrorKind::Unknown => Error::builder().kind(ErrorKind::Unknown).source(e).build(),
            omnius_axus_engine::ErrorKind::IoError => Error::builder().kind(ErrorKind::IoError).source(e).build(),
            omnius_axus_engine::ErrorKind::TimeError => Error::builder().kind(ErrorKind::TimeError).source(e).build(),
            omnius_axus_engine::ErrorKind::SerdeError => Error::builder().kind(ErrorKind::SerdeError).source(e).build(),
            omnius_axus_engine::ErrorKind::DatabaseError => Error::builder().kind(ErrorKind::DatabaseError).source(e).build(),
            omnius_axus_engine::ErrorKind::HttpClientError => Error::builder().kind(ErrorKind::HttpClientError).source(e).build(),
            omnius_axus_engine::ErrorKind::CryptoError => Error::builder().kind(ErrorKind::CryptoError).source(e).build(),
            omnius_axus_engine::ErrorKind::UpnpError => Error::builder().kind(ErrorKind::UpnpError).source(e).build(),
            omnius_axus_engine::ErrorKind::NetworkError => Error::builder().kind(ErrorKind::NetworkError).source(e).build(),
            omnius_axus_engine::ErrorKind::TaskError => Error::builder().kind(ErrorKind::TaskError).source(e).build(),
            omnius_axus_engine::ErrorKind::UnexpectedError => Error::builder().kind(ErrorKind::UnexpectedError).source(e).build(),
            omnius_axus_engine::ErrorKind::InvalidFormat => Error::builder().kind(ErrorKind::InvalidFormat).source(e).build(),
            omnius_axus_engine::ErrorKind::EndOfStream => Error::builder().kind(ErrorKind::EndOfStream).source(e).build(),
            omnius_axus_engine::ErrorKind::UnsupportedVersion => Error::builder().kind(ErrorKind::UnsupportedVersion).source(e).build(),
            omnius_axus_engine::ErrorKind::UnsupportedType => Error::builder().kind(ErrorKind::UnsupportedType).source(e).build(),
            omnius_axus_engine::ErrorKind::Reject => Error::builder().kind(ErrorKind::Reject).source(e).build(),
            omnius_axus_engine::ErrorKind::NotFound => Error::builder().kind(ErrorKind::NotFound).source(e).build(),
            omnius_axus_engine::ErrorKind::AlreadyExists => Error::builder().kind(ErrorKind::AlreadyExists).source(e).build(),
            omnius_axus_engine::ErrorKind::RateLimitExceeded => Error::builder().kind(ErrorKind::RateLimitExceeded).source(e).build(),
        }
    }
}
