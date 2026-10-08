mod accepter;
mod connector;
pub mod message;
pub mod model;

pub use accepter::*;
pub use connector::*;

#[cfg(test)]
mod tests {
    use crate::protocol::session::*;
    use std::{
        net::{IpAddr, SocketAddr},
        sync::Arc,
        time::Duration,
    };

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;
    use testresult::TestResult;
    use tokio::sync::{Mutex as TokioMutex, mpsc};
    use tokio_util::bytes::Bytes;

    use omnius_core_base::sleeper::FakeSleeper;
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        base::{
            connection::{
                ConnectionTcpAccepter, ConnectionTcpAccepterImpl, ConnectionTcpConnector, ConnectionTcpConnectorImpl, FramedRecvExt as _, FramedSendExt as _, FramedStream,
                TcpProxyOption, TcpProxyType,
            },
            runtime::Shutdown,
        },
        core::session::{
            SessionAccepter, SessionConnector,
            message::{HelloMessage, SessionVersion, V1ChallengeMessage, V1RequestMessage, V1RequestType, V1ResultMessage, V1ResultType, V1SignatureMessage},
            model::{SessionOption, SessionType},
        },
        prelude::*,
    };

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    #[ignore]
    async fn simple_test() -> TestResult {
        let tcp_accepter = Arc::new(ConnectionTcpAccepterImpl::new(&OmniAddr::create_tcp("127.0.0.1".parse()?, 60000), false).await?);
        let tcp_connector = Arc::new(
            ConnectionTcpConnectorImpl::new(TcpProxyOption {
                typ: TcpProxyType::None,
                addr: None,
            })
            .await?,
        );

        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        let sleeper = Arc::new(FakeSleeper);

        let session_accepter = SessionAccepter::new(
            tcp_accepter.clone(),
            signer.clone(),
            sleeper.clone(),
            rng.clone(),
            &[SessionType::NodeFinder],
            SessionOption::default(),
        )
        .await;
        let session_connector = SessionConnector::new(tcp_connector, signer, rng, SessionOption::default());

        let client = Arc::new(
            session_connector
                .connect(&OmniAddr::create_tcp("127.0.0.1".parse()?, 60000), &SessionType::NodeFinder)
                .await?,
        );
        let server = Arc::new(session_accepter.accept(&SessionType::NodeFinder).await?);

        client
            .stream
            .sender
            .lock()
            .await
            .send_message(&TestMessage {
                value: "Hello, World!".to_string(),
            })
            .await?;
        let text: TestMessage = server.stream.receiver.lock().await.recv_message().await?;

        println!("{}", text.value);

        session_accepter.shutdown().await;
        tcp_accepter.shutdown().await;

        Ok(())
    }

    #[tokio::test]
    async fn unsupported_requests_do_not_stop_accept_tasks() -> TestResult {
        let (tcp_accepter, tcp_connector) = in_memory_tcp_pair();
        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        let sleeper = Arc::new(FakeSleeper);
        let addr = OmniAddr::create_tcp("127.0.0.1".parse()?, 1);

        let session_accepter = SessionAccepter::new(tcp_accepter, signer.clone(), sleeper, rng.clone(), &[SessionType::NodeFinder], SessionOption::default()).await;
        let session_connector = SessionConnector::new(tcp_connector.clone(), signer.clone(), rng, SessionOption::default());

        for _ in 0..3 {
            match tokio::time::timeout(TEST_TIMEOUT, session_connector.connect(&addr, &SessionType::FileExchanger)).await? {
                Ok(_) => panic!("unsupported FileExchanger session was accepted"),
                Err(e) => assert_eq!(e.kind(), &ErrorKind::Reject),
            }
        }

        let result = tokio::time::timeout(TEST_TIMEOUT, request_session(tcp_connector.as_ref(), signer.as_ref(), &addr, V1RequestType::Unknown)).await??;
        assert_eq!(result, V1ResultType::Reject);

        let client = tokio::time::timeout(TEST_TIMEOUT, session_connector.connect(&addr, &SessionType::NodeFinder)).await??;
        let server = tokio::time::timeout(TEST_TIMEOUT, session_accepter.accept(&SessionType::NodeFinder)).await??;
        assert_eq!(client.typ, SessionType::NodeFinder);
        assert_eq!(server.typ, SessionType::NodeFinder);

        session_accepter.shutdown().await;

        Ok(())
    }

    #[tokio::test]
    async fn enabled_file_exchanger_session_is_routed() -> TestResult {
        let (tcp_accepter, tcp_connector) = in_memory_tcp_pair();
        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        let sleeper = Arc::new(FakeSleeper);
        let addr = OmniAddr::create_tcp("127.0.0.1".parse()?, 1);

        let session_accepter = SessionAccepter::new(
            tcp_accepter,
            signer.clone(),
            sleeper,
            rng.clone(),
            &[SessionType::NodeFinder, SessionType::FileExchanger, SessionType::FileExchanger],
            SessionOption::default(),
        )
        .await;
        let session_connector = SessionConnector::new(tcp_connector, signer, rng, SessionOption::default());

        let client = tokio::time::timeout(TEST_TIMEOUT, session_connector.connect(&addr, &SessionType::FileExchanger)).await??;
        let server = tokio::time::timeout(TEST_TIMEOUT, session_accepter.accept(&SessionType::FileExchanger)).await??;
        assert_eq!(client.typ, SessionType::FileExchanger);
        assert_eq!(server.typ, SessionType::FileExchanger);

        session_accepter.shutdown().await;

        Ok(())
    }

    #[tokio::test]
    async fn responsive_handshake_progresses_while_other_connections_are_silent() -> TestResult {
        let (accepter, connector, tcp_connector) = create_sessions(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        let silent = open_silent_connections(&tcp_connector, 63).await?;

        let client = tokio::time::timeout(Duration::from_millis(500), connector.connect(&test_addr(), &SessionType::NodeFinder)).await??;
        let server = tokio::time::timeout(TEST_TIMEOUT, accepter.accept(&SessionType::NodeFinder)).await??;
        assert_eq!(client.typ, server.typ);

        accepter.shutdown().await;
        drop(silent);
        Ok(())
    }

    #[tokio::test]
    async fn pending_handshake_limit_rejects_the_65th_connection_and_recovers_after_timeout() -> TestResult {
        let option = SessionOption {
            handshake_timeout: Duration::from_secs(1),
            ..SessionOption::default()
        };
        let (accepter, connector, tcp_connector) = create_sessions(option, &[SessionType::NodeFinder]).await?;
        let silent = open_silent_connections(&tcp_connector, 64).await?;

        // 上限を超えた接続には HelloMessage も送らない
        let excess = tcp_connector.connect(&test_addr()).await?;
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(300), excess.receiver.lock().await.recv())
                .await?
                .unwrap_err()
                .kind(),
            &omnius_core_omnikit::ErrorKind::EndOfStream
        );

        tokio::time::timeout(Duration::from_secs(2), async {
            for stream in &silent {
                assert!(stream.receiver.lock().await.recv().await.is_err());
            }
        })
        .await?;

        let client = tokio::time::timeout(TEST_TIMEOUT, connector.connect(&test_addr(), &SessionType::NodeFinder)).await??;
        let server = tokio::time::timeout(TEST_TIMEOUT, accepter.accept(&SessionType::NodeFinder)).await??;
        assert_eq!(client.typ, server.typ);
        accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn connector_times_out_and_closes_an_unresponsive_connection() -> TestResult {
        let (tcp_accepter, tcp_connector) = in_memory_tcp_pair();
        let connector = create_connector(
            tcp_connector,
            SessionOption {
                handshake_timeout: Duration::from_millis(50),
                ..SessionOption::default()
            },
        )?;
        let addr = test_addr();
        let (result, peer) = tokio::time::timeout(Duration::from_millis(500), async {
            tokio::join!(connector.connect(&addr, &SessionType::NodeFinder), tcp_accepter.accept())
        })
        .await?;
        assert_eq!(result.err().expect("handshake did not time out").kind(), &ErrorKind::NetworkError);
        let (peer, _) = peer?;
        let _: HelloMessage = peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        assert!(
            tokio::time::timeout(Duration::from_millis(300), peer.receiver.lock().await.recv())
                .await
                .expect("connection remained open")
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn accepter_uses_one_deadline_across_handshake_steps() -> TestResult {
        let option = SessionOption {
            handshake_timeout: Duration::from_millis(400),
            ..SessionOption::default()
        };
        let (accepter, _connector, tcp_connector) = create_sessions(option, &[SessionType::NodeFinder]).await?;
        let peer = tcp_connector.connect(&test_addr()).await?;
        let _: HelloMessage = peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        peer.sender
            .lock()
            .await
            .send_message_with::<HelloMessageCodec>(&HelloMessage { version: SessionVersion::V1 })
            .await?;
        let _: V1ChallengeMessage = peer.receiver.lock().await.recv_message_with::<V1ChallengeMessageCodec>().await?;
        // Hello で使った時間を差し引いた期限で、challenge の待機も終わる
        assert!(
            tokio::time::timeout(Duration::from_millis(300), peer.receiver.lock().await.recv())
                .await
                .expect("connection remained open")
                .is_err()
        );
        accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn connector_uses_one_deadline_across_handshake_steps() -> TestResult {
        let (tcp_accepter, tcp_connector) = in_memory_tcp_pair();
        let connector = create_connector(
            tcp_connector,
            SessionOption {
                handshake_timeout: Duration::from_millis(400),
                ..SessionOption::default()
            },
        )?;
        let addr = test_addr();
        let connect = connector.connect(&addr, &SessionType::NodeFinder);
        let peer = async {
            let (peer, _) = tcp_accepter.accept().await?;
            let _: HelloMessage = peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
            tokio::time::sleep(Duration::from_millis(250)).await;
            peer.sender
                .lock()
                .await
                .send_message_with::<HelloMessageCodec>(&HelloMessage { version: SessionVersion::V1 })
                .await?;
            let _: V1ChallengeMessage = peer.receiver.lock().await.recv_message_with::<V1ChallengeMessageCodec>().await?;
            assert!(
                tokio::time::timeout(Duration::from_millis(300), peer.receiver.lock().await.recv())
                    .await
                    .expect("connection remained open")
                    .is_err()
            );
            Result::Ok(())
        };
        let (result, peer_result) = tokio::time::timeout(Duration::from_millis(600), async { tokio::join!(connect, peer) }).await?;
        assert_eq!(result.err().expect("handshake did not time out").kind(), &ErrorKind::NetworkError);
        peer_result?;
        Ok(())
    }

    #[tokio::test]
    async fn handshake_deadline_starts_after_tcp_connect() -> TestResult {
        let (accepter, _connector, tcp_connector) = create_sessions(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        let connector = create_connector(
            Arc::new(DelayedTcpConnector {
                inner: tcp_connector,
                delay: Duration::from_millis(100),
            }),
            SessionOption {
                handshake_timeout: Duration::from_millis(50),
                ..SessionOption::default()
            },
        )?;
        let client = tokio::time::timeout(TEST_TIMEOUT, connector.connect(&test_addr(), &SessionType::NodeFinder)).await??;
        let server = tokio::time::timeout(TEST_TIMEOUT, accepter.accept(&SessionType::NodeFinder)).await??;
        assert_eq!(client.typ, server.typ);
        accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn accepter_accepts_the_handshake_frame_boundary_and_closes_an_oversized_frame() -> TestResult {
        let (accepter, _connector, tcp_connector) = create_sessions(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        let peer = tcp_connector.connect(&test_addr()).await?;
        let _: HelloMessage = peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        peer.sender.lock().await.send(padded_hello_frame(SessionOption::HANDSHAKE_MAX_FRAME_LENGTH)?).await?;
        let _: V1ChallengeMessage = tokio::time::timeout(TEST_TIMEOUT, peer.receiver.lock().await.recv_message_with::<V1ChallengeMessageCodec>()).await??;
        drop(peer);

        let peer = tcp_connector.connect(&test_addr()).await?;
        let _: HelloMessage = peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        peer.sender.lock().await.send(padded_hello_frame(SessionOption::HANDSHAKE_MAX_FRAME_LENGTH + 1)?).await?;
        assert!(
            tokio::time::timeout(Duration::from_millis(300), peer.receiver.lock().await.recv())
                .await
                .expect("connection remained open")
                .is_err()
        );
        accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn connector_accepts_the_handshake_frame_boundary_and_closes_an_oversized_frame() -> TestResult {
        for length in [SessionOption::HANDSHAKE_MAX_FRAME_LENGTH, SessionOption::HANDSHAKE_MAX_FRAME_LENGTH + 1] {
            let (tcp_accepter, tcp_connector) = in_memory_tcp_pair();
            let connector = create_connector(
                tcp_connector,
                SessionOption {
                    handshake_timeout: Duration::from_millis(100),
                    ..SessionOption::default()
                },
            )?;
            let addr = test_addr();
            let peer = async {
                let (peer, _) = tcp_accepter.accept().await?;
                let _: HelloMessage = peer.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
                peer.sender.lock().await.send(padded_hello_frame(length)?).await?;
                if length == SessionOption::HANDSHAKE_MAX_FRAME_LENGTH {
                    let _: V1ChallengeMessage = peer.receiver.lock().await.recv_message_with::<V1ChallengeMessageCodec>().await?;
                }
                assert!(
                    tokio::time::timeout(Duration::from_millis(300), peer.receiver.lock().await.recv())
                        .await
                        .expect("connection remained open")
                        .is_err()
                );
                Result::Ok(())
            };
            let (result, peer_result) = tokio::time::timeout(TEST_TIMEOUT, async { tokio::join!(connector.connect(&addr, &SessionType::NodeFinder), peer) }).await?;
            assert!(result.is_err());
            peer_result?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn established_sessions_use_the_frame_limit_for_their_purpose() -> TestResult {
        let (accepter, connector, _) = create_sessions(SessionOption::default(), &[SessionType::NodeFinder, SessionType::FileExchanger]).await?;
        for typ in [SessionType::NodeFinder, SessionType::FileExchanger] {
            let client = tokio::time::timeout(TEST_TIMEOUT, connector.connect(&test_addr(), &typ)).await??;
            let server = tokio::time::timeout(TEST_TIMEOUT, accepter.accept(&typ)).await??;
            let length = if typ == SessionType::NodeFinder { typ.max_frame_length() } else { 4 * 1024 * 1024 + 1 };
            let frame = Bytes::from(vec![0; length]);
            let (sent, received) = tokio::time::timeout(TEST_TIMEOUT, async {
                tokio::join!(async { client.stream.sender.lock().await.send(frame.clone()).await }, async {
                    server.stream.receiver.lock().await.recv().await
                })
            })
            .await?;
            sent?;
            assert_eq!(received?, frame);
            let (sent, received) = tokio::time::timeout(TEST_TIMEOUT, async {
                tokio::join!(async { server.stream.sender.lock().await.send(frame.clone()).await }, async {
                    client.stream.receiver.lock().await.recv().await
                })
            })
            .await?;
            sent?;
            assert_eq!(received?, frame);
            let oversized = Bytes::from(vec![0; typ.max_frame_length() + 1]);
            assert!(client.stream.sender.lock().await.send(oversized.clone()).await.is_err());
            assert!(server.stream.sender.lock().await.send(oversized).await.is_err());
        }
        accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn shutdown_cancels_and_joins_pending_handshakes_without_waiting_for_the_deadline() -> TestResult {
        let option = SessionOption {
            handshake_timeout: Duration::from_secs(3600),
            ..SessionOption::default()
        };
        let (accepter, _connector, tcp_connector) = create_sessions(option, &[SessionType::NodeFinder]).await?;
        let silent = open_silent_connections(&tcp_connector, 8).await?;
        tokio::time::timeout(Duration::from_millis(300), accepter.shutdown()).await?;
        for peer in silent {
            assert!(
                tokio::time::timeout(Duration::from_millis(300), peer.receiver.lock().await.recv())
                    .await
                    .expect("connection remained open")
                    .is_err()
            );
        }
        Ok(())
    }

    async fn create_sessions(option: SessionOption, supported_types: &[SessionType]) -> Result<(SessionAccepter, SessionConnector, Arc<InMemoryTcpConnector>)> {
        let (tcp_accepter, tcp_connector) = in_memory_tcp_pair();
        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        let accepter = SessionAccepter::new(tcp_accepter, signer, Arc::new(FakeSleeper), rng, supported_types, option.clone()).await;
        let connector = create_connector(tcp_connector.clone(), option)?;
        Ok((accepter, connector, tcp_connector))
    }

    fn create_connector(tcp_connector: Arc<dyn ConnectionTcpConnector + Send + Sync>, option: SessionOption) -> Result<SessionConnector> {
        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        Ok(SessionConnector::new(tcp_connector, signer, rng, option))
    }

    async fn open_silent_connections(tcp_connector: &InMemoryTcpConnector, count: usize) -> Result<Vec<FramedStream>> {
        let mut streams = Vec::new();
        for _ in 0..count {
            let stream = tcp_connector.connect(&test_addr()).await?;
            let _: HelloMessage = tokio::time::timeout(TEST_TIMEOUT, stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>())
                .await
                .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))??;
            streams.push(stream);
        }
        Ok(streams)
    }

    fn test_addr() -> OmniAddr {
        OmniAddr::create_tcp("127.0.0.1".parse().unwrap(), 1)
    }

    fn padded_hello_frame(length: usize) -> Result<Bytes> {
        let mut padding = vec![0; length];
        loop {
            let frame = PaddedHelloMessage { padding: padding.clone() }.export()?;
            if frame.len() == length {
                return Ok(Bytes::from(frame));
            }
            padding.truncate(padding.len() - (frame.len() - length));
        }
    }

    struct PaddedHelloMessage {
        padding: Vec<u8>,
    }

    impl RocketPackStruct for PaddedHelloMessage {
        fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
            encoder.write_map(2)?;
            encoder.write_u64(0)?;
            encoder.write_u32(SessionVersion::V1 as u32)?;
            encoder.write_u64(1)?;
            encoder.write_bytes(&value.padding)?;
            Ok(())
        }

        fn unpack(_decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError> {
            Err(RocketPackDecoderError::Other("test message is only used for sending"))
        }
    }

    struct DelayedTcpConnector {
        inner: Arc<InMemoryTcpConnector>,
        delay: Duration,
    }

    #[async_trait]
    impl ConnectionTcpConnector for DelayedTcpConnector {
        async fn connect(&self, addr: &OmniAddr) -> Result<FramedStream> {
            tokio::time::sleep(self.delay).await;
            self.inner.connect(addr).await
        }
    }

    async fn request_session(tcp_connector: &InMemoryTcpConnector, signer: &OmniSigner, addr: &OmniAddr, request_type: V1RequestType) -> Result<V1ResultType> {
        let stream = tcp_connector.connect(addr).await?;

        stream
            .sender
            .lock()
            .await
            .send_message_with::<HelloMessageCodec>(&HelloMessage { version: SessionVersion::V1 })
            .await?;
        let _: HelloMessage = stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;

        let send_challenge_message = V1ChallengeMessage { nonce: [1; 32] };
        stream.sender.lock().await.send_message_with::<V1ChallengeMessageCodec>(&send_challenge_message).await?;
        let received_challenge_message: V1ChallengeMessage = stream.receiver.lock().await.recv_message_with::<V1ChallengeMessageCodec>().await?;

        let send_signature_message = V1SignatureMessage {
            cert: signer.sign(&received_challenge_message.nonce)?,
        };
        stream.sender.lock().await.send_message_with::<V1SignatureMessageCodec>(&send_signature_message).await?;
        let _: V1SignatureMessage = stream.receiver.lock().await.recv_message_with::<V1SignatureMessageCodec>().await?;

        stream
            .sender
            .lock()
            .await
            .send_message_with::<V1RequestMessageCodec>(&V1RequestMessage { request_type })
            .await?;
        let result: V1ResultMessage = stream.receiver.lock().await.recv_message_with::<V1ResultMessageCodec>().await?;

        Ok(result.result_type)
    }

    fn in_memory_tcp_pair() -> (Arc<InMemoryTcpAccepter>, Arc<InMemoryTcpConnector>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Arc::new(InMemoryTcpAccepter {
                receiver: TokioMutex::new(receiver),
            }),
            Arc::new(InMemoryTcpConnector { sender }),
        )
    }

    struct InMemoryTcpAccepter {
        receiver: TokioMutex<mpsc::UnboundedReceiver<(FramedStream, SocketAddr)>>,
    }

    #[async_trait]
    impl ConnectionTcpAccepter for InMemoryTcpAccepter {
        async fn accept(&self) -> Result<(FramedStream, SocketAddr)> {
            self.receiver
                .lock()
                .await
                .recv()
                .await
                .ok_or_else(|| Error::new(ErrorKind::EndOfStream).with_message("in-memory accepter is closed"))
        }

        async fn get_global_ip_addresses(&self) -> Result<Vec<IpAddr>> {
            Ok(Vec::new())
        }
    }

    #[async_trait]
    impl Shutdown for InMemoryTcpAccepter {
        async fn shutdown(&self) {}
    }

    struct InMemoryTcpConnector {
        sender: mpsc::UnboundedSender<(FramedStream, SocketAddr)>,
    }

    #[async_trait]
    impl ConnectionTcpConnector for InMemoryTcpConnector {
        async fn connect(&self, _addr: &OmniAddr) -> Result<FramedStream> {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let (client_reader, client_writer) = tokio::io::split(client);
            let (server_reader, server_writer) = tokio::io::split(server);

            self.sender
                .send((FramedStream::new(server_reader, server_writer), SocketAddr::from(([127, 0, 0, 1], 1))))
                .map_err(|_| Error::new(ErrorKind::EndOfStream).with_message("in-memory accepter is closed"))?;

            Ok(FramedStream::new(client_reader, client_writer))
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TestMessage {
        pub value: String,
    }

    impl RocketPackStruct for TestMessage {
        fn pack(encoder: &mut impl RocketPackEncoder, value: &Self) -> std::result::Result<(), RocketPackEncoderError> {
            encoder.write_map(1)?;

            encoder.write_u64(0)?;
            encoder.write_string(value.value.as_str())?;

            Ok(())
        }

        fn unpack(decoder: &mut impl RocketPackDecoder) -> std::result::Result<Self, RocketPackDecoderError>
        where
            Self: Sized,
        {
            let mut value: Option<String> = None;

            let count = decoder.read_map()?;

            for _ in 0..count {
                match decoder.read_u64()? {
                    0 => value = Some(decoder.read_string()?),
                    _ => decoder.skip_field()?,
                }
            }

            Ok(Self {
                value: value.ok_or(RocketPackDecoderError::Other("missing field: value"))?,
            })
        }
    }
}
