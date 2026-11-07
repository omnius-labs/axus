use std::process::ExitCode;

use crate::{executor::Executor, prelude::*};

mod config;
mod error;
mod executor;
mod library;
mod owner;
mod prelude;
mod result;
mod server;
mod state;

pub use error::*;
pub use result::*;

// RUST_LOG=trace RUST_BACKTRACE=full cargo run

#[tokio::main]
async fn main() -> ExitCode {
    match Executor::run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!(error = ?e, "unexpected error");
            ExitCode::FAILURE
        }
    }
}
#[cfg(test)]
mod scratch {
    #[test]
    fn first() {
        panic!("first");
    }
}
