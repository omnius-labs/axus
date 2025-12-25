mod base;
mod core;
mod error;
pub mod model;
mod prelude;
pub mod service;

mod result {
    #[allow(unused)]
    pub type Result<T> = std::result::Result<T, crate::error::Error>;
}

pub use error::*;
pub use result::*;
