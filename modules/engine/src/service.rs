use std::{path::Path, sync::Arc};

use chrono::Utc;
use parking_lot::Mutex;
use tokio::sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc};

use omnius_core_base::{
    clock::{Clock, ClockUtc},
    random_bytes::RandomBytesProviderImpl,
    sleeper::{Sleeper, SleeperImpl},
};
use omnius_core_omnikit::model::{OmniAddr, OmniSignType, OmniSigner};

use crate::{
    core::{
        connection::{
            ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnector, ConnectionTcpConnectorImpl, TcpProxyOption, TcpProxyType,
        },
        negotiator::{NodeFinder, NodeFinderOption, NodeFinderRepo, NodeProfileFetcher, NodeProfileFetcherImpl},
        session::{SessionAccepter, SessionConnector},
    },
    model::NodeProfile,
    prelude::*,
};

struct AxusEngine {
    node_finder: NodeFinder,
}

impl AxusEngine {
    pub async fn new() -> Result<Self> {
        Ok(AxusEngine {
            node_finder: Self::create_node_finder(Path::new("aaa"), 6666).await?,
        })
    }

    async fn create_node_finder(dir_path: &Path, port: u16) -> Result<NodeFinder> {
        let tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync> =
            Arc::new(ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, port), false).await?);
        let tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync> = Arc::new(
            ConnectionTcpConnectorImpl::new(TcpProxyOption {
                typ: TcpProxyType::None,
                addr: None,
            })
            .await?,
        );

        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let sleeper: Arc<dyn Sleeper + Send + Sync> = Arc::new(SleeperImpl);
        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "TODO")?);
        let random_bytes_provider = Arc::new(Mutex::new(RandomBytesProviderImpl::new()));

        let session_accepter =
            Arc::new(SessionAccepter::new(tcp_accepter.clone(), signer.clone(), random_bytes_provider.clone(), sleeper.clone()).await);
        let session_connector = Arc::new(SessionConnector::new(tcp_connector.clone(), signer, random_bytes_provider));

        let node_ref_repo_dir = dir_path.join("repo");
        tokio::fs::create_dir_all(&node_ref_repo_dir).await?;

        let node_profile_repo = Arc::new(NodeFinderRepo::new(node_ref_repo_dir.as_os_str().to_str().unwrap(), clock.clone()).await?);

        let node_profile_fetcher = Arc::new(NodeProfileFetcherImpl::new(&[]));
        let node_finder_dir = dir_path.join("finder");
        tokio::fs::create_dir_all(&node_finder_dir).await?;

        let result = NodeFinder::new(
            tcp_connector,
            tcp_accepter,
            session_connector,
            session_accepter,
            node_profile_repo,
            node_profile_fetcher,
            clock,
            sleeper,
            NodeFinderOption {
                state_dir_path: node_finder_dir.as_os_str().to_str().unwrap().to_string(),
                max_connected_session_count: 3,
                max_accepted_session_count: 3,
            },
        )
        .await;

        Ok(result)
    }
}
