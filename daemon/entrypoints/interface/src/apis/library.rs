use async_trait::async_trait;
use axum::extract::*;
use axum_extra::extract::CookieJar;
use bytes::Bytes;
use headers::Host;
use http::Method;
use serde::{Deserialize, Serialize};

use crate::{models, types::*};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum ListLibraryEntriesResponse {
    /// Entries of the directory.
    Status200_EntriesOfTheDirectory(models::LibraryEntryList),
    /// The request is malformed.
    Status400_TheRequestIsMalformed(models::ErrorResponse),
    /// The resource does not exist.
    Status404_TheResourceDoesNotExist(models::ErrorResponse),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[must_use]
#[allow(clippy::large_enum_variant)]
pub enum ListLibraryRootsResponse {
    /// Library roots.
    Status200_LibraryRoots(models::LibraryRootList),
    /// Internal server error.
    Status500_InternalServerError(models::ErrorResponse),
}

/// Library
#[async_trait]
#[allow(clippy::ptr_arg)]
pub trait Library<E: std::fmt::Debug + Send + Sync + 'static = ()>: super::ErrorHandler<E> {
    /// List one directory of a library root.
    ///
    /// ListLibraryEntries - GET /api/v1/library/roots/{rootId}/entries
    async fn list_library_entries(
        &self,

        method: &Method,
        host: &Host,
        cookies: &CookieJar,
        path_params: &models::ListLibraryEntriesPathParams,
        query_params: &models::ListLibraryEntriesQueryParams,
    ) -> Result<ListLibraryEntriesResponse, E>;

    /// List the configured library roots.
    ///
    /// ListLibraryRoots - GET /api/v1/library/roots
    async fn list_library_roots(&self, method: &Method, host: &Host, cookies: &CookieJar) -> Result<ListLibraryRootsResponse, E>;
}
