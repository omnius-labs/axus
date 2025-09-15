use std::path::PathBuf;

use clap::Parser as _;
use omnius_core_base::error::OmniErrorBuilder;
use shared::{AppConfig, AppInfo, AppState};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use valuable::Valuable;

mod error;
mod interface;
mod prelude;
mod result;
mod shared;

pub use error::*;
pub use result::*;

const APP_NAME: &str = "axus-daemon";

#[derive(clap::Parser)]
struct Opts {
    #[clap(short = 'c', long = "config", default_value = "axus-config.toml")]
    config_path: PathBuf,
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=off"));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).json().init();

    info!("----- start -----");

    if let Err(e) = run().await {
        error!(error = ?e, "unexpected error");
    }

    info!("----- end -----");
}

async fn run() -> Result<()> {
    let info = AppInfo::new(APP_NAME)?;
    info!(info = info.as_value());

    let opts = Opts::parse();
    if !opts.config_path.is_file() {
        return Err(Error::builder()
            .kind(ErrorKind::NotFound)
            .message(format!("Config file not found: {}", opts.config_path.display()))
            .build());
    }

    let conf = AppConfig::load(opts.config_path).await?;

    let state = AppState::new(info, conf).await?;
    interface::RpcServer::serve(state).await?;

    Ok(())
}
