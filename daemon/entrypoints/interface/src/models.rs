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
pub struct CreateSessionHeaderParams {
    pub x_axus_transport: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct ListLibraryEntriesPathParams {
    pub root_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct ListLibraryEntriesQueryParams {
    #[serde(rename = "path")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct CancelPublicationPathParams {
    pub publication_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct DeletePublicationPathParams {
    pub publication_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct GetPublicationPathParams {
    pub publication_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct WritePublicationContentHeaderParams {
    pub content_range: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct WritePublicationContentPathParams {
    pub publication_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct CancelSubscriptionPathParams {
    pub subscription_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct DeleteSubscriptionPathParams {
    pub subscription_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct DeleteSubscriptionQueryParams {
    #[serde(rename = "deleteOutput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_output: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct GetSubscriptionPathParams {
    pub subscription_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct GetSubscriptionContentHeaderParams {
    pub range: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct GetSubscriptionContentPathParams {
    pub subscription_id: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct CreatePublicationRequest {
    /// Required for an upload source.
    #[serde(rename = "fileName")]
    #[validate(custom(function = "check_xss_string"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,

    /// The final size in bytes, required for an upload source.
    #[serde(rename = "size")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,

    #[serde(rename = "source")]
    #[validate(nested)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<models::PublicationSource>,
}

impl CreatePublicationRequest {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new() -> CreatePublicationRequest {
        CreatePublicationRequest {
            file_name: None,
            size: None,
            source: None,
        }
    }
}

/// Converts the CreatePublicationRequest value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for CreatePublicationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            self.file_name.as_ref().map(|file_name| ["fileName".to_string(), file_name.to_string()].join(",")),
            self.size.as_ref().map(|size| ["size".to_string(), size.to_string()].join(",")),
            // Skipping source in query parameter serialization
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a CreatePublicationRequest value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for CreatePublicationRequest {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub file_name: Vec<String>,
            pub size: Vec<i64>,
            pub source: Vec<models::PublicationSource>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing CreatePublicationRequest".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "fileName" => intermediate_rep.file_name.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "size" => intermediate_rep.size.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "source" => intermediate_rep
                        .source
                        .push(<models::PublicationSource as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing CreatePublicationRequest".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(CreatePublicationRequest {
            file_name: intermediate_rep.file_name.into_iter().next(),
            size: intermediate_rep.size.into_iter().next(),
            source: intermediate_rep.source.into_iter().next(),
        })
    }
}

// Methods for converting between header::IntoHeaderValue<CreatePublicationRequest> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<CreatePublicationRequest>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<CreatePublicationRequest>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for CreatePublicationRequest - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<CreatePublicationRequest> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <CreatePublicationRequest as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into CreatePublicationRequest - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct CreateSubscriptionRequest {
    #[serde(rename = "rootHash")]
    #[validate(custom(function = "check_xss_string"))]
    pub root_hash: String,

    #[serde(rename = "fileName")]
    #[validate(custom(function = "check_xss_string"))]
    pub file_name: String,
}

impl CreateSubscriptionRequest {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(root_hash: String, file_name: String) -> CreateSubscriptionRequest {
        CreateSubscriptionRequest { root_hash, file_name }
    }
}

/// Converts the CreateSubscriptionRequest value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for CreateSubscriptionRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("rootHash".to_string()),
            Some(self.root_hash.to_string()),
            Some("fileName".to_string()),
            Some(self.file_name.to_string()),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a CreateSubscriptionRequest value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for CreateSubscriptionRequest {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub root_hash: Vec<String>,
            pub file_name: Vec<String>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing CreateSubscriptionRequest".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "rootHash" => intermediate_rep.root_hash.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "fileName" => intermediate_rep.file_name.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing CreateSubscriptionRequest".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(CreateSubscriptionRequest {
            root_hash: intermediate_rep
                .root_hash
                .into_iter()
                .next()
                .ok_or_else(|| "rootHash missing in CreateSubscriptionRequest".to_string())?,
            file_name: intermediate_rep
                .file_name
                .into_iter()
                .next()
                .ok_or_else(|| "fileName missing in CreateSubscriptionRequest".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<CreateSubscriptionRequest> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<CreateSubscriptionRequest>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<CreateSubscriptionRequest>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for CreateSubscriptionRequest - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<CreateSubscriptionRequest> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <CreateSubscriptionRequest as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into CreateSubscriptionRequest - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct Credentials {
    #[serde(rename = "name")]
    #[validate(custom(function = "check_xss_string"))]
    pub name: String,

    #[serde(rename = "password")]
    #[validate(custom(function = "check_xss_string"))]
    pub password: String,
}

impl Credentials {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(name: String, password: String) -> Credentials {
        Credentials { name, password }
    }
}

/// Converts the Credentials value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("name".to_string()),
            Some(self.name.to_string()),
            Some("password".to_string()),
            Some(self.password.to_string()),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a Credentials value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for Credentials {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub name: Vec<String>,
            pub password: Vec<String>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing Credentials".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "name" => intermediate_rep.name.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "password" => intermediate_rep.password.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing Credentials".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(Credentials {
            name: intermediate_rep.name.into_iter().next().ok_or_else(|| "name missing in Credentials".to_string())?,
            password: intermediate_rep.password.into_iter().next().ok_or_else(|| "password missing in Credentials".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<Credentials> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<Credentials>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<Credentials>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for Credentials - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<Credentials> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <Credentials as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into Credentials - {err}"#)),
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
pub enum ErrorKind {
    #[serde(rename = "InternalServerError")]
    InternalServerError,
    #[serde(rename = "InvalidRequest")]
    InvalidRequest,
    #[serde(rename = "LimitExceeded")]
    LimitExceeded,
    #[serde(rename = "NotFound")]
    NotFound,
    #[serde(rename = "Unauthorized")]
    Unauthorized,
    #[serde(rename = "Forbidden")]
    Forbidden,
    #[serde(rename = "Conflict")]
    Conflict,
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
            ErrorKind::LimitExceeded => write!(f, "LimitExceeded"),
            ErrorKind::NotFound => write!(f, "NotFound"),
            ErrorKind::Unauthorized => write!(f, "Unauthorized"),
            ErrorKind::Forbidden => write!(f, "Forbidden"),
            ErrorKind::Conflict => write!(f, "Conflict"),
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
            "LimitExceeded" => std::result::Result::Ok(ErrorKind::LimitExceeded),
            "NotFound" => std::result::Result::Ok(ErrorKind::NotFound),
            "Unauthorized" => std::result::Result::Ok(ErrorKind::Unauthorized),
            "Forbidden" => std::result::Result::Ok(ErrorKind::Forbidden),
            "Conflict" => std::result::Result::Ok(ErrorKind::Conflict),
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

    /// Present when the request conflicts with the received offset.
    #[serde(rename = "receivedBytes")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_bytes: Option<i64>,
}

impl ErrorResponse {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(kind: models::ErrorKind) -> ErrorResponse {
        ErrorResponse {
            kind,
            message: None,
            request_id: None,
            received_bytes: None,
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
            self.received_bytes
                .as_ref()
                .map(|received_bytes| ["receivedBytes".to_string(), received_bytes.to_string()].join(",")),
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
            pub received_bytes: Vec<i64>,
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
                    #[allow(clippy::redundant_clone)]
                    "receivedBytes" => intermediate_rep.received_bytes.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
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
            received_bytes: intermediate_rep.received_bytes.into_iter().next(),
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
pub struct LibraryEntry {
    #[serde(rename = "name")]
    #[validate(custom(function = "check_xss_string"))]
    pub name: String,

    /// Note: inline enums are not fully supported by openapi-generator
    #[serde(rename = "kind")]
    #[validate(custom(function = "check_xss_string"))]
    pub kind: String,

    #[serde(rename = "size")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
}

impl LibraryEntry {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(name: String, kind: String) -> LibraryEntry {
        LibraryEntry { name, kind, size: None }
    }
}

/// Converts the LibraryEntry value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for LibraryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("name".to_string()),
            Some(self.name.to_string()),
            Some("kind".to_string()),
            Some(self.kind.to_string()),
            self.size.as_ref().map(|size| ["size".to_string(), size.to_string()].join(",")),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a LibraryEntry value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for LibraryEntry {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub name: Vec<String>,
            pub kind: Vec<String>,
            pub size: Vec<i64>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing LibraryEntry".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "name" => intermediate_rep.name.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "kind" => intermediate_rep.kind.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "size" => intermediate_rep.size.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing LibraryEntry".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(LibraryEntry {
            name: intermediate_rep.name.into_iter().next().ok_or_else(|| "name missing in LibraryEntry".to_string())?,
            kind: intermediate_rep.kind.into_iter().next().ok_or_else(|| "kind missing in LibraryEntry".to_string())?,
            size: intermediate_rep.size.into_iter().next(),
        })
    }
}

// Methods for converting between header::IntoHeaderValue<LibraryEntry> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<LibraryEntry>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<LibraryEntry>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for LibraryEntry - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<LibraryEntry> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <LibraryEntry as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into LibraryEntry - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct LibraryEntryList {
    #[serde(rename = "entries")]
    #[validate(nested)]
    pub entries: Vec<models::LibraryEntry>,
}

impl LibraryEntryList {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(entries: Vec<models::LibraryEntry>) -> LibraryEntryList {
        LibraryEntryList { entries }
    }
}

/// Converts the LibraryEntryList value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for LibraryEntryList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping entries in query parameter serialization

        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a LibraryEntryList value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for LibraryEntryList {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub entries: Vec<Vec<models::LibraryEntry>>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing LibraryEntryList".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    "entries" => return std::result::Result::Err("Parsing a container in this style is not supported in LibraryEntryList".to_string()),
                    _ => return std::result::Result::Err("Unexpected key while parsing LibraryEntryList".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(LibraryEntryList {
            entries: intermediate_rep
                .entries
                .into_iter()
                .next()
                .ok_or_else(|| "entries missing in LibraryEntryList".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<LibraryEntryList> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<LibraryEntryList>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<LibraryEntryList>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for LibraryEntryList - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<LibraryEntryList> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <LibraryEntryList as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into LibraryEntryList - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct LibraryRoot {
    #[serde(rename = "id")]
    #[validate(custom(function = "check_xss_string"))]
    pub id: String,

    #[serde(rename = "name")]
    #[validate(custom(function = "check_xss_string"))]
    pub name: String,
}

impl LibraryRoot {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(id: String, name: String) -> LibraryRoot {
        LibraryRoot { id, name }
    }
}

/// Converts the LibraryRoot value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for LibraryRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![Some("id".to_string()), Some(self.id.to_string()), Some("name".to_string()), Some(self.name.to_string())];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a LibraryRoot value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for LibraryRoot {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub id: Vec<String>,
            pub name: Vec<String>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing LibraryRoot".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "id" => intermediate_rep.id.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "name" => intermediate_rep.name.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing LibraryRoot".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(LibraryRoot {
            id: intermediate_rep.id.into_iter().next().ok_or_else(|| "id missing in LibraryRoot".to_string())?,
            name: intermediate_rep.name.into_iter().next().ok_or_else(|| "name missing in LibraryRoot".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<LibraryRoot> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<LibraryRoot>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<LibraryRoot>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for LibraryRoot - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<LibraryRoot> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <LibraryRoot as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into LibraryRoot - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct LibraryRootList {
    #[serde(rename = "roots")]
    #[validate(nested)]
    pub roots: Vec<models::LibraryRoot>,
}

impl LibraryRootList {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(roots: Vec<models::LibraryRoot>) -> LibraryRootList {
        LibraryRootList { roots }
    }
}

/// Converts the LibraryRootList value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for LibraryRootList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping roots in query parameter serialization

        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a LibraryRootList value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for LibraryRootList {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub roots: Vec<Vec<models::LibraryRoot>>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing LibraryRootList".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    "roots" => return std::result::Result::Err("Parsing a container in this style is not supported in LibraryRootList".to_string()),
                    _ => return std::result::Result::Err("Unexpected key while parsing LibraryRootList".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(LibraryRootList {
            roots: intermediate_rep.roots.into_iter().next().ok_or_else(|| "roots missing in LibraryRootList".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<LibraryRootList> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<LibraryRootList>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<LibraryRootList>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for LibraryRootList - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<LibraryRootList> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <LibraryRootList as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into LibraryRootList - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct LoginResponse {
    #[serde(rename = "token")]
    #[validate(custom(function = "check_xss_string"))]
    pub token: String,
}

impl LoginResponse {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(token: String) -> LoginResponse {
        LoginResponse { token }
    }
}

/// Converts the LoginResponse value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for LoginResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![Some("token".to_string()), Some(self.token.to_string())];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a LoginResponse value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for LoginResponse {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub token: Vec<String>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing LoginResponse".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "token" => intermediate_rep.token.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing LoginResponse".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(LoginResponse {
            token: intermediate_rep.token.into_iter().next().ok_or_else(|| "token missing in LoginResponse".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<LoginResponse> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<LoginResponse>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<LoginResponse>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for LoginResponse - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<LoginResponse> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <LoginResponse as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into LoginResponse - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct Publication {
    #[serde(rename = "id")]
    #[validate(custom(function = "check_xss_string"))]
    pub id: String,

    #[serde(rename = "fileName")]
    #[validate(custom(function = "check_xss_string"))]
    pub file_name: String,

    #[serde(rename = "size")]
    pub size: i64,

    #[serde(rename = "receivedBytes")]
    pub received_bytes: i64,

    #[serde(rename = "status")]
    #[validate(nested)]
    pub status: models::PublicationStatus,

    /// Present when Committed.
    #[serde(rename = "rootHash")]
    #[validate(custom(function = "check_xss_string"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_hash: Option<String>,

    /// Present when Failed.
    #[serde(rename = "failedReason")]
    #[validate(custom(function = "check_xss_string"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_reason: Option<String>,

    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,

    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Publication {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(
        id: String,
        file_name: String,
        size: i64,
        received_bytes: i64,
        status: models::PublicationStatus,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Publication {
        Publication {
            id,
            file_name,
            size,
            received_bytes,
            status,
            root_hash: None,
            failed_reason: None,
            created_at,
            updated_at,
        }
    }
}

/// Converts the Publication value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for Publication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("id".to_string()),
            Some(self.id.to_string()),
            Some("fileName".to_string()),
            Some(self.file_name.to_string()),
            Some("size".to_string()),
            Some(self.size.to_string()),
            Some("receivedBytes".to_string()),
            Some(self.received_bytes.to_string()),
            // Skipping status in query parameter serialization
            self.root_hash.as_ref().map(|root_hash| ["rootHash".to_string(), root_hash.to_string()].join(",")),
            self.failed_reason
                .as_ref()
                .map(|failed_reason| ["failedReason".to_string(), failed_reason.to_string()].join(",")),
            // Skipping createdAt in query parameter serialization

            // Skipping updatedAt in query parameter serialization
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a Publication value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for Publication {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub id: Vec<String>,
            pub file_name: Vec<String>,
            pub size: Vec<i64>,
            pub received_bytes: Vec<i64>,
            pub status: Vec<models::PublicationStatus>,
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
                None => return std::result::Result::Err("Missing value while parsing Publication".to_string()),
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
                    "receivedBytes" => intermediate_rep.received_bytes.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "status" => intermediate_rep
                        .status
                        .push(<models::PublicationStatus as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "rootHash" => intermediate_rep.root_hash.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "failedReason" => intermediate_rep
                        .failed_reason
                        .push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "createdAt" => intermediate_rep
                        .created_at
                        .push(<chrono::DateTime<chrono::Utc> as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "updatedAt" => intermediate_rep
                        .updated_at
                        .push(<chrono::DateTime<chrono::Utc> as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing Publication".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(Publication {
            id: intermediate_rep.id.into_iter().next().ok_or_else(|| "id missing in Publication".to_string())?,
            file_name: intermediate_rep.file_name.into_iter().next().ok_or_else(|| "fileName missing in Publication".to_string())?,
            size: intermediate_rep.size.into_iter().next().ok_or_else(|| "size missing in Publication".to_string())?,
            received_bytes: intermediate_rep
                .received_bytes
                .into_iter()
                .next()
                .ok_or_else(|| "receivedBytes missing in Publication".to_string())?,
            status: intermediate_rep.status.into_iter().next().ok_or_else(|| "status missing in Publication".to_string())?,
            root_hash: intermediate_rep.root_hash.into_iter().next(),
            failed_reason: intermediate_rep.failed_reason.into_iter().next(),
            created_at: intermediate_rep
                .created_at
                .into_iter()
                .next()
                .ok_or_else(|| "createdAt missing in Publication".to_string())?,
            updated_at: intermediate_rep
                .updated_at
                .into_iter()
                .next()
                .ok_or_else(|| "updatedAt missing in Publication".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<Publication> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<Publication>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<Publication>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for Publication - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<Publication> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <Publication as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into Publication - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct PublicationList {
    #[serde(rename = "publications")]
    #[validate(nested)]
    pub publications: Vec<models::Publication>,
}

impl PublicationList {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(publications: Vec<models::Publication>) -> PublicationList {
        PublicationList { publications }
    }
}

/// Converts the PublicationList value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for PublicationList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping publications in query parameter serialization

        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a PublicationList value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for PublicationList {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub publications: Vec<Vec<models::Publication>>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing PublicationList".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    "publications" => return std::result::Result::Err("Parsing a container in this style is not supported in PublicationList".to_string()),
                    _ => return std::result::Result::Err("Unexpected key while parsing PublicationList".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(PublicationList {
            publications: intermediate_rep
                .publications
                .into_iter()
                .next()
                .ok_or_else(|| "publications missing in PublicationList".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<PublicationList> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<PublicationList>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<PublicationList>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for PublicationList - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<PublicationList> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <PublicationList as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into PublicationList - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct PublicationSource {
    /// Note: inline enums are not fully supported by openapi-generator
    #[serde(rename = "kind")]
    #[validate(custom(function = "check_xss_string"))]
    pub kind: String,

    /// The library root, required when kind is library.
    #[serde(rename = "rootId")]
    #[validate(custom(function = "check_xss_string"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<String>,

    /// The path inside the root, required when kind is library.
    #[serde(rename = "path")]
    #[validate(custom(function = "check_xss_string"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl PublicationSource {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(kind: String) -> PublicationSource {
        PublicationSource { kind, root_id: None, path: None }
    }
}

/// Converts the PublicationSource value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for PublicationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("kind".to_string()),
            Some(self.kind.to_string()),
            self.root_id.as_ref().map(|root_id| ["rootId".to_string(), root_id.to_string()].join(",")),
            self.path.as_ref().map(|path| ["path".to_string(), path.to_string()].join(",")),
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a PublicationSource value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for PublicationSource {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub kind: Vec<String>,
            pub root_id: Vec<String>,
            pub path: Vec<String>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing PublicationSource".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "kind" => intermediate_rep.kind.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "rootId" => intermediate_rep.root_id.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "path" => intermediate_rep.path.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing PublicationSource".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(PublicationSource {
            kind: intermediate_rep.kind.into_iter().next().ok_or_else(|| "kind missing in PublicationSource".to_string())?,
            root_id: intermediate_rep.root_id.into_iter().next(),
            path: intermediate_rep.path.into_iter().next(),
        })
    }
}

// Methods for converting between header::IntoHeaderValue<PublicationSource> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<PublicationSource>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<PublicationSource>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for PublicationSource - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<PublicationSource> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <PublicationSource as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into PublicationSource - {err}"#)),
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
pub enum PublicationStatus {
    #[serde(rename = "Receiving")]
    Receiving,
    #[serde(rename = "Pending")]
    Pending,
    #[serde(rename = "Processing")]
    Processing,
    #[serde(rename = "Committed")]
    Committed,
    #[serde(rename = "Failed")]
    Failed,
    #[serde(rename = "Canceled")]
    Canceled,
}

impl validator::Validate for PublicationStatus {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        std::result::Result::Ok(())
    }
}

impl std::fmt::Display for PublicationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            PublicationStatus::Receiving => write!(f, "Receiving"),
            PublicationStatus::Pending => write!(f, "Pending"),
            PublicationStatus::Processing => write!(f, "Processing"),
            PublicationStatus::Committed => write!(f, "Committed"),
            PublicationStatus::Failed => write!(f, "Failed"),
            PublicationStatus::Canceled => write!(f, "Canceled"),
        }
    }
}

impl std::str::FromStr for PublicationStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "Receiving" => std::result::Result::Ok(PublicationStatus::Receiving),
            "Pending" => std::result::Result::Ok(PublicationStatus::Pending),
            "Processing" => std::result::Result::Ok(PublicationStatus::Processing),
            "Committed" => std::result::Result::Ok(PublicationStatus::Committed),
            "Failed" => std::result::Result::Ok(PublicationStatus::Failed),
            "Canceled" => std::result::Result::Ok(PublicationStatus::Canceled),
            _ => std::result::Result::Err(format!(r#"Value not valid: {s}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct SetupStatus {
    #[serde(rename = "initialized")]
    pub initialized: bool,
}

impl SetupStatus {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(initialized: bool) -> SetupStatus {
        SetupStatus { initialized }
    }
}

/// Converts the SetupStatus value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for SetupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![Some("initialized".to_string()), Some(self.initialized.to_string())];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a SetupStatus value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for SetupStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub initialized: Vec<bool>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing SetupStatus".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "initialized" => intermediate_rep.initialized.push(<bool as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing SetupStatus".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(SetupStatus {
            initialized: intermediate_rep
                .initialized
                .into_iter()
                .next()
                .ok_or_else(|| "initialized missing in SetupStatus".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<SetupStatus> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<SetupStatus>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<SetupStatus>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for SetupStatus - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<SetupStatus> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <SetupStatus as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into SetupStatus - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct Subscription {
    #[serde(rename = "id")]
    #[validate(custom(function = "check_xss_string"))]
    pub id: String,

    #[serde(rename = "rootHash")]
    #[validate(custom(function = "check_xss_string"))]
    pub root_hash: String,

    #[serde(rename = "fileName")]
    #[validate(custom(function = "check_xss_string"))]
    pub file_name: String,

    #[serde(rename = "status")]
    #[validate(nested)]
    pub status: models::SubscriptionStatus,

    /// Absent until the root block is decoded.
    #[serde(rename = "rank")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<i64>,

    #[serde(rename = "blockCountDownloaded")]
    pub block_count_downloaded: i64,

    #[serde(rename = "blockCountTotal")]
    pub block_count_total: i64,

    /// Present when Failed.
    #[serde(rename = "failedReason")]
    #[validate(custom(function = "check_xss_string"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_reason: Option<String>,

    #[serde(rename = "createdAt")]
    pub created_at: chrono::DateTime<chrono::Utc>,

    #[serde(rename = "updatedAt")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Subscription {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(
        id: String,
        root_hash: String,
        file_name: String,
        status: models::SubscriptionStatus,
        block_count_downloaded: i64,
        block_count_total: i64,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Subscription {
        Subscription {
            id,
            root_hash,
            file_name,
            status,
            rank: None,
            block_count_downloaded,
            block_count_total,
            failed_reason: None,
            created_at,
            updated_at,
        }
    }
}

/// Converts the Subscription value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            Some("id".to_string()),
            Some(self.id.to_string()),
            Some("rootHash".to_string()),
            Some(self.root_hash.to_string()),
            Some("fileName".to_string()),
            Some(self.file_name.to_string()),
            // Skipping status in query parameter serialization
            self.rank.as_ref().map(|rank| ["rank".to_string(), rank.to_string()].join(",")),
            Some("blockCountDownloaded".to_string()),
            Some(self.block_count_downloaded.to_string()),
            Some("blockCountTotal".to_string()),
            Some(self.block_count_total.to_string()),
            self.failed_reason
                .as_ref()
                .map(|failed_reason| ["failedReason".to_string(), failed_reason.to_string()].join(",")),
            // Skipping createdAt in query parameter serialization

            // Skipping updatedAt in query parameter serialization
        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a Subscription value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for Subscription {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub id: Vec<String>,
            pub root_hash: Vec<String>,
            pub file_name: Vec<String>,
            pub status: Vec<models::SubscriptionStatus>,
            pub rank: Vec<i64>,
            pub block_count_downloaded: Vec<i64>,
            pub block_count_total: Vec<i64>,
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
                None => return std::result::Result::Err("Missing value while parsing Subscription".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    #[allow(clippy::redundant_clone)]
                    "id" => intermediate_rep.id.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "rootHash" => intermediate_rep.root_hash.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "fileName" => intermediate_rep.file_name.push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "status" => intermediate_rep
                        .status
                        .push(<models::SubscriptionStatus as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "rank" => intermediate_rep.rank.push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "blockCountDownloaded" => intermediate_rep
                        .block_count_downloaded
                        .push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "blockCountTotal" => intermediate_rep
                        .block_count_total
                        .push(<i64 as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "failedReason" => intermediate_rep
                        .failed_reason
                        .push(<String as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "createdAt" => intermediate_rep
                        .created_at
                        .push(<chrono::DateTime<chrono::Utc> as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    #[allow(clippy::redundant_clone)]
                    "updatedAt" => intermediate_rep
                        .updated_at
                        .push(<chrono::DateTime<chrono::Utc> as std::str::FromStr>::from_str(val).map_err(|x| x.to_string())?),
                    _ => return std::result::Result::Err("Unexpected key while parsing Subscription".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(Subscription {
            id: intermediate_rep.id.into_iter().next().ok_or_else(|| "id missing in Subscription".to_string())?,
            root_hash: intermediate_rep
                .root_hash
                .into_iter()
                .next()
                .ok_or_else(|| "rootHash missing in Subscription".to_string())?,
            file_name: intermediate_rep
                .file_name
                .into_iter()
                .next()
                .ok_or_else(|| "fileName missing in Subscription".to_string())?,
            status: intermediate_rep.status.into_iter().next().ok_or_else(|| "status missing in Subscription".to_string())?,
            rank: intermediate_rep.rank.into_iter().next(),
            block_count_downloaded: intermediate_rep
                .block_count_downloaded
                .into_iter()
                .next()
                .ok_or_else(|| "blockCountDownloaded missing in Subscription".to_string())?,
            block_count_total: intermediate_rep
                .block_count_total
                .into_iter()
                .next()
                .ok_or_else(|| "blockCountTotal missing in Subscription".to_string())?,
            failed_reason: intermediate_rep.failed_reason.into_iter().next(),
            created_at: intermediate_rep
                .created_at
                .into_iter()
                .next()
                .ok_or_else(|| "createdAt missing in Subscription".to_string())?,
            updated_at: intermediate_rep
                .updated_at
                .into_iter()
                .next()
                .ok_or_else(|| "updatedAt missing in Subscription".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<Subscription> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<Subscription>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<Subscription>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for Subscription - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<Subscription> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <Subscription as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into Subscription - {err}"#)),
            },
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Unable to convert header: {hdr_value:?} to string: {e}"#)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, validator::Validate)]
#[cfg_attr(feature = "conversion", derive(frunk::LabelledGeneric))]
pub struct SubscriptionList {
    #[serde(rename = "subscriptions")]
    #[validate(nested)]
    pub subscriptions: Vec<models::Subscription>,
}

impl SubscriptionList {
    #[allow(clippy::new_without_default, clippy::too_many_arguments)]
    pub fn new(subscriptions: Vec<models::Subscription>) -> SubscriptionList {
        SubscriptionList { subscriptions }
    }
}

/// Converts the SubscriptionList value to the Query Parameters representation (style=form, explode=false)
/// specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde serializer
impl std::fmt::Display for SubscriptionList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let params: Vec<Option<String>> = vec![
            // Skipping subscriptions in query parameter serialization

        ];

        write!(f, "{}", params.into_iter().flatten().collect::<Vec<_>>().join(","))
    }
}

/// Converts Query Parameters representation (style=form, explode=false) to a SubscriptionList value
/// as specified in https://swagger.io/docs/specification/serialization/
/// Should be implemented in a serde deserializer
impl std::str::FromStr for SubscriptionList {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        /// An intermediate representation of the struct to use for parsing.
        #[derive(Default)]
        #[allow(dead_code)]
        struct IntermediateRep {
            pub subscriptions: Vec<Vec<models::Subscription>>,
        }

        let mut intermediate_rep = IntermediateRep::default();

        // Parse into intermediate representation
        let mut string_iter = s.split(',');
        let mut key_result = string_iter.next();

        while key_result.is_some() {
            let val = match string_iter.next() {
                Some(x) => x,
                None => return std::result::Result::Err("Missing value while parsing SubscriptionList".to_string()),
            };

            if let Some(key) = key_result {
                #[allow(clippy::match_single_binding)]
                match key {
                    "subscriptions" => return std::result::Result::Err("Parsing a container in this style is not supported in SubscriptionList".to_string()),
                    _ => return std::result::Result::Err("Unexpected key while parsing SubscriptionList".to_string()),
                }
            }

            // Get the next key
            key_result = string_iter.next();
        }

        // Use the intermediate representation to return the struct
        std::result::Result::Ok(SubscriptionList {
            subscriptions: intermediate_rep
                .subscriptions
                .into_iter()
                .next()
                .ok_or_else(|| "subscriptions missing in SubscriptionList".to_string())?,
        })
    }
}

// Methods for converting between header::IntoHeaderValue<SubscriptionList> and HeaderValue

#[cfg(feature = "server")]
impl std::convert::TryFrom<header::IntoHeaderValue<SubscriptionList>> for HeaderValue {
    type Error = String;

    fn try_from(hdr_value: header::IntoHeaderValue<SubscriptionList>) -> std::result::Result<Self, Self::Error> {
        let hdr_value = hdr_value.to_string();
        match HeaderValue::from_str(&hdr_value) {
            std::result::Result::Ok(value) => std::result::Result::Ok(value),
            std::result::Result::Err(e) => std::result::Result::Err(format!(r#"Invalid header value for SubscriptionList - value: {hdr_value} is invalid {e}"#)),
        }
    }
}

#[cfg(feature = "server")]
impl std::convert::TryFrom<HeaderValue> for header::IntoHeaderValue<SubscriptionList> {
    type Error = String;

    fn try_from(hdr_value: HeaderValue) -> std::result::Result<Self, Self::Error> {
        match hdr_value.to_str() {
            std::result::Result::Ok(value) => match <SubscriptionList as std::str::FromStr>::from_str(value) {
                std::result::Result::Ok(value) => std::result::Result::Ok(header::IntoHeaderValue(value)),
                std::result::Result::Err(err) => std::result::Result::Err(format!(r#"Unable to convert header value '{value}' into SubscriptionList - {err}"#)),
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
pub enum SubscriptionStatus {
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

impl validator::Validate for SubscriptionStatus {
    fn validate(&self) -> std::result::Result<(), validator::ValidationErrors> {
        std::result::Result::Ok(())
    }
}

impl std::fmt::Display for SubscriptionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            SubscriptionStatus::Downloading => write!(f, "Downloading"),
            SubscriptionStatus::Decoding => write!(f, "Decoding"),
            SubscriptionStatus::Finalizing => write!(f, "Finalizing"),
            SubscriptionStatus::Completed => write!(f, "Completed"),
            SubscriptionStatus::Failed => write!(f, "Failed"),
            SubscriptionStatus::Canceled => write!(f, "Canceled"),
        }
    }
}

impl std::str::FromStr for SubscriptionStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "Downloading" => std::result::Result::Ok(SubscriptionStatus::Downloading),
            "Decoding" => std::result::Result::Ok(SubscriptionStatus::Decoding),
            "Finalizing" => std::result::Result::Ok(SubscriptionStatus::Finalizing),
            "Completed" => std::result::Result::Ok(SubscriptionStatus::Completed),
            "Failed" => std::result::Result::Ok(SubscriptionStatus::Failed),
            "Canceled" => std::result::Result::Ok(SubscriptionStatus::Canceled),
            _ => std::result::Result::Err(format!(r#"Value not valid: {s}"#)),
        }
    }
}
