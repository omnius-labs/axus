mod base;
mod core;
pub mod engine;
mod error;
pub mod model;
mod prelude;

mod result {
    #[allow(unused)]
    pub type Result<T> = std::result::Result<T, crate::error::Error>;
}

pub use error::*;
pub use result::*;
