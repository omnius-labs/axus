use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::Parser as _;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use omnius_core_base::file_lock::FileLock;

use crate::{
    config::{DaemonConfig, LoggingConfig},
    prelude::*,
    server::ApiServer,
    state::DaemonState,
};

#[derive(clap::Parser)]
#[command(name = "axus-daemon", about = "xxx", version = env!("GIT_TAG"))]
struct Args {
    #[command(subcommand)]
    command: SubCommand,
}

fn default_config_dir() -> PathBuf {
    std::env::var_os("AXUS_DAEMON_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| get_home_dir().map(|p| p.join(".config/axus")))
        .unwrap_or_else(|| PathBuf::from(".config/axus"))
}

fn get_home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    return std::env::var_os("USERPROFILE").map(PathBuf::from);

    #[cfg(target_os = "linux")]
    return std::env::var_os("HOME").map(PathBuf::from);

    #[cfg(target_os = "macos")]
    return std::env::var_os("HOME").map(PathBuf::from);
}

#[derive(Debug, clap::Subcommand)]
enum SubCommand {
    Start {
        #[arg(long, default_value_os_t = default_config_dir())]
        config_dir: PathBuf,
    },
}

pub struct Executor;

impl Executor {
    pub async fn run() -> Result<()> {
        let args = Args::parse();
        Self::execute(args).await
    }

    fn tracing_config(conf: &LoggingConfig) {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(format!("{},sqlx=off", conf.level)));
        let fmt = tracing_subscriber::fmt().with_env_filter(filter);

        if conf.json {
            fmt.json().init();
        } else {
            fmt.init();
        }
    }

    async fn execute(args: Args) -> Result<()> {
        match args.command {
            SubCommand::Start { config_dir } => Self::handle_start(config_dir).await?,
        }

        Ok(())
    }

    async fn handle_start(config_dir: impl AsRef<Path>) -> Result<()> {
        let config_dir = config_dir.as_ref();

        let _lock = FileLock::acquire(config_dir.join("axus.lock")).await?;

        let token = CancellationToken::new();
        _ = Self::spawn_signal_handler(&token)?;

        let conf = DaemonConfig::load(&config_dir).await.inspect_err(|error| {
            eprintln!("failed to load daemon config from {}: {error:?}", config_dir.display());
        })?;

        Self::tracing_config(&conf.logging);

        let state = Arc::new(DaemonState::new(conf).await?);

        if let Err(error) = ApiServer::serve(state.clone(), token).await {
            error!(?error, "console server stopped");
        }

        // state.engine.shutdown().await;

        Ok(())
    }

    fn spawn_signal_handler(token: &CancellationToken) -> Result<JoinHandle<()>> {
        let token = token.clone();

        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};

            let mut terminate = signal(SignalKind::terminate())?;
            let mut interrupt = signal(SignalKind::interrupt())?;

            Ok(tokio::spawn(async move {
                tokio::select! {
                    result = tokio::signal::ctrl_c() => {
                        if result.is_ok() {
                            token.cancel();
                        }
                    }
                    _ = terminate.recv() => {
                        token.cancel();
                    }
                    _ = interrupt.recv() => {
                        token.cancel();
                    }
                    _ = token.cancelled() => {}
                }
            }))
        }

        #[cfg(not(unix))]
        {
            Ok(tokio::spawn(async move {
                tokio::select! {
                    result = tokio::signal::ctrl_c() => {
                        if result.is_ok() {
                            cancellation.cancel();
                        }
                    }
                    _ = cancellation.cancelled() => {}
                }
            }))
        }
    }
}
