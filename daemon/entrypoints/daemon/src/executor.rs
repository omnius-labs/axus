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

// omnius-lint:debt(free-fn) Executor の関連関数へ移す変更を、規約の導入と分ける
fn default_config_dir() -> PathBuf {
    std::env::var_os("AXUS_DAEMON_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| get_home_dir().map(|p| p.join(".config/axus")))
        .unwrap_or_else(|| PathBuf::from(".config/axus"))
}

// omnius-lint:debt(free-fn) OS ごとの home directory を包む境界を Executor の関連関数へ移す変更を、規約の導入と分ける
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

        let conf = DaemonConfig::load(&config_dir).await.inspect_err(|error| {
            eprintln!("failed to load daemon config from {}: {error:?}", config_dir.display());
        })?;

        let _lock = Self::acquire_state_lock(&conf.core.state_dir).await?;

        let token = CancellationToken::new();
        _ = Self::spawn_signal_handler(&token)?;

        Self::tracing_config(&conf.logging);

        let state = Arc::new(DaemonState::new(conf).await?);

        if let Err(error) = ApiServer::serve(state.clone(), token).await {
            error!(?error, "console server stopped");
        }

        state.shutdown().await;

        Ok(())
    }

    async fn acquire_state_lock(state_dir: &Path) -> Result<FileLock> {
        tokio::fs::create_dir_all(state_dir)
            .await
            .map_err(|error| Error::from_error(error, ErrorKind::IoError).with_message(format!("failed to create state directory {}", state_dir.display())))?;

        let lock_path = state_dir.join("axus.lock");
        FileLock::acquire(&lock_path)
            .await
            .map_err(|error| Error::from_error(error, ErrorKind::IoError).with_message(format!("failed to acquire daemon lock at {}", lock_path.display())))
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
                            token.cancel();
                        }
                    }
                    _ = token.cancelled() => {}
                }
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use testresult::TestResult;

    use super::*;

    #[tokio::test]
    async fn state_lock_rejects_another_config_using_the_same_state_directory() -> TestResult {
        let dir = tempfile::tempdir()?;
        let first_config_dir = dir.path().join("first-config");
        let second_config_dir = dir.path().join("second-config");
        let toml = r#"
            [core]
            state_dir = "../state"

            [api]
            listen_addr = "127.0.0.1:6051"

            [p2p]
            listen_addr = "127.0.0.1:6052"

            [logging]
            level = "info"
            json = false
        "#;
        for config_dir in [&first_config_dir, &second_config_dir] {
            tokio::fs::create_dir_all(config_dir).await?;
            tokio::fs::write(config_dir.join("axus.toml"), toml).await?;
        }
        let first_conf = DaemonConfig::load(&first_config_dir).await?;
        let second_conf = DaemonConfig::load(&second_config_dir).await?;

        assert!(!first_conf.core.state_dir.exists());
        let _lock = Executor::acquire_state_lock(&first_conf.core.state_dir).await?;
        assert!(first_conf.core.state_dir.join("axus.lock").is_file());
        assert!(!first_config_dir.join("axus.lock").exists());
        assert!(!second_config_dir.join("axus.lock").exists());
        assert_eq!(first_conf.core.state_dir.canonicalize()?, second_conf.core.state_dir.canonicalize()?);

        let error = Executor::acquire_state_lock(&second_conf.core.state_dir)
            .await
            .expect_err("second daemon should fail to acquire the state lock");
        assert_eq!(*error.kind(), ErrorKind::IoError);
        let source = error.source().unwrap().downcast_ref::<std::io::Error>().unwrap();
        assert_eq!(source.kind(), std::io::ErrorKind::AlreadyExists);

        Ok(())
    }

    #[tokio::test]
    async fn state_lock_reports_state_directory_creation_failure() -> TestResult {
        let dir = tempfile::tempdir()?;
        let state_file = dir.path().join("state-file");
        tokio::fs::write(&state_file, b"").await?;
        let state_dir = state_file.join("state");

        let error = Executor::acquire_state_lock(&state_dir)
            .await
            .expect_err("state directory cannot be created beneath a regular file");
        assert_eq!(*error.kind(), ErrorKind::IoError);
        assert_eq!(error.message(), Some(format!("failed to create state directory {}", state_dir.display()).as_str()));
        assert!(error.source().unwrap().downcast_ref::<std::io::Error>().is_some());

        Ok(())
    }
}
