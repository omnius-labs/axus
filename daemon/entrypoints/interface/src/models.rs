#![allow(unused_qualifications)]

use http::HeaderValue;
use validator::Validate;

#[cfg(feature = "server")]
use crate::header;
use crate::{models, types::*};

#[allow(dead_code)]
fn from_validation_error(e: validator::ValidationError) -> validator::ValidationErrors {
    let mut errs = validator::ValidationErrors::new();
    errs.add("na", e);
    errs
}

#[allow(dead_code)]
pub fn check_xss_string(v: &str) -> std::result::Result<(), validator::ValidationError> {
    if ammonia::is_html(v) {
        std::result::Result::Err(validator::ValidationError::new("xss detected"))
    } else {
        std::result::Result::Ok(())
    }
}

#[allow(dead_code)]
pub fn check_xss_vec_string(v: &[String]) -> std::result::Result<(), validator::ValidationError> {
    if v.iter().any(|i| ammonia::is_html(i)) {
        std::result::Result::Err(validator::ValidationError::new("xss detected"))
    } else {
        std::result::Result::Ok(())
    }
}

#[allow(dead_code)]
pub fn check_xss_map_string(v: &std::collections::HashMap<String, String>) -> std::result::Result<(), validator::ValidationError> {
    if v.keys().any(|k| ammonia::is_html(k)) || v.values().any(|v| ammonia::is_html(v)) {
        std::result::Result::Err(validator::ValidationError::new("xss detected"))
    } else {
        std::result::Result::Ok(())
    }
}

#[allow(dead_code)]
pub fn check_xss_map_nested<T>(v: &std::collections::HashMap<String, T>) -> std::result::Result<(), validator::ValidationError>
where
    T: validator::Validate,
{
    if v.keys().any(|k| ammonia::is_html(k)) || v.values().any(|v| v.validate().is_err()) {
        std::result::Result::Err(validator::ValidationError::new("xss detected"))
    } else {
        std::result::Result::Ok(())
    }
}

#[allow(dead_code)]
pub fn check_xss_map<T>(v: &std::collections::HashMap<String, T>) -> std::result::Result<(), validator::ValidationError> {
    if v.keys().any(|k| ammonia::is_html(k)) {
        std::result::Result::Err(validator::ValidationError::new("xss detected"))
    } else {
        std::result::Result::Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct CancelPublishPathParams {
    #[validate(length(min = 1))]
    pub publish_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct CommitPublishPathParams {
    #[validate(length(min = 1))]
    pub publish_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct GetPublishPathParams {
    #[validate(length(min = 1))]
    pub publish_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct ListPublishesQueryParams {
    #[serde(rename = "limit")]
    #[validate(range(min = 1u64, max = 1000u64))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(rename = "offset")]
    #[validate(range(min = 0u64))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct RemovePublishPathParams {
    #[validate(length(min = 1))]
    pub publish_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct WritePublishContentPathParams {
    #[validate(length(min = 1))]
    pub publish_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct WritePublishContentQueryParams {
    #[serde(rename = "offset")]
    #[validate(range(min = 0u64))]
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct CancelSubscribePathParams {
    #[validate(length(min = 1))]
    pub subscribe_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct GetSubscribePathParams {
    #[validate(length(min = 1))]
    pub subscribe_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct ListSubscribesQueryParams {
    #[serde(rename = "limit")]
    #[validate(range(min = 1u64, max = 1000u64))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(rename = "offset")]
    #[validate(range(min = 0u64))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct ReadSubscribeContentHeaderParams {
    pub range: Option<String>,
    pub if_none_match: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct ReadSubscribeContentPathParams {
    #[validate(length(min = 1))]
    pub subscribe_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct RemoveSubscribePathParams {
    #[validate(length(min = 1))]
    pub subscribe_id: String,
}

/// Enumeration of values.
/// Since this enum's variants do not hold data, we can easily define them as `#[repr(C)]`
/// which helps with FFI.
#[allow(non_camel_case_types, clippy::large_enum_variant)]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "conversion", derive(frunk_enum_derive::LabelledGenericEnum))]
pub enum ErrorKind {
    #[serde(rename = "InternalServerError")]
    InternalServerError,
    #[serde(rename = "InvalidRequest")]
    InvalidRequest,
    #[serde(rename = "NotFound")]
    NotFound,
    #[serde(rename = "Conflict")]
    Conflict,
    #[serde(rename = "Gone")]
    Gone,
    #[serde(rename = "PayloadTooLarge")]
    PayloadTooLarge,
    #[serde(rename = "InsufficientStorage")]
    InsufficientStorage,
}

impl validator::Validate for ErrorKind {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        std::result::Result::Ok(())
    }
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ErrorKind::InternalServerError => write!(f, "InternalServerError"),
            ErrorKind::InvalidRequest => write!(f, "InvalidRequest"),
            ErrorKind::NotFound => write!(f, "NotFound"),
            ErrorKind::Conflict => write!(f, "Conflict"),
            ErrorKind::Gone => write!(f, "Gone"),
            ErrorKind::PayloadTooLarge => write!(f, "PayloadTooLarge"),
            ErrorKind::InsufficientStorage => write!(f, "InsufficientStorage"),
        }
    }
}

impl std::str::FromStr for ErrorKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "InternalServerError" => std::result::Result::Ok(ErrorKind::InternalServerError),
            "InvalidRequest" => std::result::Result::Ok(ErrorKind::InvalidRequest),
            "NotFound" => std::result::Result::Ok(ErrorKind::NotFound),
            "Conflict" => std::result::Result::Ok(ErrorKind::Conflict),
            "Gone" => std::result::Result::Ok(ErrorKind::Gone),
            "PayloadTooLarge" => std::result::Result::Ok(ErrorKind::PayloadTooLarge),
            "InsufficientStorage" => std::result::Result::Ok(ErrorKind::InsufficientStorage),
            _ => std::result::Result::Err(format!(r#"Value not valid: {s}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct ErrorResponse {
    #[serde(rename = "kind")]
    #[validate(nested)]
    pub kind: models::ErrorKind,

    #[serde(rename = "message")]
    #[validate(custom(function = "check_xss_string"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Optional request identifier for server logs.
    #[serde(rename = "requestId")]
    #[validate(custom(function = "check_xss_string"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ErrorResponse {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(kind: models::ErrorKind) -> ErrorResponse {
        ErrorResponse {
            kind,
            message: None,
            request_id: None,
        }
    }
}

/// Converts the ErrorResponse value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping kind in query parameter serialization
            self.message.as_ref().map(|message| ["message".to_string(), message.to_string()].join(",")),
            self.request_id.as_ref().map(|request_id| ["requestId".to_string(), request_id.to_string()].join(",")),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a ErrorResponse value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for ErrorResponse {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub kind: Vec<models::ErrorKind>,
            pub message: Vec<String>,
            pub request_id: Vec<String>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing ErrorResponse".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "kind" => intermediate_rep
                        .kind
                        .push(<models::ErrorKind as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "message" => intermediate_rep.message.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "requestId" => intermediate_rep.request_id.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing ErrorResponse".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(ErrorResponse {
            kind: intermediate_rep.kind.into_iter().next().ok_or_else(|| "kind missing in ErrorResponse".to_string())?,
            message: intermediate_rep.message.into_iter().next(),
            request_id: intermediate_rep.request_id.into_iter().next(),
        })
    }
}

// Methods for converting between header::IntoHeaderValue<ErrorResponse> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<ErrorResponse>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<ErrorResponse>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for ErrorResponse - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<ErrorResponse> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <ErrorResponse as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into ErrorResponse - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct HealthResponse {
    #[serde(rename = "result")]
    pub result: bool,
}

impl HealthResponse {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(result: bool) -> HealthResponse {
        HealthResponse { result }
    }
}

/// Converts the HealthResponse value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for HealthResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![Some("result".to_string()), Some(self.result.to_string())];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a HealthResponse value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for HealthResponse {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub result: Vec<bool>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing HealthResponse".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "result" => intermediate_rep.result.push(<bool as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing HealthResponse".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(HealthResponse {
            result: intermediate_rep.result.into_iter().next().ok_or_else(|| "result missing in HealthResponse".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<HealthResponse> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<HealthResponse>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<HealthResponse>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for HealthResponse - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<HealthResponse> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <HealthResponse as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into HealthResponse - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct Publish {
    /// Opaque daemon-issued publish identifier.
    #[serde(rename = "id")]
    #[validate(length(min = 1), custom(function = "check_xss_string"))]
    pub id: String,

    #[serde(rename = "fileName")]
    #[validate(custom(function = "check_xss_string"))]
    pub file_name: String,

    #[serde(rename = "size")]
    pub size: i64,

    #[serde(rename = "blockSize")]
    pub block_size: i32,

    #[serde(rename = "attrs")]
    #[serde(deserialize_with = "deserialize_optional_nullable")]
    #[serde(default = "default_optional_nullable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<Nullable<String>>,

    #[serde(rename = "priority")]
    pub priority: i64,

    #[serde(rename = "status")]
    #[validate(nested)]
    pub status: models::PublishStatus,

    #[serde(rename = "uploadedSize")]
    pub uploaded_size: i64,

    #[serde(rename = "encodedBlockCount")]
    pub encoded_block_count: i64,

    #[serde(rename = "rootHash")]
    #[serde(deserialize_with = "deserialize_optional_nullable")]
    #[serde(default = "default_optional_nullable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_hash: Option<Nullable<String>>,

    #[serde(rename = "failedReason")]
    #[serde(deserialize_with = "deserialize_optional_nullable")]
    #[serde(default = "default_optional_nullable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_reason: Option<Nullable<String>>,

    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,

    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Publish {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(
        id: String,
        file_name: String,
        size: i64,
        block_size: i32,
        priority: i64,
        status: models::PublishStatus,
        uploaded_size: i64,
        encoded_block_count: i64,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Publish {
        Publish {
            id,
            file_name,
            size,
            block_size,
            attrs: None,
            priority,
            status,
            uploaded_size,
            encoded_block_count,
            root_hash: None,
            failed_reason: None,
            created_at,
            updated_at,
        }
    }
}

/// Converts the Publish value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for Publish {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("id".to_string()),
            Some(self.id.to_string()),
            Some("fileName".to_string()),
            Some(self.file_name.to_string()),
            Some("size".to_string()),
            Some(self.size.to_string()),
            Some("blockSize".to_string()),
            Some(self.block_size.to_string()),
            self.attrs
                .as_ref()
                .map(|attrs| ["attrs".to_string(), attrs.as_ref().map_or("null".to_string(), |x| x.to_string())].join(",")),
            Some("priority".to_string()),
            Some(self.priority.to_string()),
            // Skipping status in query parameter serialization
            Some("uploadedSize".to_string()),
            Some(self.uploaded_size.to_string()),
            Some("encodedBlockCount".to_string()),
            Some(self.encoded_block_count.to_string()),
            self.root_hash
                .as_ref()
                .map(|root_hash| ["rootHash".to_string(), root_hash.as_ref().map_or("null".to_string(), |x| x.to_string())].join(",")),
            self.failed_reason
                .as_ref()
                .map(|failed_reason| ["failedReason".to_string(), failed_reason.as_ref().map_or("null".to_string(), |x| x.to_string())].join(",")),
            // Skipping createdAt in query parameter serialization

            // Skipping updatedAt in query parameter serialization
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a Publish value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for Publish {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub id: Vec<String>,
            pub file_name: Vec<String>,
            pub size: Vec<i64>,
            pub block_size: Vec<i32>,
            pub attrs: Vec<String>,
            pub priority: Vec<i64>,
            pub status: Vec<models::PublishStatus>,
            pub uploaded_size: Vec<i64>,
            pub encoded_block_count: Vec<i64>,
            pub root_hash: Vec<String>,
            pub failed_reason: Vec<String>,
            pub created_at: Vec<chrono::DateTime<chrono::Utc>>,
            pub updated_at: Vec<chrono::DateTime<chrono::Utc>>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing Publish".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "id" => intermediate_rep.id.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "fileName" => intermediate_rep.file_name.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "size" => intermediate_rep.size.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "blockSize" => intermediate_rep.block_size.push(<i32 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    "attrs" => return std::result::Result::Err("Parsing a nullable type in this style is not supported in Publish".to_string()),
                    #[allow(clippy::redundant_clone)]
                    "priority" => intermediate_rep.priority.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "status" => intermediate_rep
                        .status
                        .push(<models::PublishStatus as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "uploadedSize" => intermediate_rep.uploaded_size.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "encodedBlockCount" => intermediate_rep
                        .encoded_block_count
                        .push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    "rootHash" => return std::result::Result::Err("Parsing a nullable type in this style is not supported in Publish".to_string()),
                    "failedReason" => return std::result::Result::Err("Parsing a nullable type in this style is not supported in Publish".to_string()),
                    #[allow(clippy::redundant_clone)]
                    "createdAt" => intermediate_rep
                        .created_at
                        .push(<chrono::DateTime<chrono::Utc> as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "updatedAt" => intermediate_rep
                        .updated_at
                        .push(<chrono::DateTime<chrono::Utc> as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing Publish".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(Publish {
            id: intermediate_rep.id.into_iter().next().ok_or_else(|| "id missing in Publish".to_string())?,
            file_name: intermediate_rep.file_name.into_iter().next().ok_or_else(|| "fileName missing in Publish".to_string())?,
            size: intermediate_rep.size.into_iter().next().ok_or_else(|| "size missing in Publish".to_string())?,
            block_size: intermediate_rep.block_size.into_iter().next().ok_or_else(|| "blockSize missing in Publish".to_string())?,
            attrs: std::result::Result::Err("Nullable types not supported in Publish".to_string())?,
            priority: intermediate_rep.priority.into_iter().next().ok_or_else(|| "priority missing in Publish".to_string())?,
            status: intermediate_rep.status.into_iter().next().ok_or_else(|| "status missing in Publish".to_string())?,
            uploaded_size: intermediate_rep
                .uploaded_size
                .into_iter()
                .next()
                .ok_or_else(|| "uploadedSize missing in Publish".to_string())?,
            encoded_block_count: intermediate_rep
                .encoded_block_count
                .into_iter()
                .next()
                .ok_or_else(|| "encodedBlockCount missing in Publish".to_string())?,
            root_hash: std::result::Result::Err("Nullable types not supported in Publish".to_string())?,
            failed_reason: std::result::Result::Err("Nullable types not supported in Publish".to_string())?,
            created_at: intermediate_rep.created_at.into_iter().next().ok_or_else(|| "createdAt missing in Publish".to_string())?,
            updated_at: intermediate_rep.updated_at.into_iter().next().ok_or_else(|| "updatedAt missing in Publish".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<Publish> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<Publish>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<Publish>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for Publish - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<Publish> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <Publish as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into Publish - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct PublishCreateRequest {
    #[serde(rename = "fileName")]
    #[validate(length(min = 1), custom(function = "check_xss_string"))]
    pub file_name: String,

    #[serde(rename = "size")]
    #[validate(range(min = 0u64))]
    pub size: u64,

    #[serde(rename = "blockSize")]
    #[validate(range(min = 1u32, max = 1048576u32))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size: Option<u32>,

    #[serde(rename = "attrs")]
    #[serde(deserialize_with = "deserialize_optional_nullable")]
    #[serde(default = "default_optional_nullable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<Nullable<String>>,

    #[serde(rename = "priority")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
}

impl PublishCreateRequest {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(file_name: String, size: u64) -> PublishCreateRequest {
        PublishCreateRequest {
            file_name,
            size,
            block_size: Some(1048576),
            attrs: None,
            priority: Some(0),
        }
    }
}

/// Converts the PublishCreateRequest value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for PublishCreateRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("fileName".to_string()),
            Some(self.file_name.to_string()),
            Some("size".to_string()),
            Some(self.size.to_string()),
            self.block_size.as_ref().map(|block_size| ["blockSize".to_string(), block_size.to_string()].join(",")),
            self.attrs
                .as_ref()
                .map(|attrs| ["attrs".to_string(), attrs.as_ref().map_or("null".to_string(), |x| x.to_string())].join(",")),
            self.priority.as_ref().map(|priority| ["priority".to_string(), priority.to_string()].join(",")),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a PublishCreateRequest value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for PublishCreateRequest {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub file_name: Vec<String>,
            pub size: Vec<u64>,
            pub block_size: Vec<u32>,
            pub attrs: Vec<String>,
            pub priority: Vec<i64>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing PublishCreateRequest".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "fileName" => intermediate_rep.file_name.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "size" => intermediate_rep.size.push(<u64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "blockSize" => intermediate_rep.block_size.push(<u32 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    "attrs" => return std::result::Result::Err("Parsing a nullable type in this style is not supported in PublishCreateRequest".to_string()),
                    #[allow(clippy::redundant_clone)]
                    "priority" => intermediate_rep.priority.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing PublishCreateRequest".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(PublishCreateRequest {
            file_name: intermediate_rep
                .file_name
                .into_iter()
                .next()
                .ok_or_else(|| "fileName missing in PublishCreateRequest".to_string())?,
            size: intermediate_rep.size.into_iter().next().ok_or_else(|| "size missing in PublishCreateRequest".to_string())?,
            block_size: intermediate_rep.block_size.into_iter().next(),
            attrs: std::result::Result::Err("Nullable types not supported in PublishCreateRequest".to_string())?,
            priority: intermediate_rep.priority.into_iter().next(),
        })
    }
}

// Methods for converting between header::IntoHeaderValue<PublishCreateRequest> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<PublishCreateRequest>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<PublishCreateRequest>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for PublishCreateRequest - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<PublishCreateRequest> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <PublishCreateRequest as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into PublishCreateRequest - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

/// Opaque daemon-issued publish identifier.
#[derive(Debug, Clone, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct PublishId(pub String);

impl validator::Validate for PublishId {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        std::result::Result::Ok(())
    }
}

impl std::convert::From<String> for PublishId {
    fn from(x: String) -> Self {
        PublishId(x)
    }
}

impl std::fmt::Display for PublishId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for PublishId {
    type Err = std::string::ParseError;
    fn from_str(x: &str) -> std::result::Result<Self, Self::Err> {
        std::result::Result::Ok(PublishId(x.to_string()))
    }
}

impl std::convert::From<PublishId> for String {
    fn from(x: PublishId) -> Self {
        x.0
    }
}

impl std::ops::Deref for PublishId {
    type Target = String;
    fn deref(&self) -> &String {
        &self.0
    }
}

impl std::ops::DerefMut for PublishId {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct PublishList {
    #[serde(rename = "publishes")]
    #[validate(nested)]
    pub publishes: Vec<models::Publish>,

    #[serde(rename = "total")]
    pub total: i64,
}

impl PublishList {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(publishes: Vec<models::Publish>, total: i64) -> PublishList {
        PublishList { publishes, total }
    }
}

/// Converts the PublishList value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for PublishList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping publishes in query parameter serialization
            Some("total".to_string()),
            Some(self.total.to_string()),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a PublishList value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for PublishList {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub publishes: Vec<Vec<models::Publish>>,
            pub total: Vec<i64>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing PublishList".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    "publishes" => return std::result::Result::Err("Parsing a container in this style is not supported in PublishList".to_string()),
                    #[allow(clippy::redundant_clone)]
                    "total" => intermediate_rep.total.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing PublishList".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(PublishList {
            publishes: intermediate_rep
                .publishes
                .into_iter()
                .next()
                .ok_or_else(|| "publishes missing in PublishList".to_string())?,
            total: intermediate_rep.total.into_iter().next().ok_or_else(|| "total missing in PublishList".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<PublishList> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<PublishList>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<PublishList>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for PublishList - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<PublishList> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <PublishList as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into PublishList - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct PublishResponse {
    #[serde(rename = "publish")]
    #[validate(nested)]
    pub publish: models::Publish,
}

impl PublishResponse {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(publish: models::Publish) -> PublishResponse {
        PublishResponse { publish }
    }
}

/// Converts the PublishResponse value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for PublishResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping publish in query parameter serialization

        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a PublishResponse value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for PublishResponse {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub publish: Vec<models::Publish>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing PublishResponse".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "publish" => intermediate_rep
                        .publish
                        .push(<models::Publish as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing PublishResponse".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(PublishResponse {
            publish: intermediate_rep
                .publish
                .into_iter()
                .next()
                .ok_or_else(|| "publish missing in PublishResponse".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<PublishResponse> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<PublishResponse>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<PublishResponse>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for PublishResponse - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<PublishResponse> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <PublishResponse as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into PublishResponse - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

/// Enumeration of values.
/// Since this enum's variants do not hold data, we can easily define them as `#[repr(C)]`
/// which helps with FFI.
#[allow(non_camel_case_types, clippy::large_enum_variant)]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "conversion", derive(frunk_enum_derive::LabelledGenericEnum))]
pub enum PublishStatus {
    #[serde(rename = "Uploading")]
    Uploading,
    #[serde(rename = "Pending")]
    Pending,
    #[serde(rename = "Processing")]
    Processing,
    #[serde(rename = "Succeeded")]
    Succeeded,
    #[serde(rename = "Failed")]
    Failed,
}

impl validator::Validate for PublishStatus {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        std::result::Result::Ok(())
    }
}

impl std::fmt::Display for PublishStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            PublishStatus::Uploading => write!(f, "Uploading"),
            PublishStatus::Pending => write!(f, "Pending"),
            PublishStatus::Processing => write!(f, "Processing"),
            PublishStatus::Succeeded => write!(f, "Succeeded"),
            PublishStatus::Failed => write!(f, "Failed"),
        }
    }
}

impl std::str::FromStr for PublishStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "Uploading" => std::result::Result::Ok(PublishStatus::Uploading),
            "Pending" => std::result::Result::Ok(PublishStatus::Pending),
            "Processing" => std::result::Result::Ok(PublishStatus::Processing),
            "Succeeded" => std::result::Result::Ok(PublishStatus::Succeeded),
            "Failed" => std::result::Result::Ok(PublishStatus::Failed),
            _ => std::result::Result::Err(format!(r#"Value not valid: {s}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct Subscribe {
    /// Opaque daemon-issued subscription identifier.
    #[serde(rename = "id")]
    #[validate(length(min = 1), custom(function = "check_xss_string"))]
    pub id: String,

    #[serde(rename = "rootHash")]
    #[validate(custom(function = "check_xss_string"))]
    pub root_hash: String,

    #[serde(rename = "outputPath")]
    #[validate(custom(function = "check_xss_string"))]
    pub output_path: String,

    #[serde(rename = "rank")]
    pub rank: i32,

    #[serde(rename = "blockCountDownloaded")]
    pub block_count_downloaded: i32,

    #[serde(rename = "blockCountTotal")]
    pub block_count_total: i32,

    #[serde(rename = "attrs")]
    #[serde(deserialize_with = "deserialize_optional_nullable")]
    #[serde(default = "default_optional_nullable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<Nullable<String>>,

    #[serde(rename = "priority")]
    pub priority: i64,

    #[serde(rename = "status")]
    #[validate(nested)]
    pub status: models::SubscribeStatus,

    #[serde(rename = "failedReason")]
    #[serde(deserialize_with = "deserialize_optional_nullable")]
    #[serde(default = "default_optional_nullable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_reason: Option<Nullable<String>>,

    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,

    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Subscribe {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(
        id: String,
        root_hash: String,
        output_path: String,
        rank: i32,
        block_count_downloaded: i32,
        block_count_total: i32,
        priority: i64,
        status: models::SubscribeStatus,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Subscribe {
        Subscribe {
            id,
            root_hash,
            output_path,
            rank,
            block_count_downloaded,
            block_count_total,
            attrs: None,
            priority,
            status,
            failed_reason: None,
            created_at,
            updated_at,
        }
    }
}

/// Converts the Subscribe value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for Subscribe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("id".to_string()),
            Some(self.id.to_string()),
            Some("rootHash".to_string()),
            Some(self.root_hash.to_string()),
            Some("outputPath".to_string()),
            Some(self.output_path.to_string()),
            Some("rank".to_string()),
            Some(self.rank.to_string()),
            Some("blockCountDownloaded".to_string()),
            Some(self.block_count_downloaded.to_string()),
            Some("blockCountTotal".to_string()),
            Some(self.block_count_total.to_string()),
            self.attrs
                .as_ref()
                .map(|attrs| ["attrs".to_string(), attrs.as_ref().map_or("null".to_string(), |x| x.to_string())].join(",")),
            Some("priority".to_string()),
            Some(self.priority.to_string()),
            // Skipping status in query parameter serialization
            self.failed_reason
                .as_ref()
                .map(|failed_reason| ["failedReason".to_string(), failed_reason.as_ref().map_or("null".to_string(), |x| x.to_string())].join(",")),
            // Skipping createdAt in query parameter serialization

            // Skipping updatedAt in query parameter serialization
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a Subscribe value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for Subscribe {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub id: Vec<String>,
            pub root_hash: Vec<String>,
            pub output_path: Vec<String>,
            pub rank: Vec<i32>,
            pub block_count_downloaded: Vec<i32>,
            pub block_count_total: Vec<i32>,
            pub attrs: Vec<String>,
            pub priority: Vec<i64>,
            pub status: Vec<models::SubscribeStatus>,
            pub failed_reason: Vec<String>,
            pub created_at: Vec<chrono::DateTime<chrono::Utc>>,
            pub updated_at: Vec<chrono::DateTime<chrono::Utc>>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing Subscribe".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "id" => intermediate_rep.id.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "rootHash" => intermediate_rep.root_hash.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "outputPath" => intermediate_rep.output_path.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "rank" => intermediate_rep.rank.push(<i32 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "blockCountDownloaded" => intermediate_rep
                        .block_count_downloaded
                        .push(<i32 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "blockCountTotal" => intermediate_rep
                        .block_count_total
                        .push(<i32 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    "attrs" => return std::result::Result::Err("Parsing a nullable type in this style is not supported in Subscribe".to_string()),
                    #[allow(clippy::redundant_clone)]
                    "priority" => intermediate_rep.priority.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "status" => intermediate_rep
                        .status
                        .push(<models::SubscribeStatus as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    "failedReason" => return std::result::Result::Err("Parsing a nullable type in this style is not supported in Subscribe".to_string()),
                    #[allow(clippy::redundant_clone)]
                    "createdAt" => intermediate_rep
                        .created_at
                        .push(<chrono::DateTime<chrono::Utc> as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "updatedAt" => intermediate_rep
                        .updated_at
                        .push(<chrono::DateTime<chrono::Utc> as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing Subscribe".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(Subscribe {
            id: intermediate_rep.id.into_iter().next().ok_or_else(|| "id missing in Subscribe".to_string())?,
            root_hash: intermediate_rep.root_hash.into_iter().next().ok_or_else(|| "rootHash missing in Subscribe".to_string())?,
            output_path: intermediate_rep
                .output_path
                .into_iter()
                .next()
                .ok_or_else(|| "outputPath missing in Subscribe".to_string())?,
            rank: intermediate_rep.rank.into_iter().next().ok_or_else(|| "rank missing in Subscribe".to_string())?,
            block_count_downloaded: intermediate_rep
                .block_count_downloaded
                .into_iter()
                .next()
                .ok_or_else(|| "blockCountDownloaded missing in Subscribe".to_string())?,
            block_count_total: intermediate_rep
                .block_count_total
                .into_iter()
                .next()
                .ok_or_else(|| "blockCountTotal missing in Subscribe".to_string())?,
            attrs: std::result::Result::Err("Nullable types not supported in Subscribe".to_string())?,
            priority: intermediate_rep.priority.into_iter().next().ok_or_else(|| "priority missing in Subscribe".to_string())?,
            status: intermediate_rep.status.into_iter().next().ok_or_else(|| "status missing in Subscribe".to_string())?,
            failed_reason: std::result::Result::Err("Nullable types not supported in Subscribe".to_string())?,
            created_at: intermediate_rep.created_at.into_iter().next().ok_or_else(|| "createdAt missing in Subscribe".to_string())?,
            updated_at: intermediate_rep.updated_at.into_iter().next().ok_or_else(|| "updatedAt missing in Subscribe".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<Subscribe> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<Subscribe>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<Subscribe>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for Subscribe - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<Subscribe> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <Subscribe as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into Subscribe - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct SubscribeCreateRequest {
    #[serde(rename = "rootHash")]
    #[validate(length(min = 1), custom(function = "check_xss_string"))]
    pub root_hash: String,

    #[serde(rename = "outputPath")]
    #[validate(length(min = 1), custom(function = "check_xss_string"))]
    pub output_path: String,

    #[serde(rename = "attrs")]
    #[serde(deserialize_with = "deserialize_optional_nullable")]
    #[serde(default = "default_optional_nullable")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attrs: Option<Nullable<String>>,

    #[serde(rename = "priority")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
}

impl SubscribeCreateRequest {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(root_hash: String, output_path: String) -> SubscribeCreateRequest {
        SubscribeCreateRequest {
            root_hash,
            output_path,
            attrs: None,
            priority: Some(0),
        }
    }
}

/// Converts the SubscribeCreateRequest value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for SubscribeCreateRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("rootHash".to_string()),
            Some(self.root_hash.to_string()),
            Some("outputPath".to_string()),
            Some(self.output_path.to_string()),
            self.attrs
                .as_ref()
                .map(|attrs| ["attrs".to_string(), attrs.as_ref().map_or("null".to_string(), |x| x.to_string())].join(",")),
            self.priority.as_ref().map(|priority| ["priority".to_string(), priority.to_string()].join(",")),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a SubscribeCreateRequest value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for SubscribeCreateRequest {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub root_hash: Vec<String>,
            pub output_path: Vec<String>,
            pub attrs: Vec<String>,
            pub priority: Vec<i64>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing SubscribeCreateRequest".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "rootHash" => intermediate_rep.root_hash.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "outputPath" => intermediate_rep.output_path.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    "attrs" => return std::result::Result::Err("Parsing a nullable type in this style is not supported in SubscribeCreateRequest".to_string()),
                    #[allow(clippy::redundant_clone)]
                    "priority" => intermediate_rep.priority.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing SubscribeCreateRequest".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(SubscribeCreateRequest {
            root_hash: intermediate_rep
                .root_hash
                .into_iter()
                .next()
                .ok_or_else(|| "rootHash missing in SubscribeCreateRequest".to_string())?,
            output_path: intermediate_rep
                .output_path
                .into_iter()
                .next()
                .ok_or_else(|| "outputPath missing in SubscribeCreateRequest".to_string())?,
            attrs: std::result::Result::Err("Nullable types not supported in SubscribeCreateRequest".to_string())?,
            priority: intermediate_rep.priority.into_iter().next(),
        })
    }
}

// Methods for converting between header::IntoHeaderValue<SubscribeCreateRequest> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<SubscribeCreateRequest>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<SubscribeCreateRequest>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for SubscribeCreateRequest - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<SubscribeCreateRequest> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <SubscribeCreateRequest as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into SubscribeCreateRequest - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

/// Opaque daemon-issued subscription identifier.
#[derive(Debug, Clone, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct SubscribeId(pub String);

impl validator::Validate for SubscribeId {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        std::result::Result::Ok(())
    }
}

impl std::convert::From<String> for SubscribeId {
    fn from(x: String) -> Self {
        SubscribeId(x)
    }
}

impl std::fmt::Display for SubscribeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SubscribeId {
    type Err = std::string::ParseError;
    fn from_str(x: &str) -> std::result::Result<Self, Self::Err> {
        std::result::Result::Ok(SubscribeId(x.to_string()))
    }
}

impl std::convert::From<SubscribeId> for String {
    fn from(x: SubscribeId) -> Self {
        x.0
    }
}

impl std::ops::Deref for SubscribeId {
    type Target = String;
    fn deref(&self) -> &String {
        &self.0
    }
}

impl std::ops::DerefMut for SubscribeId {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.0
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct SubscribeList {
    #[serde(rename = "subscribes")]
    #[validate(nested)]
    pub subscribes: Vec<models::Subscribe>,

    #[serde(rename = "total")]
    pub total: i64,
}

impl SubscribeList {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(subscribes: Vec<models::Subscribe>, total: i64) -> SubscribeList {
        SubscribeList { subscribes, total }
    }
}

/// Converts the SubscribeList value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for SubscribeList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping subscribes in query parameter serialization
            Some("total".to_string()),
            Some(self.total.to_string()),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a SubscribeList value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for SubscribeList {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub subscribes: Vec<Vec<models::Subscribe>>,
            pub total: Vec<i64>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing SubscribeList".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    "subscribes" => return std::result::Result::Err("Parsing a container in this style is not supported in SubscribeList".to_string()),
                    #[allow(clippy::redundant_clone)]
                    "total" => intermediate_rep.total.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing SubscribeList".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(SubscribeList {
            subscribes: intermediate_rep
                .subscribes
                .into_iter()
                .next()
                .ok_or_else(|| "subscribes missing in SubscribeList".to_string())?,
            total: intermediate_rep.total.into_iter().next().ok_or_else(|| "total missing in SubscribeList".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<SubscribeList> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<SubscribeList>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<SubscribeList>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for SubscribeList - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<SubscribeList> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <SubscribeList as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into SubscribeList - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct SubscribeResponse {
    #[serde(rename = "subscribe")]
    #[validate(nested)]
    pub subscribe: models::Subscribe,
}

impl SubscribeResponse {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(subscribe: models::Subscribe) -> SubscribeResponse {
        SubscribeResponse { subscribe }
    }
}

/// Converts the SubscribeResponse value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for SubscribeResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping subscribe in query parameter serialization

        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a SubscribeResponse value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for SubscribeResponse {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub subscribe: Vec<models::Subscribe>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing SubscribeResponse".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "subscribe" => intermediate_rep
                        .subscribe
                        .push(<models::Subscribe as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing SubscribeResponse".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(SubscribeResponse {
            subscribe: intermediate_rep
                .subscribe
                .into_iter()
                .next()
                .ok_or_else(|| "subscribe missing in SubscribeResponse".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<SubscribeResponse> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<SubscribeResponse>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<SubscribeResponse>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for SubscribeResponse - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<SubscribeResponse> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <SubscribeResponse as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into SubscribeResponse - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

/// Enumeration of values.
/// Since this enum's variants do not hold data, we can easily define them as `#[repr(C)]`
/// which helps with FFI.
#[allow(non_camel_case_types, clippy::large_enum_variant)]
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "conversion", derive(frunk_enum_derive::LabelledGenericEnum))]
pub enum SubscribeStatus {
    #[serde(rename = "Downloading")]
    Downloading,
    #[serde(rename = "Decoding")]
    Decoding,
    #[serde(rename = "Finalizing")]
    Finalizing,
    #[serde(rename = "Completed")]
    Completed,
    #[serde(rename = "Failed")]
    Failed,
    #[serde(rename = "Canceled")]
    Canceled,
}

impl validator::Validate for SubscribeStatus {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        std::result::Result::Ok(())
    }
}

impl std::fmt::Display for SubscribeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            SubscribeStatus::Downloading => write!(f, "Downloading"),
            SubscribeStatus::Decoding => write!(f, "Decoding"),
            SubscribeStatus::Finalizing => write!(f, "Finalizing"),
            SubscribeStatus::Completed => write!(f, "Completed"),
            SubscribeStatus::Failed => write!(f, "Failed"),
            SubscribeStatus::Canceled => write!(f, "Canceled"),
        }
    }
}

impl std::str::FromStr for SubscribeStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "Downloading" => std::result::Result::Ok(SubscribeStatus::Downloading),
            "Decoding" => std::result::Result::Ok(SubscribeStatus::Decoding),
            "Finalizing" => std::result::Result::Ok(SubscribeStatus::Finalizing),
            "Completed" => std::result::Result::Ok(SubscribeStatus::Completed),
            "Failed" => std::result::Result::Ok(SubscribeStatus::Failed),
            "Canceled" => std::result::Result::Ok(SubscribeStatus::Canceled),
            _ => std::result::Result::Err(format!(r#"Value not valid: {s}"#)),
        }
    }
}
