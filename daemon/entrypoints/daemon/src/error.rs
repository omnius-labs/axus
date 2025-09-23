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
            ErrorKind::AlreadyExists => write!(fmt, "already Exists"),
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

impl From<std::num::TryFromIntError> for Error {
    fn from(e: std::num::TryFromIntError) -> Self {
        Error::from_error(e, ErrorKind::InvalidFormat).with_message("integer conversion error")
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::from_error(e, ErrorKind::HttpClientError).with_message("http request failed")
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Error {
        Error::from_error(e, ErrorKind::SerdeError).with_message("failed to parse toml file")
    }
}

impl From<omnius_core_rocketpack::Error> for Error {
    fn from(e: omnius_core_rocketpack::Error) -> Error {
        Error::from_error(e, ErrorKind::SerdeError).with_message("rocket pack error")
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

impl From<omnius_axus_engine::Error> for Error {
    fn from(e: omnius_axus_engine::Error) -> Self {
        match e.kind() {
            omnius_axus_engine::ErrorKind::Unknown => Error::from_error(e, ErrorKind::Unknown),
            omnius_axus_engine::ErrorKind::IoError => Error::from_error(e, ErrorKind::IoError),
            omnius_axus_engine::ErrorKind::TimeError => Error::from_error(e, ErrorKind::TimeError),
            omnius_axus_engine::ErrorKind::SerdeError => Error::from_error(e, ErrorKind::SerdeError),
            omnius_axus_engine::ErrorKind::DatabaseError => Error::from_error(e, ErrorKind::DatabaseError),
            omnius_axus_engine::ErrorKind::HttpClientError => Error::from_error(e, ErrorKind::HttpClientError),
            omnius_axus_engine::ErrorKind::CryptoError => Error::from_error(e, ErrorKind::CryptoError),
            omnius_axus_engine::ErrorKind::UpnpError => Error::from_error(e, ErrorKind::UpnpError),
            omnius_axus_engine::ErrorKind::NetworkError => Error::from_error(e, ErrorKind::NetworkError),
            omnius_axus_engine::ErrorKind::TaskError => Error::from_error(e, ErrorKind::TaskError),
            omnius_axus_engine::ErrorKind::UnexpectedError => Error::from_error(e, ErrorKind::UnexpectedError),
            omnius_axus_engine::ErrorKind::InvalidFormat => Error::from_error(e, ErrorKind::InvalidFormat),
            omnius_axus_engine::ErrorKind::EndOfStream => Error::from_error(e, ErrorKind::EndOfStream),
            omnius_axus_engine::ErrorKind::UnsupportedType => Error::from_error(e, ErrorKind::UnsupportedType),
            omnius_axus_engine::ErrorKind::Reject => Error::from_error(e, ErrorKind::Reject),
            omnius_axus_engine::ErrorKind::NotFound => Error::from_error(e, ErrorKind::NotFound),
            omnius_axus_engine::ErrorKind::AlreadyExists => Error::from_error(e, ErrorKind::AlreadyExists),
            omnius_axus_engine::ErrorKind::RateLimitExceeded => Error::from_error(e, ErrorKind::RateLimitExceeded),
            omnius_axus_engine::ErrorKind::AlreadyConnected => Error::from_error(e, ErrorKind::AlreadyConnected),
            omnius_axus_engine::ErrorKind::NotConnected => Error::from_error(e, ErrorKind::NotConnected),
        }
    }
}
