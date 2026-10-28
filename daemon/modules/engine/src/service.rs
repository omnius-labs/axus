use std::{path::Path, sync::Arc};

use chrono::Utc;
use parking_lot::Mutex;

use omnius_core_base::{
    clock::{Clock, ClockUtc},
    sleeper::{Sleeper, SleeperImpl},
    tsid::TsidProviderImpl,
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
        negotiator::{
            FileExchanger, FileExchangerIntervals, FileExchangerOption, NodeFinder, NodeFinderIntervals, NodeFinderLocationFinder, NodeFinderOption, NodeFinderRepo,
            NodeProfileFetcherImpl,
        },
        session::{
            SessionAccepter, SessionConnector,
            model::{SessionOption, SessionType},
        },
    },
    model::NodeProfile,
    prelude::*,
};

pub struct AxusService {
    node_finder: Arc<NodeFinder>,
    file_exchanger: FileExchanger,
}

#[derive(Debug, Clone, Default)]
pub struct AxusServiceOption {
    pub bootstrap_node_profiles: Vec<NodeProfile>,
    pub node_finder_intervals: NodeFinderIntervals,
    /// 空なら、待ち受けアドレスから広告するアドレスを決める
    pub advertise_addrs: Vec<OmniAddr>,
    pub use_upnp: bool,
}

impl AxusService {
    pub async fn new<S: AsRef<Path>, A: AsRef<str>, T: AsRef<Path>>(state_dir: S, listen_addr: A, _temp_dir: T, option: AxusServiceOption) -> Result<Self> {
        let (node_finder, file_exchanger) = Self::create_components(state_dir.as_ref(), listen_addr.as_ref(), option).await?;
        Ok(AxusService { node_finder, file_exchanger })
    }

    async fn create_components(state_dir: &Path, listen_addr: &str, option: AxusServiceOption) -> Result<(Arc<NodeFinder>, FileExchanger)> {
        // 明示設定の違反は、接続 worker の開始前に返す。
        for profile in &option.bootstrap_node_profiles {
            profile.to_uri()?;
        }
        NodeProfile::validate_addr_byte_lengths(&option.advertise_addrs)?;
        let tcp_accepter = ConnectionTcpAccepterImpl::new(&OmniAddr::from_host_and_port_str(listen_addr)?, option.use_upnp).await?;
        let my_addrs = if option.advertise_addrs.is_empty() {
            tcp_accepter.get_advertised_addrs().await?
        } else {
            option.advertise_addrs.clone()
        };
        if my_addrs.is_empty() {
            warn!("no address to advertise; other nodes can reach this node only through their bootstrap settings");
        }
        let tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync> = Arc::new(tcp_accepter);
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
        let mut my_addrs = my_addrs;
        if my_addrs.len() > NodeProfile::MAX_WIRE_ADDRS {
            warn!(
                address_count = my_addrs.len(),
                max_address_count = NodeProfile::MAX_WIRE_ADDRS,
                "too many addresses to advertise; truncating node profile"
            );
            my_addrs.truncate(NodeProfile::MAX_WIRE_ADDRS);
        }
        let my_node_profile = NodeProfile::sign(&signer, my_addrs, clock.now())?;
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));

        let session_accepter = Arc::new(
            SessionAccepter::new(
                tcp_accepter.clone(),
                signer.clone(),
                clock.clone(),
                sleeper.clone(),
                rng.clone(),
                &[SessionType::NodeFinder, SessionType::FileExchanger],
                SessionOption::default(),
            )
            .await,
        );
        let session_connector = Arc::new(SessionConnector::new(tcp_connector.clone(), signer, clock.clone(), rng.clone(), SessionOption::default()));

        let node_ref_repo_dir = state_dir.join("repo");
        tokio::fs::create_dir_all(&node_ref_repo_dir).await?;

        let node_profile_repo = Arc::new(NodeFinderRepo::new(node_ref_repo_dir.as_os_str().to_str().unwrap(), clock.clone()).await?);
        let bootstrap_node_profiles: Vec<&NodeProfile> = option.bootstrap_node_profiles.iter().collect();
        node_profile_repo.insert_or_update_node_profiles(&bootstrap_node_profiles, 0).await?;

        let node_profile_fetcher = Arc::new(NodeProfileFetcherImpl::new(&[]));
        let node_finder_dir = state_dir.join("finder");
        tokio::fs::create_dir_all(&node_finder_dir).await?;

        let tsid_provider = Arc::new(Mutex::new(TsidProviderImpl::new(ClockUtc, ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)), 8)));
        let node_finder = Arc::new(
            NodeFinder::new(
                my_node_profile,
                session_connector.clone(),
                session_accepter.clone(),
                node_profile_repo,
                node_profile_fetcher,
                clock.clone(),
                sleeper.clone(),
                rng.clone(),
                NodeFinderOption {
                    state_dir: node_finder_dir.as_os_str().to_str().unwrap().to_string(),
                    max_connected_session_count: 3,
                    max_accepted_session_count: 3,
                    intervals: option.node_finder_intervals,
                },
            )
            .await?,
        );

        // 他の node の設定 p2p.bootstrap_nodes に書けるよう、起動時の制約を適用した NodeProfile を記録する
        info!(node_profile = node_finder.my_node_profile().to_uri()?, "node profile");

        let file_exchanger = match FileExchanger::new(
            session_connector,
            session_accepter,
            Arc::new(NodeFinderLocationFinder::new(node_finder.clone())),
            tsid_provider,
            clock,
            sleeper,
            rng,
            FileExchangerOption::new(state_dir.join("file_exchanger")),
            FileExchangerIntervals::default(),
        )
        .await
        {
            Ok(file_exchanger) => file_exchanger,
            Err(error) => {
                node_finder.shutdown().await;
                return Err(error);
            }
        };

        Ok((node_finder, file_exchanger))
    }

    pub async fn shutdown(&self) {
        // FileExchanger uses the Sessions NodeFinder keeps looking nodes up for, so it stops first.
        self.file_exchanger.shutdown().await;
        self.node_finder.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::MessageCodec;
    use std::{net::TcpListener, sync::Arc, time::Duration};

    use chrono::Utc;
    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;

    use testresult::TestResult;

    use omnius_core_base::clock::{Clock, ClockUtc};
    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        base::connection::{ConnectionTcpConnectorImpl, TcpProxyOption, TcpProxyType},
        core::{
            identity::NodeIdentity,
            negotiator::NodeFinderIntervals,
            session::{
                SessionConnector,
                model::{SessionOption, SessionType},
            },
        },
        model::{AssetKey, NodeProfile},
        prelude::*,
    };

    use super::{AxusService, AxusServiceOption};

    const TEST_TIMEOUT: Duration = Duration::from_secs(30);

    #[tokio::test]
    async fn oversized_configuration_fails_before_starting_workers() -> TestResult {
        let dir = tempfile::tempdir()?;
        for option in [AxusServiceOption {
            advertise_addrs: vec![OmniAddr::new("a".repeat(513))],
            ..Default::default()
        }] {
            assert!(AxusService::new(dir.path(), "127.0.0.1:0", dir.path(), option).await.is_err());
        }
        assert!(!dir.path().join("identity").exists());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn node_finds_asset_key_owner_through_bootstrap_node() -> TestResult {
        let dir = tempfile::tempdir()?;

        let owner_port = free_port()?;
        let owner = AxusService::new(dir.path().join("owner"), format!("127.0.0.1:{owner_port}"), dir.path(), fast_option(vec![])).await?;
        let owner_id = owner.node_finder.my_node_profile().id().to_vec();

        let owner_node_profile = owner.node_finder.my_node_profile();
        let seeker_port = free_port()?;
        let seeker = AxusService::new(
            dir.path().join("seeker"),
            format!("127.0.0.1:{seeker_port}"),
            dir.path(),
            fast_option(vec![owner_node_profile.clone()]),
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
        // owner は特定のアドレスで待ち受けているため、そのアドレスを広告する
        assert!(
            found
                .iter()
                .all(|node_profile| node_profile.addrs() == [OmniAddr::create_tcp("127.0.0.1".parse().unwrap(), owner_port)])
        );
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
            let a_node_profile = NodeProfile::sign(
                &NodeIdentity::load_or_create(&a_dir.join("identity")).await?.signer(),
                vec![OmniAddr::create_tcp("127.0.0.1".parse()?, a_port)],
                Utc::now(),
            )?;
            let b_node_profile = NodeProfile::sign(
                &NodeIdentity::load_or_create(&b_dir.join("identity")).await?.signer(),
                vec![OmniAddr::create_tcp("127.0.0.1".parse()?, b_port)],
                Utc::now(),
            )?;

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn node_finds_and_dials_an_owner_known_only_through_another_node() -> TestResult {
        let dir = tempfile::tempdir()?;

        // hub だけが owner と seeker の両方を知り、owner と seeker は hub だけを bootstrap に指定する
        let hub_port = free_port()?;
        let hub = AxusService::new(dir.path().join("hub"), format!("127.0.0.1:{hub_port}"), dir.path(), fast_option(vec![])).await?;
        let hub_node_profile = hub.node_finder.my_node_profile();

        let owner = AxusService::new(
            dir.path().join("owner"),
            format!("127.0.0.1:{}", free_port()?),
            dir.path(),
            fast_option(vec![hub_node_profile.clone()]),
        )
        .await?;
        let seeker = AxusService::new(
            dir.path().join("seeker"),
            format!("127.0.0.1:{}", free_port()?),
            dir.path(),
            fast_option(vec![hub_node_profile.clone()]),
        )
        .await?;
        let owner_node_profile = owner.node_finder.my_node_profile();

        // Kadex は自 node より AssetKey に近い node にだけ want と push を送るため、hub を最も近い node にする
        let asset_key = AssetKey {
            typ: "test".to_string(),
            hash: OmniHash {
                typ: OmniHashAlgorithmType::Sha3_256,
                value: hub_node_profile.id().to_vec(),
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
        assert!(found.iter().all(|node_profile| **node_profile == owner_node_profile));

        // seeker と owner は hub から互いの NodeProfile を受け取り、広告されたアドレスで直接つながる
        tokio::time::timeout(TEST_TIMEOUT, async {
            while seeker.node_finder.get_session_count().await < 2 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await?;

        seeker.shutdown().await;
        owner.shutdown().await;
        hub.shutdown().await;

        Ok(())
    }

    #[tokio::test]
    async fn the_file_exchanger_use_is_accepted_after_startup() -> TestResult {
        let dir = tempfile::tempdir()?;
        let service = AxusService::new(dir.path().join("node"), "127.0.0.1:0", dir.path(), fast_option(vec![])).await?;
        let client_dir = tempfile::tempdir()?;
        let signer = crate::core::identity::NodeIdentity::load_or_create(client_dir.path()).await?.signer();
        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        let connector = SessionConnector::new(
            Arc::new(
                ConnectionTcpConnectorImpl::new(TcpProxyOption {
                    typ: TcpProxyType::None,
                    addr: None,
                })
                .await?,
            ),
            signer,
            clock,
            Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng)))),
            SessionOption::default(),
        );
        let addr = service.node_finder.my_node_profile().addrs()[0].clone();

        let session = tokio::time::timeout(TEST_TIMEOUT, connector.connect(&addr, &SessionType::FileExchanger)).await??;

        assert_eq!(session.typ, SessionType::FileExchanger);
        service.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn advertise_addrs_override_the_listen_address() -> TestResult {
        let dir = tempfile::tempdir()?;
        let advertise_addrs = vec![OmniAddr::create_tcp("203.0.113.1".parse()?, 6052)];
        let option = AxusServiceOption {
            advertise_addrs: advertise_addrs.clone(),
            ..fast_option(vec![])
        };

        let service = AxusService::new(dir.path().join("node"), "127.0.0.1:0", dir.path(), option).await?;
        assert_eq!(service.node_finder.my_node_profile().addrs(), advertise_addrs);

        service.shutdown().await;

        Ok(())
    }

    #[tokio::test]
    async fn startup_truncates_advertised_addresses_to_the_wire_limit() -> TestResult {
        let dir = tempfile::tempdir()?;
        let addrs: Vec<_> = (0..12).map(|i| OmniAddr::create_tcp("127.0.0.1".parse().unwrap(), 1000 + i)).collect();
        let option = AxusServiceOption {
            advertise_addrs: addrs.clone(),
            ..fast_option(vec![])
        };
        let service = AxusService::new(dir.path().join("state"), format!("127.0.0.1:{}", free_port()?), dir.path(), option).await?;
        let profile = service.node_finder.my_node_profile();
        assert_eq!(profile.addrs(), &addrs[..NodeProfile::MAX_WIRE_ADDRS]);
        assert_eq!(crate::protocol::NodeProfileCodec::decode(&crate::protocol::NodeProfileCodec::encode(&profile)?)?, profile);
        service.shutdown().await;
        Ok(())
    }

    fn fast_option(bootstrap_node_profiles: Vec<NodeProfile>) -> AxusServiceOption {
        AxusServiceOption {
            bootstrap_node_profiles,
            node_finder_intervals: NodeFinderIntervals {
                connect: Duration::from_millis(100),
                compute: Duration::from_millis(100),
                // 受理 worker の 1 秒の待機より、Hello の受信期限を長くする
                communicate: Duration::from_millis(500),
            },
            ..Default::default()
        }
    }

    fn free_port() -> Result<u16> {
        Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
    }
}
