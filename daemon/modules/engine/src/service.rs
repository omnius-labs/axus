use std::{path::Path, sync::Arc};

use chrono::Utc;
use parking_lot::Mutex;

use omnius_core_base::{
    clock::{Clock, ClockUtc},
    sleeper::{Sleeper, SleeperImpl},
};
use omnius_core_omnikit::model::omni_addr::OmniAddr;
use rand::{
    SeedableRng,
    rngs::{ChaCha20Rng, SysRng},
};
use rand_core::UnwrapErr;

use crate::{
    base::{
        connection::{ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnector, ConnectionTcpConnectorImpl, TcpProxyOption, TcpProxyType},
        runtime::Shutdown,
    },
    core::{
        identity::NodeIdentity,
        negotiator::{NodeFinder, NodeFinderIntervals, NodeFinderOption, NodeFinderRepo, NodeProfileFetcherImpl},
        session::{SessionAccepter, SessionConnector, model::SessionType},
    },
    model::NodeProfile,
    prelude::*,
};

pub struct AxusService {
    node_finder: NodeFinder,
}

#[derive(Debug, Clone, Default)]
pub struct AxusServiceOption {
    pub bootstrap_node_profiles: Vec<NodeProfile>,
    pub node_finder_intervals: NodeFinderIntervals,
}

impl AxusService {
    pub async fn new<S: AsRef<Path>, A: AsRef<str>, T: AsRef<Path>>(state_dir: S, listen_addr: A, _temp_dir: T, option: AxusServiceOption) -> Result<Self> {
        Ok(AxusService {
            node_finder: Self::create_node_finder(state_dir.as_ref(), listen_addr.as_ref(), option).await?,
        })
    }

    async fn create_node_finder(state_dir: &Path, listen_addr: &str, option: AxusServiceOption) -> Result<NodeFinder> {
        let tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync> = Arc::new(ConnectionTcpAccepterImpl::new(&OmniAddr::from_host_and_port_str(listen_addr)?, false).await?);
        let tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync> = Arc::new(
            ConnectionTcpConnectorImpl::new(TcpProxyOption {
                typ: TcpProxyType::None,
                addr: None,
            })
            .await?,
        );

        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let sleeper: Arc<dyn Sleeper + Send + Sync> = Arc::new(SleeperImpl);
        let identity = NodeIdentity::load_or_create(&state_dir.join("identity")).await?;
        let signer = identity.signer();
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));

        let session_accepter = Arc::new(SessionAccepter::new(tcp_accepter.clone(), signer.clone(), sleeper.clone(), rng.clone(), &[SessionType::NodeFinder]).await);
        let session_connector = Arc::new(SessionConnector::new(tcp_connector.clone(), signer, rng.clone()));

        let node_ref_repo_dir = state_dir.join("repo");
        tokio::fs::create_dir_all(&node_ref_repo_dir).await?;

        let node_profile_repo = Arc::new(NodeFinderRepo::new(node_ref_repo_dir.as_os_str().to_str().unwrap(), clock.clone()).await?);
        let bootstrap_node_profiles: Vec<&NodeProfile> = option.bootstrap_node_profiles.iter().collect();
        node_profile_repo.insert_or_ignore_node_profiles(&bootstrap_node_profiles, 0).await?;

        let node_profile_fetcher = Arc::new(NodeProfileFetcherImpl::new(&[]));
        let node_finder_dir = state_dir.join("finder");
        tokio::fs::create_dir_all(&node_finder_dir).await?;

        let my_node_profile = NodeProfile::new(identity.public_key().to_vec(), Vec::new());

        let result = NodeFinder::new(
            my_node_profile,
            session_connector,
            session_accepter,
            node_profile_repo,
            node_profile_fetcher,
            clock,
            sleeper,
            rng,
            NodeFinderOption {
                state_dir: node_finder_dir.as_os_str().to_str().unwrap().to_string(),
                max_connected_session_count: 3,
                max_accepted_session_count: 3,
                intervals: option.node_finder_intervals,
            },
        )
        .await?;

        Ok(result)
    }

    pub async fn shutdown(&self) {
        self.node_finder.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use std::{net::TcpListener, time::Duration};

    use testresult::TestResult;

    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        core::{identity::NodeIdentity, negotiator::NodeFinderIntervals},
        model::{AssetKey, NodeProfile},
        prelude::*,
    };

    use super::{AxusService, AxusServiceOption};

    const TEST_TIMEOUT: Duration = Duration::from_secs(30);

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn node_finds_asset_key_owner_through_bootstrap_node() -> TestResult {
        let dir = tempfile::tempdir()?;

        let owner_port = free_port()?;
        let owner = AxusService::new(dir.path().join("owner"), format!("127.0.0.1:{owner_port}"), dir.path(), fast_option(vec![])).await?;
        let owner_id = owner.node_finder.my_node_profile().id().to_vec();

        let owner_node_profile = NodeProfile::new(
            owner.node_finder.my_node_profile().public_key().to_vec(),
            vec![OmniAddr::create_tcp("127.0.0.1".parse()?, owner_port)],
        );
        let seeker_port = free_port()?;
        let seeker = AxusService::new(
            dir.path().join("seeker"),
            format!("127.0.0.1:{seeker_port}"),
            dir.path(),
            fast_option(vec![owner_node_profile]),
        )
        .await?;

        // Kadex は自ノードより近いノードにだけ want を送るため、owner を最も近いノードにする
        let asset_key = AssetKey {
            typ: "test".to_string(),
            hash: OmniHash {
                typ: OmniHashAlgorithmType::Sha3_256,
                value: owner_id.clone(),
            },
        };
        let _push_handle = owner.node_finder.listen_push_asset_keys({
            let asset_key = asset_key.clone();
            move |_| vec![asset_key.clone()]
        });
        let _want_handle = seeker.node_finder.listen_want_asset_keys({
            let asset_key = asset_key.clone();
            move |_| vec![asset_key.clone()]
        });

        let found = tokio::time::timeout(TEST_TIMEOUT, async {
            loop {
                let node_profiles = seeker.node_finder.find_node_profile(&asset_key).await?;
                if !node_profiles.is_empty() {
                    return Result::Ok(node_profiles);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await??;

        assert!(found.iter().all(|node_profile| node_profile.id() == owner_id.as_slice()));
        assert_eq!(seeker.node_finder.get_session_count().await, 1);
        assert_eq!(owner.node_finder.get_session_count().await, 1);

        seeker.shutdown().await;
        owner.shutdown().await;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn nodes_dialing_each_other_keep_one_session() -> TestResult {
        // 同時に接続し合う状況は起動の時刻に左右されるため、何度か繰り返す
        for _ in 0..5 {
            let dir = tempfile::tempdir()?;
            let a_dir = dir.path().join("a");
            let b_dir = dir.path().join("b");
            let (a_port, b_port) = (free_port()?, free_port()?);

            // 互いを bootstrap に指定するため、起動の前に鍵を作って公開鍵を確定させる
            let a_node_profile = NodeProfile::new(
                NodeIdentity::load_or_create(&a_dir.join("identity")).await?.public_key().to_vec(),
                vec![OmniAddr::create_tcp("127.0.0.1".parse()?, a_port)],
            );
            let b_node_profile = NodeProfile::new(
                NodeIdentity::load_or_create(&b_dir.join("identity")).await?.public_key().to_vec(),
                vec![OmniAddr::create_tcp("127.0.0.1".parse()?, b_port)],
            );

            let (a, b) = tokio::try_join!(
                AxusService::new(&a_dir, format!("127.0.0.1:{a_port}"), dir.path(), fast_option(vec![b_node_profile])),
                AxusService::new(&b_dir, format!("127.0.0.1:{b_port}"), dir.path(), fast_option(vec![a_node_profile])),
            )?;

            tokio::time::timeout(TEST_TIMEOUT, async {
                while a.node_finder.get_session_count().await != 1 || b.node_finder.get_session_count().await != 1 {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await?;

            // 重複した Session を閉じた後も、残した 1 本が閉じずに続くことを確かめる
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_millis(100)).await;
                assert_eq!(a.node_finder.get_session_count().await, 1);
                assert_eq!(b.node_finder.get_session_count().await, 1);
            }

            a.shutdown().await;
            b.shutdown().await;
        }

        Ok(())
    }

    fn fast_option(bootstrap_node_profiles: Vec<NodeProfile>) -> AxusServiceOption {
        AxusServiceOption {
            bootstrap_node_profiles,
            node_finder_intervals: NodeFinderIntervals {
                connect: Duration::from_millis(100),
                compute: Duration::from_millis(100),
                communicate: Duration::from_millis(100),
            },
        }
    }

    fn free_port() -> Result<u16> {
        Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
    }
}
