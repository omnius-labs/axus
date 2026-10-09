#![cfg(test)]

#[cfg(test)]
mod tests {

    use std::{
        net::{IpAddr, SocketAddr},
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        task::{Context, Poll},
        time::Duration,
    };

    use async_trait::async_trait;
    use omnius_core_base::sleeper::FakeSleeper;
    use omnius_core_omnikit::{
        generated::omni_sign::{OmniSignType, OmniSigner},
        model::omni_addr::OmniAddr,
        service::connection::secure::{OmniSecureAuth, OmniSecureStream, OmniSecureStreamType},
    };
    use parking_lot::Mutex;
    use rand::{SeedableRng, rngs::ChaCha20Rng};
    use rand_core::CryptoRng;
    use testresult::TestResult;
    use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf},
        sync::{Mutex as TokioMutex, mpsc},
    };
    use tokio_util::bytes::Bytes;

    use super::super::{SessionAccepter, SessionConnector, message::*, model::*};
    use crate::{
        base::{
            connection::{
                ConnectionTcpAccepter, ConnectionTcpConnector, ConnectionTcpConnectorImpl, FramedRecvExt as _, FramedSendExt as _, FramedStream, RawStream, TcpProxyOption,
                TcpProxyType,
            },
            runtime::Shutdown,
        },
        prelude::*,
        protocol::session::*,
    };

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn signer(name: &str) -> Result<Arc<OmniSigner>> {
        Ok(Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, name)?))
    }
    fn rng(seed: u64) -> Arc<Mutex<dyn CryptoRng + Send + Sync>> {
        Arc::new(Mutex::new(ChaCha20Rng::seed_from_u64(seed)))
    }
    fn addr() -> OmniAddr {
        OmniAddr::create_tcp("127.0.0.1".parse().unwrap(), 1)
    }

    struct Fixture {
        accepter: SessionAccepter,
        connector: SessionConnector,
        tcp: Arc<MemoryConnector>,
        server: Arc<OmniSigner>,
        client: Arc<OmniSigner>,
    }

    impl Fixture {
        async fn new(option: SessionOption, types: &[SessionType]) -> Result<Self> {
            let (incoming, tcp) = memory_pair();
            let server = signer("server")?;
            let client = signer("client")?;
            let accepter = SessionAccepter::new(incoming, server.clone(), Arc::new(FakeSleeper), rng(1), types, option.clone()).await;
            let connector = SessionConnector::new(tcp.clone(), client.clone(), rng(2), option);
            Ok(Self {
                accepter,
                connector,
                tcp,
                server,
                client,
            })
        }
        async fn connect(&self, typ: &SessionType) -> Result<Session> {
            self.connector.connect(&addr(), typ, &self.server.public_key()?).await
        }
    }

    async fn peer(raw: RawStream, role: OmniSecureStreamType, signer: Arc<OmniSigner>, option: &SessionOption) -> Result<FramedStream> {
        let secure = OmniSecureStream::new(raw, role, option.secure_option()?, OmniSecureAuth::Mutual { signer }, rng(3)).await?;
        Ok(FramedStream::from_stream(secure, option.handshake_max_frame_length))
    }

    async fn exchange_hello(stream: &FramedStream) -> Result<()> {
        stream
            .sender
            .lock()
            .await
            .send_message_with::<HelloMessageCodec>(&HelloMessage { version: SessionVersion::V2 })
            .await?;
        let hello: HelloMessage = stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        assert_eq!(hello.version, SessionVersion::V2);
        Ok(())
    }

    #[tokio::test]
    async fn supported_purposes_route_and_unknown_requests_do_not_stop_acceptance() -> TestResult {
        let f = Fixture::new(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        for _ in 0..3 {
            assert_eq!(f.connect(&SessionType::FileExchanger).await.err().unwrap().kind(), &ErrorKind::Reject);
        }
        let stream = peer(f.tcp.connect(&addr()).await?, OmniSecureStreamType::Connected, f.client.clone(), &SessionOption::default()).await?;
        exchange_hello(&stream).await?;
        stream
            .sender
            .lock()
            .await
            .send_message_with::<V2RequestMessageCodec>(&V2RequestMessage {
                request_type: V2RequestType::Unknown,
            })
            .await?;
        let result: V2ResultMessage = stream.receiver.lock().await.recv_message_with::<V2ResultMessageCodec>().await?;
        assert_eq!(result.result_type, V2ResultType::Reject);
        drop(stream);
        let client = f.connect(&SessionType::NodeFinder).await?;
        let server = tokio::time::timeout(TEST_TIMEOUT, f.accepter.accept(&SessionType::NodeFinder)).await??;
        assert_eq!(client.cert.public_key, f.server.public_key()?);
        assert_eq!(server.cert.public_key, f.client.public_key()?);
        assert_eq!(client.typ, server.typ);
        f.accepter.shutdown().await;
        let f = Fixture::new(SessionOption::default(), &[SessionType::NodeFinder, SessionType::FileExchanger, SessionType::FileExchanger]).await?;
        let client = f.connect(&SessionType::FileExchanger).await?;
        let server = f.accepter.accept(&SessionType::FileExchanger).await?;
        assert_eq!(client.typ, server.typ);
        f.accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn expected_key_mismatch_never_reaches_a_purpose_queue() -> TestResult {
        let f = Fixture::new(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        let unrelated = signer("unrelated")?.public_key()?;
        let result = f.connector.connect(&addr(), &SessionType::NodeFinder, &unrelated).await;
        assert_eq!(result.err().unwrap().kind(), &ErrorKind::Reject);
        assert!(tokio::time::timeout(Duration::from_millis(50), f.accepter.accept(&SessionType::NodeFinder)).await.is_err());
        let client = f.connect(&SessionType::NodeFinder).await?;
        let server = f.accepter.accept(&SessionType::NodeFinder).await?;
        assert_eq!(client.cert.public_key, f.server.public_key()?);
        assert_eq!(server.cert.public_key, f.client.public_key()?);
        f.accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn old_plaintext_anonymous_context_and_encrypted_v1_are_rejected() -> TestResult {
        let f = Fixture::new(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        let mut legacy = f.tcp.connect(&addr()).await?;
        legacy.write_all(&[3, 0, 0, 0, 0xa1, 0, 1, 36, 0, 0, 0]).await?;
        let mut response = Vec::new();
        tokio::time::timeout(TEST_TIMEOUT, legacy.read_to_end(&mut response)).await??;
        assert!(response.is_empty());
        for mode in 0..2 {
            let raw = f.tcp.connect(&addr()).await?;
            let mut option = SessionOption::default().secure_option()?;
            let auth = if mode == 0 {
                OmniSecureAuth::Anonymous
            } else {
                option.context = b"wrong-context".to_vec();
                OmniSecureAuth::Mutual { signer: f.client.clone() }
            };
            assert!(OmniSecureStream::new(raw, OmniSecureStreamType::Connected, option, auth, rng(4)).await.is_err());
        }
        let stream = peer(f.tcp.connect(&addr()).await?, OmniSecureStreamType::Connected, f.client.clone(), &SessionOption::default()).await?;
        let _: HelloMessage = stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        stream.sender.lock().await.send(Bytes::from_static(&[0xa1, 0, 1])).await?;
        assert!(stream.receiver.lock().await.recv().await.is_err());
        assert!(tokio::time::timeout(Duration::from_millis(50), f.accepter.accept(&SessionType::NodeFinder)).await.is_err());
        let _client = f.connect(&SessionType::NodeFinder).await?;
        let _server = f.accepter.accept(&SessionType::NodeFinder).await?;
        f.accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn unknown_result_is_not_mistaken_for_accept() -> TestResult {
        let (incoming, tcp) = memory_pair();
        let server = signer("server")?;
        let connector = SessionConnector::new(tcp, signer("client")?, rng(5), SessionOption::default());
        let public_key = server.public_key()?;
        let target = addr();
        let connect = connector.connect(&target, &SessionType::NodeFinder, &public_key);
        let remote = async {
            let (raw, _) = incoming.accept().await?;
            let stream = peer(raw, OmniSecureStreamType::Accepted, server, &SessionOption::default()).await?;
            exchange_hello(&stream).await?;
            let _: V2RequestMessage = stream.receiver.lock().await.recv_message_with::<V2RequestMessageCodec>().await?;
            stream
                .sender
                .lock()
                .await
                .send_message_with::<V2ResultMessageCodec>(&V2ResultMessage {
                    result_type: V2ResultType::Unknown,
                })
                .await?;
            Result::Ok(())
        };
        let (result, remote) = tokio::time::timeout(TEST_TIMEOUT, async { tokio::join!(connect, remote) }).await?;
        assert_eq!(result.err().unwrap().kind(), &ErrorKind::InvalidFormat);
        remote?;
        Ok(())
    }

    #[tokio::test]
    async fn healthy_handshake_progresses_beside_63_silent_connections() -> TestResult {
        let f = Fixture::new(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        let mut silent = Vec::new();
        for _ in 0..63 {
            silent.push(f.tcp.connect(&addr()).await?);
        }
        let client = tokio::time::timeout(Duration::from_secs(1), f.connect(&SessionType::NodeFinder)).await??;
        let server = f.accepter.accept(&SessionType::NodeFinder).await?;
        assert_eq!(client.typ, server.typ);
        f.accepter.shutdown().await;
        drop(silent);
        Ok(())
    }

    #[tokio::test]
    async fn handshake_limit_drops_the_65th_transport_and_recovers() -> TestResult {
        let f = Fixture::new(
            SessionOption {
                handshake_timeout: Duration::from_secs(1),
                ..SessionOption::default()
            },
            &[SessionType::NodeFinder],
        )
        .await?;
        let mut silent = Vec::new();
        for _ in 0..64 {
            silent.push(f.tcp.connect(&addr()).await?);
        }
        tokio::time::timeout(Duration::from_millis(300), async {
            while f.tcp.accepted.load(Ordering::SeqCst) < 64 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        let mut excess = f.tcp.connect(&addr()).await?;
        let mut byte = [0; 1];
        assert_eq!(tokio::time::timeout(Duration::from_millis(300), excess.read(&mut byte)).await??, 0);
        tokio::time::timeout(Duration::from_secs(2), async {
            for stream in &mut silent {
                assert_eq!(stream.read(&mut byte).await?, 0);
            }
            Result::Ok(())
        })
        .await??;
        let _client = f.connect(&SessionType::NodeFinder).await?;
        let _server = f.accepter.accept(&SessionType::NodeFinder).await?;
        f.accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn accepter_deadline_includes_secure_handshake_and_encrypted_hello() -> TestResult {
        let option = SessionOption {
            handshake_timeout: Duration::from_millis(400),
            ..SessionOption::default()
        };
        let f = Fixture::new(option.clone(), &[SessionType::NodeFinder]).await?;
        let raw = f.tcp.connect(&addr()).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        let stream = peer(raw, OmniSecureStreamType::Connected, f.client.clone(), &option).await?;
        let _: HelloMessage = stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
        assert!(tokio::time::timeout(Duration::from_millis(300), stream.receiver.lock().await.recv()).await?.is_err());
        f.accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn connector_deadline_spans_secure_and_purpose_negotiation() -> TestResult {
        let (incoming, tcp) = memory_pair();
        let server = signer("server")?;
        let key = server.public_key()?;
        let option = SessionOption {
            handshake_timeout: Duration::from_millis(400),
            ..SessionOption::default()
        };
        let connector = SessionConnector::new(tcp, signer("client")?, rng(8), option.clone());
        let target = addr();
        let remote = async {
            let (raw, _) = incoming.accept().await?;
            tokio::time::sleep(Duration::from_millis(250)).await;
            let stream = peer(raw, OmniSecureStreamType::Accepted, server, &option).await?;
            let _: HelloMessage = stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;
            assert!(
                tokio::time::timeout(Duration::from_millis(300), stream.receiver.lock().await.recv())
                    .await
                    .map_err(|e| Error::from_error(e, ErrorKind::NetworkError))?
                    .is_err()
            );
            Result::Ok(())
        };
        let (result, remote) = tokio::time::timeout(TEST_TIMEOUT, async { tokio::join!(connector.connect(&target, &SessionType::NodeFinder, &key), remote) }).await?;
        assert_eq!(result.err().unwrap().kind(), &ErrorKind::NetworkError);
        remote?;
        Ok(())
    }

    #[tokio::test]
    async fn connector_timeout_starts_only_after_tcp_connect() -> TestResult {
        let f = Fixture::new(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        let connector = SessionConnector::new(
            Arc::new(DelayedConnector { inner: f.tcp.clone() }),
            f.client.clone(),
            rng(9),
            SessionOption {
                handshake_timeout: Duration::from_millis(100),
                ..SessionOption::default()
            },
        );
        let _client = connector.connect(&addr(), &SessionType::NodeFinder, &f.server.public_key()?).await?;
        let _server = f.accepter.accept(&SessionType::NodeFinder).await?;
        f.accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn unresponsive_secure_peer_is_closed_by_connector_timeout() -> TestResult {
        let (incoming, tcp) = memory_pair();
        let server = signer("server")?;
        let key = server.public_key()?;
        let connector = SessionConnector::new(
            tcp,
            signer("client")?,
            rng(10),
            SessionOption {
                handshake_timeout: Duration::from_millis(50),
                ..SessionOption::default()
            },
        );
        let target = addr();
        let (result, remote) = tokio::time::timeout(TEST_TIMEOUT, async {
            tokio::join!(connector.connect(&target, &SessionType::NodeFinder, &key), incoming.accept())
        })
        .await?;
        assert_eq!(result.err().unwrap().kind(), &ErrorKind::NetworkError);
        let (mut remote, _) = remote?;
        let mut bytes = Vec::new();
        remote.read_to_end(&mut bytes).await?;
        assert!(bytes.starts_with(b"OMNISC2\0"));
        Ok(())
    }

    #[tokio::test]
    async fn shutdown_cancels_all_pending_secure_handshakes() -> TestResult {
        let f = Fixture::new(
            SessionOption {
                handshake_timeout: Duration::from_secs(3600),
                ..SessionOption::default()
            },
            &[SessionType::NodeFinder],
        )
        .await?;
        let mut silent = Vec::new();
        for _ in 0..8 {
            silent.push(f.tcp.connect(&addr()).await?);
        }
        tokio::time::timeout(Duration::from_millis(300), async {
            while f.tcp.accepted.load(Ordering::SeqCst) < silent.len() {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        tokio::time::timeout(Duration::from_millis(300), f.accepter.shutdown()).await?;
        for stream in &mut silent {
            let mut out = Vec::new();
            tokio::time::timeout(Duration::from_millis(300), stream.read_to_end(&mut out)).await??;
            assert!(out.is_empty());
        }
        Ok(())
    }

    #[tokio::test]
    async fn secure_profile_and_encrypted_messages_keep_handshake_frame_limits() -> TestResult {
        let f = Fixture::new(SessionOption::default(), &[SessionType::NodeFinder]).await?;
        let mut raw = f.tcp.connect(&addr()).await?;
        raw.write_all(b"OMNISC2\0").await?;
        raw.write_u32_le(16385).await?;
        let mut out = Vec::new();
        tokio::time::timeout(TEST_TIMEOUT, raw.read_to_end(&mut out)).await??;
        assert!(out.is_empty());
        f.accepter.shutdown().await;
        for length in [16384usize, 16385] {
            let (incoming, tcp) = memory_pair();
            let server = signer("server")?;
            let key = server.public_key()?;
            let connector = SessionConnector::new(tcp, signer("client")?, rng(12), SessionOption::default());
            let target = addr();
            let remote = async {
                let (raw, _) = incoming.accept().await?;
                let stream = peer(raw, OmniSecureStreamType::Accepted, server, &SessionOption::default()).await?;
                exchange_hello(&stream).await?;
                let _: V2RequestMessage = stream.receiver.lock().await.recv_message_with::<V2RequestMessageCodec>().await?;
                stream.set_max_frame_length(16385).await;
                stream.sender.lock().await.send(Bytes::from(vec![0; length])).await?;
                Result::Ok(())
            };
            let (result, remote) = tokio::time::timeout(TEST_TIMEOUT, async { tokio::join!(connector.connect(&target, &SessionType::NodeFinder, &key), remote) }).await?;
            remote?;
            assert_eq!(result.err().unwrap().kind(), if length == 16384 { &ErrorKind::SerdeError } else { &ErrorKind::IoError });
        }
        Ok(())
    }

    #[tokio::test]
    async fn secure_sessions_use_purpose_limits_and_ciphertext_on_transport() -> TestResult {
        let f = Fixture::new(SessionOption::default(), &[SessionType::NodeFinder, SessionType::FileExchanger]).await?;
        for typ in [SessionType::NodeFinder, SessionType::FileExchanger] {
            let client = f.connect(&typ).await?;
            let server = f.accepter.accept(&typ).await?;
            let length = if typ == SessionType::NodeFinder { typ.max_frame_length() } else { 4 * 1024 * 1024 + 1 };
            let payload = Bytes::from(vec![0x5a; length]);
            let (sent, received) = tokio::time::timeout(TEST_TIMEOUT, async {
                tokio::join!(async { client.stream.sender.lock().await.send(payload.clone()).await }, async {
                    server.stream.receiver.lock().await.recv().await
                })
            })
            .await?;
            sent?;
            assert_eq!(received?, payload);
            let (sent, received) = tokio::time::timeout(TEST_TIMEOUT, async {
                tokio::join!(async { server.stream.sender.lock().await.send(payload.clone()).await }, async {
                    client.stream.receiver.lock().await.recv().await
                })
            })
            .await?;
            sent?;
            assert_eq!(received?, payload);
            assert!(client.stream.sender.lock().await.send(Bytes::from(vec![0; typ.max_frame_length() + 1])).await.is_err());
        }
        let client = f.connect(&SessionType::NodeFinder).await?;
        let server = f.accepter.accept(&SessionType::NodeFinder).await?;
        let before = f.tcp.wire.bytes.lock().len();
        let payload = Bytes::from_static(b"private-axus-v2-payload-not-visible-on-raw-transport");
        client.stream.sender.lock().await.send(payload.clone()).await?;
        assert_eq!(server.stream.receiver.lock().await.recv().await?, payload);
        assert!(!f.tcp.wire.bytes.lock()[before..].windows(payload.len()).any(|window| window == payload));
        f.tcp.wire.corrupt.store(true, Ordering::SeqCst);
        client.stream.sender.lock().await.send(Bytes::from_static(b"tampered")).await?;
        assert!(server.stream.receiver.lock().await.recv().await.is_err());
        f.accepter.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn socks5_tunnel_uses_the_same_secure_session_path() -> TestResult {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let proxy_addr = listener.local_addr()?.to_string();
        let server = signer("server")?;
        let public_key = server.public_key()?;
        let transport = Arc::new(
            ConnectionTcpConnectorImpl::new(TcpProxyOption {
                typ: TcpProxyType::Socks5,
                addr: Some(proxy_addr),
            })
            .await?,
        );
        let connector = SessionConnector::new(transport, signer("client")?, rng(14), SessionOption::default());
        let target = OmniAddr::create_tcp("127.0.0.1".parse()?, 6052);
        let remote = async {
            let (mut socket, _) = listener.accept().await?;
            assert_eq!(socket.read_u8().await?, 5);
            let count = socket.read_u8().await?;
            let mut methods = vec![0; count as usize];
            socket.read_exact(&mut methods).await?;
            assert!(methods.contains(&0));
            socket.write_all(&[5, 0]).await?;
            let mut header = [0; 4];
            socket.read_exact(&mut header).await?;
            assert_eq!(&header[..3], &[5, 1, 0]);
            let length = match header[3] {
                1 => 4,
                3 => socket.read_u8().await? as usize,
                4 => 16,
                _ => panic!("address kind"),
            };
            let mut destination = vec![0; length];
            socket.read_exact(&mut destination).await?;
            assert_eq!(socket.read_u16().await?, 6052);
            socket.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0x17, 0xa4]).await?;
            let stream = peer(Box::new(socket), OmniSecureStreamType::Accepted, server, &SessionOption::default()).await?;
            exchange_hello(&stream).await?;
            let request: V2RequestMessage = stream.receiver.lock().await.recv_message_with::<V2RequestMessageCodec>().await?;
            assert_eq!(request.request_type, V2RequestType::NodeFinder);
            stream
                .sender
                .lock()
                .await
                .send_message_with::<V2ResultMessageCodec>(&V2ResultMessage {
                    result_type: V2ResultType::Accept,
                })
                .await?;
            assert_eq!(stream.receiver.lock().await.recv().await?, Bytes::from_static(b"through-proxy"));
            Result::Ok(())
        };
        let client = async {
            let session = connector.connect(&target, &SessionType::NodeFinder, &public_key).await?;
            session.stream.sender.lock().await.send(Bytes::from_static(b"through-proxy")).await?;
            Result::Ok(())
        };
        tokio::time::timeout(TEST_TIMEOUT, async { tokio::try_join!(client, remote) }).await??;
        Ok(())
    }

    fn memory_pair() -> (Arc<MemoryAccepter>, Arc<MemoryConnector>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let accepted = Arc::new(AtomicUsize::new(0));
        (
            Arc::new(MemoryAccepter {
                receiver: TokioMutex::new(receiver),
                accepted: accepted.clone(),
            }),
            Arc::new(MemoryConnector {
                sender,
                accepted,
                wire: Arc::new(WireSpy::default()),
            }),
        )
    }
    struct MemoryAccepter {
        receiver: TokioMutex<mpsc::UnboundedReceiver<(RawStream, SocketAddr)>>,
        accepted: Arc<AtomicUsize>,
    }
    #[async_trait]
    impl Shutdown for MemoryAccepter {
        async fn shutdown(&self) {}
    }
    #[async_trait]
    impl ConnectionTcpAccepter for MemoryAccepter {
        async fn accept(&self) -> Result<(RawStream, SocketAddr)> {
            let value = self.receiver.lock().await.recv().await.ok_or_else(|| Error::new(ErrorKind::EndOfStream))?;
            self.accepted.fetch_add(1, Ordering::SeqCst);
            Ok(value)
        }
        async fn get_global_ip_addresses(&self) -> Result<Vec<IpAddr>> {
            Ok(vec![])
        }
    }
    struct MemoryConnector {
        sender: mpsc::UnboundedSender<(RawStream, SocketAddr)>,
        accepted: Arc<AtomicUsize>,
        wire: Arc<WireSpy>,
    }
    #[async_trait]
    impl ConnectionTcpConnector for MemoryConnector {
        async fn connect(&self, _: &OmniAddr) -> Result<RawStream> {
            let (client, server) = tokio::io::duplex(64 * 1024);
            self.sender
                .send((Box::new(server), SocketAddr::from(([127, 0, 0, 1], 1))))
                .map_err(|_| Error::new(ErrorKind::EndOfStream))?;
            Ok(Box::new(ObservedStream {
                inner: client,
                wire: self.wire.clone(),
            }))
        }
    }
    struct DelayedConnector {
        inner: Arc<MemoryConnector>,
    }
    #[async_trait]
    impl ConnectionTcpConnector for DelayedConnector {
        async fn connect(&self, addr: &OmniAddr) -> Result<RawStream> {
            tokio::time::sleep(Duration::from_millis(150)).await;
            self.inner.connect(addr).await
        }
    }
    #[derive(Default)]
    struct WireSpy {
        bytes: Mutex<Vec<u8>>,
        corrupt: AtomicBool,
    }
    struct ObservedStream {
        inner: DuplexStream,
        wire: Arc<WireSpy>,
    }
    impl AsyncRead for ObservedStream {
        fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, output: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_read(cx, output)
        }
    }
    impl AsyncWrite for ObservedStream {
        fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, input: &[u8]) -> Poll<std::io::Result<usize>> {
            let this = self.get_mut();
            let mut altered = input.to_vec();
            let corrupt = this.wire.corrupt.load(Ordering::SeqCst);
            if corrupt {
                *altered.last_mut().unwrap() ^= 1;
            }
            match Pin::new(&mut this.inner).poll_write(cx, &altered) {
                Poll::Ready(Ok(n)) => {
                    if corrupt {
                        assert_eq!(n, input.len(), "small tamper fixture must fit the empty pipe");
                        this.wire.corrupt.store(false, Ordering::SeqCst);
                    }
                    this.wire.bytes.lock().extend_from_slice(&altered[..n]);
                    Poll::Ready(Ok(n))
                }
                result => result,
            }
        }
        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_flush(cx)
        }
        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
        }
    }
}
