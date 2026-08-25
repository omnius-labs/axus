mod accepter;
mod connector;
pub mod message;
pub mod model;

pub use accepter::*;
pub use connector::*;

#[cfg(test)]
mod tests {
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
            model::SessionType,
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

        let session_accepter = SessionAccepter::new(tcp_accepter.clone(), signer.clone(), sleeper.clone(), rng.clone(), &[SessionType::NodeFinder]).await;
        let session_connector = SessionConnector::new(tcp_connector, signer, rng);

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

        let session_accepter = SessionAccepter::new(tcp_accepter, signer.clone(), sleeper, rng.clone(), &[SessionType::NodeFinder]).await;
        let session_connector = SessionConnector::new(tcp_connector.clone(), signer.clone(), rng);

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
        )
        .await;
        let session_connector = SessionConnector::new(tcp_connector, signer, rng);

        let client = tokio::time::timeout(TEST_TIMEOUT, session_connector.connect(&addr, &SessionType::FileExchanger)).await??;
        let server = tokio::time::timeout(TEST_TIMEOUT, session_accepter.accept(&SessionType::FileExchanger)).await??;
        assert_eq!(client.typ, SessionType::FileExchanger);
        assert_eq!(server.typ, SessionType::FileExchanger);

        session_accepter.shutdown().await;

        Ok(())
    }

    async fn request_session(tcp_connector: &InMemoryTcpConnector, signer: &OmniSigner, addr: &OmniAddr, request_type: V1RequestType) -> Result<V1ResultType> {
        let stream = tcp_connector.connect(addr).await?;

        stream.sender.lock().await.send_message(&HelloMessage { version: SessionVersion::V1 }).await?;
        let _: HelloMessage = stream.receiver.lock().await.recv_message().await?;

        let send_challenge_message = V1ChallengeMessage { nonce: [1; 32] };
        stream.sender.lock().await.send_message(&send_challenge_message).await?;
        let received_challenge_message: V1ChallengeMessage = stream.receiver.lock().await.recv_message().await?;

        let send_signature_message = V1SignatureMessage {
            cert: signer.sign(&received_challenge_message.nonce)?,
        };
        stream.sender.lock().await.send_message(&send_signature_message).await?;
        let _: V1SignatureMessage = stream.receiver.lock().await.recv_message().await?;

        stream.sender.lock().await.send_message(&V1RequestMessage { request_type }).await?;
        let result: V1ResultMessage = stream.receiver.lock().await.recv_message().await?;

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
