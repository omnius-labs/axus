use crate::protocol::session::*;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use omnius_core_omnikit::service::connection::secure::{OmniSecureAuth, OmniSecureStream, OmniSecureStreamType};
use parking_lot::Mutex;
use rand_core::CryptoRng;
use tokio::{
    select,
    sync::{Mutex as TokioMutex, Semaphore, mpsc},
    task::{JoinHandle, JoinSet},
    time::{Instant, timeout_at},
};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use omnius_core_base::sleeper::Sleeper;
use omnius_core_omnikit::generated::omni_sign::OmniSigner;
use omnius_core_omnikit::model::omni_addr::OmniAddr;

use crate::{
    base::{
        connection::{ConnectionTcpAccepter, FramedRecvExt as _, FramedSendExt as _, FramedStream, RawStream},
        runtime::Shutdown,
    },
    core::session::message::{HelloMessage, SessionVersion, V2RequestMessage},
    prelude::*,
};

use super::{
    message::{V2RequestType, V2ResultMessage, V2ResultType},
    model::{Session, SessionHandshakeType, SessionOption, SessionType},
};

pub struct SessionAccepter {
    tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync>,
    receivers: Arc<TokioMutex<HashMap<SessionType, mpsc::Receiver<Session>>>>,
    task_accepter: TaskAccepter,
}

impl SessionAccepter {
    pub async fn new(
        tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync>,
        signer: Arc<OmniSigner>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        rng: Arc<Mutex<dyn CryptoRng + Send + Sync>>,
        supported_types: &[SessionType],
        option: SessionOption,
    ) -> Self {
        let mut senders = HashMap::<SessionType, mpsc::Sender<Session>>::new();
        let mut receivers = HashMap::<SessionType, mpsc::Receiver<Session>>::new();

        for typ in supported_types {
            if senders.contains_key(typ) {
                continue;
            }
            let (tx, rx) = mpsc::channel(20);
            senders.insert(typ.clone(), tx);
            receivers.insert(typ.clone(), rx);
        }

        let task_accepter = TaskAccepter::new(Arc::new(TokioMutex::new(senders)), tcp_accepter.clone(), signer, rng, sleeper, option);
        task_accepter.run().await;

        Self {
            tcp_accepter,
            receivers: Arc::new(TokioMutex::new(receivers)),
            task_accepter,
        }
    }

    pub async fn accept(&self, typ: &SessionType) -> Result<Session> {
        let mut receivers = self.receivers.lock().await;
        let receiver = receivers
            .get_mut(typ)
            .ok_or_else(|| Error::new(ErrorKind::UnsupportedType).with_message("unsupported session type"))?;

        receiver.recv().await.ok_or_else(|| Error::new(ErrorKind::EndOfStream).with_message("receiver is closed"))
    }
}

#[async_trait]
impl Shutdown for SessionAccepter {
    async fn shutdown(&self) {
        self.task_accepter.shutdown().await;

        self.tcp_accepter.shutdown().await;
    }
}

#[derive(Clone)]
struct TaskAccepter {
    inner: Inner,
    tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    token: CancellationToken,
}

impl TaskAccepter {
    pub fn new(
        senders: Arc<TokioMutex<HashMap<SessionType, mpsc::Sender<Session>>>>,
        tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync>,
        signer: Arc<OmniSigner>,
        rng: Arc<Mutex<dyn CryptoRng + Send + Sync>>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: SessionOption,
    ) -> Self {
        let inner = Inner { senders, signer, rng, option };
        Self {
            inner,
            tcp_accepter,
            sleeper,
            join_handle: Arc::new(TokioMutex::new(None)),
            token: CancellationToken::new(),
        }
    }

    pub async fn run(&self) {
        let sleeper = self.sleeper.clone();
        let tcp_accepter = self.tcp_accepter.clone();
        let inner = self.inner.clone();
        let token = self.token.clone();
        let join_handle = tokio::spawn(async move {
            let semaphore = Arc::new(Semaphore::new(inner.option.max_pending_handshake_count));
            let mut handshakes = JoinSet::new();
            while !token.is_cancelled() {
                // 完了した task を残さず、TCP の受理を続ける
                while let Some(result) = handshakes.try_join_next() {
                    if let Err(e) = result {
                        warn!(error_message = e.to_string(), "handshake task failed");
                    }
                }
                select! {
                    biased;
                    _ = token.cancelled() => break,
                    result = handshakes.join_next(), if !handshakes.is_empty() => {
                        if let Some(Err(e)) = result {
                            warn!(error_message = e.to_string(), "handshake task failed");
                        }
                    }
                    result = tcp_accepter.accept() => {
                        let (stream, addr) = match result {
                            Ok(connection) => connection,
                            Err(e) => {
                                warn!(error_message = e.to_string(), "accept failed");
                                select! {
                                    _ = token.cancelled() => break,
                                    _ = sleeper.sleep(std::time::Duration::from_secs(1)) => {}
                                }
                                continue;
                            }
                        };
                        let deadline = Instant::now() + inner.option.handshake_timeout;
                        let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                            warn!(address = %addr, "pending handshake limit reached");
                            drop(stream);
                            continue;
                        };
                        let inner = inner.clone();
                        let token = token.clone();
                        handshakes.spawn(async move {
                            let _permit = permit;
                            select! {
                                biased;
                                _ = token.cancelled() => {}
                                result = timeout_at(deadline, inner.handshake(stream, addr)) => {
                                    match result {
                                        Ok(Ok(())) => {}
                                        Ok(Err(e)) => warn!(address = %addr, error_message = e.to_string(), "handshake failed"),
                                        Err(e) => warn!(address = %addr, error_message = e.to_string(), "handshake timed out"),
                                    }
                                }
                            }
                        });
                    }
                }
            }
            // JoinSet を空にしてから破棄し、cancel された全 task の終了を待つ
            token.cancel();
            while let Some(result) = handshakes.join_next().await {
                if let Err(e) = result {
                    warn!(error_message = e.to_string(), "handshake task failed");
                }
            }
        });
        self.join_handle.lock().await.replace(join_handle);
    }
}

#[async_trait]
impl Shutdown for TaskAccepter {
    async fn shutdown(&self) {
        self.token.cancel();
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            let _ = join_handle.await;
        }
    }
}

#[derive(Clone)]
struct Inner {
    senders: Arc<TokioMutex<HashMap<SessionType, mpsc::Sender<Session>>>>,
    signer: Arc<OmniSigner>,
    rng: Arc<Mutex<dyn CryptoRng + Send + Sync>>,
    option: SessionOption,
}

impl Inner {
    async fn handshake(&self, raw: RawStream, addr: SocketAddr) -> Result<()> {
        let secure = OmniSecureStream::new(
            raw,
            OmniSecureStreamType::Accepted,
            self.option.secure_option()?,
            OmniSecureAuth::Mutual { signer: self.signer.clone() },
            self.rng.clone(),
        )
        .await?;
        let cert = secure
            .peer_cert()
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::Reject).with_message("missing peer identity"))?;
        let stream = FramedStream::from_stream(secure, self.option.handshake_max_frame_length);
        let send_hello_message = HelloMessage { version: SessionVersion::V2 };
        stream.sender.lock().await.send_message_with::<HelloMessageCodec>(&send_hello_message).await?;
        let received_hello_message: HelloMessage = stream.receiver.lock().await.recv_message_with::<HelloMessageCodec>().await?;

        if received_hello_message.version == SessionVersion::V2 {
            let received_session_request_message: V2RequestMessage = stream.receiver.lock().await.recv_message_with::<V2RequestMessageCodec>().await?;
            let typ = match received_session_request_message.request_type {
                V2RequestType::Unknown => None,
                V2RequestType::NodeFinder => Some(SessionType::NodeFinder),
                V2RequestType::FileExchanger => Some(SessionType::FileExchanger),
            };

            let permit = match typ.as_ref() {
                Some(typ) => self.senders.lock().await.get(typ).cloned().and_then(|sender| sender.try_reserve_owned().ok()),
                None => None,
            };

            if let (Some(typ), Some(permit)) = (typ, permit) {
                let send_session_result_message = V2ResultMessage {
                    result_type: V2ResultType::Accept,
                };
                stream.sender.lock().await.send_message_with::<V2ResultMessageCodec>(&send_session_result_message).await?;

                stream.set_max_frame_length(typ.max_frame_length()).await;

                let session = Session {
                    typ,
                    address: OmniAddr::new(format!("tcp({addr})").as_str()),
                    handshake_type: SessionHandshakeType::Accepted,
                    cert,
                    stream,
                };
                permit.send(session);
            } else {
                let send_session_result_message = V2ResultMessage {
                    result_type: V2ResultType::Reject,
                };
                stream.sender.lock().await.send_message_with::<V2ResultMessageCodec>(&send_session_result_message).await?;
            }

            Ok(())
        } else {
            Err(Error::new(ErrorKind::UnsupportedType).with_message("unsupported session version"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{net::IpAddr, net::SocketAddr, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use rand::{
        SeedableRng as _,
        rngs::{ChaCha20Rng, SysRng},
    };
    use rand_core::UnwrapErr;
    use testresult::TestResult;

    use omnius_core_base::sleeper::FakeSleeper;
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};

    use crate::{
        base::{
            connection::{ConnectionTcpAccepter, RawStream},
            runtime::Shutdown,
        },
        prelude::*,
    };

    use super::{SessionAccepter, SessionOption, SessionType};

    #[tokio::test]
    async fn shutdown_completes_while_waiting_for_a_connection() -> TestResult {
        let signer = Arc::new(OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?);
        let rng = Arc::new(Mutex::new(ChaCha20Rng::from_rng(&mut UnwrapErr(SysRng))));
        let session_accepter = SessionAccepter::new(
            Arc::new(PendingTcpAccepter),
            signer,
            Arc::new(FakeSleeper),
            rng,
            &[SessionType::NodeFinder],
            SessionOption::default(),
        )
        .await;

        // 接続を待つ accept に入るまで待つ
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(tokio::time::timeout(Duration::from_secs(5), session_accepter.shutdown()).await.is_ok());

        Ok(())
    }

    /// 接続が来ないまま accept を待ち続ける
    struct PendingTcpAccepter;

    #[async_trait]
    impl Shutdown for PendingTcpAccepter {
        async fn shutdown(&self) {}
    }

    #[async_trait]
    impl ConnectionTcpAccepter for PendingTcpAccepter {
        async fn accept(&self) -> Result<(RawStream, SocketAddr)> {
            std::future::pending().await
        }

        async fn get_global_ip_addresses(&self) -> Result<Vec<IpAddr>> {
            Ok(vec![])
        }
    }
}
