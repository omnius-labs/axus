mod features;
mod server;

#[cfg(test)]
mod rest_api_tests;

pub use server::*;

#[macro_export]
macro_rules! impl_api_response {
    ($response:ty; $($status:ident => $variant:ident),* $(,)?) => {
        impl From<$crate::error::ApiError> for $response {
            fn from(error: $crate::error::ApiError) -> Self {
                match error.status {
                    $($crate::error::ApiStatus::$status => <$response>::$variant(error.body),)*
                    _ => <$response>::Status500_InternalServerError(error.body),
                }
            }
        }
    };
}
