use std::{collections::HashMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use parking_lot::Mutex;
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc};

use omnius_core_base::{clock::Clock, sleeper::Sleeper, tsid::TsidProvider};

use crate::{
    base::{collections::VolatileHashSet, runtime::Shutdown},
    core::{
        negotiator::NodeFinder,
        session::{SessionAccepter, SessionConnector},
    },
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

#[derive(Debug, Clone)]
pub struct FileExchangerOption {
    pub state_dir: PathBuf,
    pub max_connected_session_for_publish_count: usize,
    pub max_connected_session_for_subscribe_count: usize,
    pub max_accepted_session_count: usize,
}

pub struct FileExchanger {
    session_connector: Arc<SessionConnector>,
    session_accepter: Arc<SessionAccepter>,
    node_finder: Arc<NodeFinder>,
    tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
    option: FileExchangerOption,

    // SessionStatus の受信処理は未実装である。
    #[allow(dead_code)]
    session_receiver: Arc<TokioMutex<mpsc::Receiver<SessionStatus>>>,
    session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
    sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
    connected_node_profiles: Arc<Mutex<VolatileHashSet<Arc<NodeProfile>>>>,
    // 公開する asset key の管理は未実装である。
    #[allow(dead_code)]
    push_asset_keys: Arc<Mutex<Vec<AssetKey>>>,
    // 購読する asset key の管理は未実装である。
    #[allow(dead_code)]
    want_asset_keys: Arc<Mutex<Vec<AssetKey>>>,

    file_publisher: Arc<TokioMutex<Option<Arc<FilePublisher>>>>,
    file_subscriber: Arc<TokioMutex<Option<Arc<FileSubscriber>>>>,

    task_connectors: Arc<TokioMutex<Vec<Arc<TaskConnector>>>>,
    task_acceptors: Arc<TokioMutex<Vec<Arc<TaskAccepter>>>>,
}

#[async_trait]
impl Shutdown for FileExchanger {
    async fn shutdown(&self) {
        let connectors = std::mem::take(&mut *self.task_connectors.lock().await);
        for task in connectors {
            task.shutdown().await;
        }
        let acceptors = std::mem::take(&mut *self.task_acceptors.lock().await);
        for task in acceptors {
            task.shutdown().await;
        }
        let publisher = self.file_publisher.lock().await.take();
        if let Some(publisher) = publisher {
            publisher.shutdown().await;
        }
        let subscriber = self.file_subscriber.lock().await.take();
        if let Some(subscriber) = subscriber {
            subscriber.shutdown().await;
        }
    }
}

impl FileExchanger {
    // AxusService への結線が未実装のため、constructor はまだ呼ばれない。
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        session_connector: Arc<SessionConnector>,
        session_accepter: Arc<SessionAccepter>,
        node_finder: Arc<NodeFinder>,
        tsid_provider: Arc<Mutex<dyn TsidProvider + Send + Sync>>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
        option: FileExchangerOption,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::channel(20);

        let v = Self {
            session_connector,
            session_accepter,
            node_finder,
            tsid_provider,
            clock: clock.clone(),
            sleeper,
            rng,
            option,

            session_receiver: Arc::new(TokioMutex::new(rx)),
            session_sender: Arc::new(TokioMutex::new(tx)),
            sessions: Arc::new(TokioRwLock::new(HashMap::new())),
            connected_node_profiles: Arc::new(Mutex::new(VolatileHashSet::new(Duration::seconds(180), clock))),
            push_asset_keys: Arc::new(Mutex::new(vec![])),
            want_asset_keys: Arc::new(Mutex::new(vec![])),

            file_publisher: Arc::new(TokioMutex::new(None)),
            file_subscriber: Arc::new(TokioMutex::new(None)),

            task_connectors: Arc::new(TokioMutex::new(Vec::new())),
            task_acceptors: Arc::new(TokioMutex::new(Vec::new())),
        };
        if let Err(error) = v.start().await {
            v.shutdown().await;
            return Err(error);
        }

        Ok(v)
    }

    async fn start(&self) -> Result<()> {
        {
            let state_dir = self.option.state_dir.join("file_publisher");
            let file_publisher = FilePublisher::new(&state_dir, self.tsid_provider.clone(), self.clock.clone(), self.sleeper.clone()).await?;
            self.file_publisher.lock().await.replace(file_publisher);
        }

        {
            let state_dir = self.option.state_dir.join("file_subscriber");
            let file_subscriber = FileSubscriber::new(&state_dir, self.tsid_provider.clone(), self.clock.clone(), self.sleeper.clone()).await?;
            self.file_subscriber.lock().await.replace(file_subscriber);
        }

        for _ in 0..3 {
            let task = TaskConnector::new(
                self.sessions.clone(),
                self.session_sender.clone(),
                self.session_connector.clone(),
                self.node_finder.clone(),
                self.file_publisher.clone(),
                self.file_subscriber.clone(),
                self.connected_node_profiles.clone(),
                self.clock.clone(),
                self.sleeper.clone(),
                self.rng.clone(),
                self.option.clone(),
            )
            .await?;
            self.task_connectors.lock().await.push(task);
        }

        for _ in 0..3 {
            let task = TaskAccepter::new(
                self.sessions.clone(),
                self.session_sender.clone(),
                self.session_accepter.clone(),
                self.clock.clone(),
                self.sleeper.clone(),
                self.option.clone(),
            )
            .await?;
            self.task_acceptors.lock().await.push(task);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_base::{clock::ClockUtc, sleeper::SleeperImpl, tsid::TsidProviderImpl};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;
    use rand::{SeedableRng as _, rngs::ChaCha20Rng};
    use testresult::TestResult;

    use crate::{
        base::connection::{ConnectionTcpAccepterImpl, ConnectionTcpConnectorImpl, TcpProxyOption, TcpProxyType},
        core::{
            identity::NodeIdentity,
            negotiator::{NodeFinderIntervals, NodeFinderOption, NodeFinderRepo, NodeProfileFetcherImpl},
            session::model::SessionType,
        },
    };

    use super::*;

    #[tokio::test]
    async fn new_fails_when_state_directory_cannot_be_created() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(ClockUtc);
        let sleeper = Arc::new(SleeperImpl);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::seed_from_u64(0)));
        let identity = NodeIdentity::load_or_create(&dir.path().join("identity")).await?;
        let tcp_accepter = Arc::new(ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, 0), false).await?);
        let tcp_connector = Arc::new(
            ConnectionTcpConnectorImpl::new(TcpProxyOption {
                typ: TcpProxyType::None,
                addr: None,
            })
            .await?,
        );
        let session_accepter = Arc::new(
            SessionAccepter::new(
                tcp_accepter,
                identity.signer(),
                sleeper.clone(),
                rng.clone(),
                &[SessionType::NodeFinder, SessionType::FileExchanger],
            )
            .await,
        );
        let session_connector = Arc::new(SessionConnector::new(tcp_connector, identity.signer(), rng.clone()));
        let repo_dir = dir.path().join("repo");
        tokio::fs::create_dir_all(&repo_dir).await?;
        let node_profile_repo = Arc::new(NodeFinderRepo::new(repo_dir.to_str().unwrap(), clock.clone()).await?);
        let node_finder = Arc::new(
            NodeFinder::new(
                NodeProfile::new(identity.public_key().to_vec(), vec![]),
                session_connector.clone(),
                session_accepter.clone(),
                node_profile_repo,
                Arc::new(NodeProfileFetcherImpl::new(&[])),
                clock.clone(),
                sleeper.clone(),
                rng.clone(),
                NodeFinderOption {
                    state_dir: dir.path().join("finder").to_str().unwrap().to_string(),
                    max_connected_session_count: 3,
                    max_accepted_session_count: 3,
                    intervals: NodeFinderIntervals::default(),
                },
            )
            .await?,
        );
        let tsid_provider = Arc::new(Mutex::new(TsidProviderImpl::new(ClockUtc, ChaCha20Rng::seed_from_u64(1), 8)));

        // 通常 file の下には state directory を作れない
        let state_file = dir.path().join("state-file");
        tokio::fs::write(&state_file, b"").await?;
        let result = FileExchanger::new(
            session_connector,
            session_accepter.clone(),
            node_finder.clone(),
            tsid_provider,
            clock,
            sleeper,
            rng,
            FileExchangerOption {
                state_dir: state_file.join("state"),
                max_connected_session_for_publish_count: 3,
                max_connected_session_for_subscribe_count: 3,
                max_accepted_session_count: 3,
            },
        )
        .await;

        if let Ok(exchanger) = &result {
            exchanger.shutdown().await;
        }
        node_finder.shutdown().await;
        session_accepter.shutdown().await;

        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn new_shuts_down_publisher_when_subscriber_start_fails() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(ClockUtc);
        let sleeper = Arc::new(SleeperImpl);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::seed_from_u64(0)));
        let identity = NodeIdentity::load_or_create(&dir.path().join("identity")).await?;
        let tcp_accepter = Arc::new(ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, 0), false).await?);
        let tcp_connector = Arc::new(
            ConnectionTcpConnectorImpl::new(TcpProxyOption {
                typ: TcpProxyType::None,
                addr: None,
            })
            .await?,
        );
        let session_accepter = Arc::new(
            SessionAccepter::new(
                tcp_accepter,
                identity.signer(),
                sleeper.clone(),
                rng.clone(),
                &[SessionType::NodeFinder, SessionType::FileExchanger],
            )
            .await,
        );
        let session_connector = Arc::new(SessionConnector::new(tcp_connector, identity.signer(), rng.clone()));
        let repo_dir = dir.path().join("repo");
        tokio::fs::create_dir_all(&repo_dir).await?;
        let node_profile_repo = Arc::new(NodeFinderRepo::new(repo_dir.to_str().unwrap(), clock.clone()).await?);
        let node_finder = Arc::new(
            NodeFinder::new(
                NodeProfile::new(identity.public_key().to_vec(), vec![]),
                session_connector.clone(),
                session_accepter.clone(),
                node_profile_repo,
                Arc::new(NodeProfileFetcherImpl::new(&[])),
                clock.clone(),
                sleeper.clone(),
                rng.clone(),
                NodeFinderOption {
                    state_dir: dir.path().join("finder").to_str().unwrap().to_string(),
                    max_connected_session_count: 3,
                    max_accepted_session_count: 3,
                    intervals: NodeFinderIntervals::default(),
                },
            )
            .await?,
        );
        let tsid_provider = Arc::new(Mutex::new(TsidProviderImpl::new(ClockUtc, ChaCha20Rng::seed_from_u64(1), 8)));

        // publisher の起動後に、通常 file として置いた subscriber の起動を失敗させる。
        let state_dir = dir.path().join("state");
        tokio::fs::create_dir_all(&state_dir).await?;
        tokio::fs::write(state_dir.join("file_subscriber"), b"").await?;
        let result = FileExchanger::new(
            session_connector,
            session_accepter.clone(),
            node_finder.clone(),
            tsid_provider.clone(),
            clock.clone(),
            sleeper.clone(),
            rng,
            FileExchangerOption {
                state_dir: state_dir.clone(),
                max_connected_session_for_publish_count: 3,
                max_connected_session_for_subscribe_count: 3,
                max_accepted_session_count: 3,
            },
        )
        .await;

        if let Ok(exchanger) = &result {
            exchanger.shutdown().await;
        }
        node_finder.shutdown().await;
        session_accepter.shutdown().await;

        assert!(result.is_err());

        assert!(state_dir.join("file_publisher/blocks/LOCK").is_file());

        // encoder が終了して store を解放していれば、同じ RocksDB を開き直せる。
        let publisher = FilePublisher::new(&state_dir.join("file_publisher"), tsid_provider, clock, sleeper).await?;
        publisher.shutdown().await;

        Ok(())
    }
}
