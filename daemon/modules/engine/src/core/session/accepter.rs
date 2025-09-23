use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use futures::{FutureExt, future::join_all};
use parking_lot::Mutex;
use tokio::{
    sync::{Mutex as TokioMutex, mpsc},
    task::JoinHandle,
};
use tracing::warn;

use omnius_core_base::{random_bytes::RandomBytesProvider, sleeper::Sleeper};
use omnius_core_omnikit::model::{OmniAddr, OmniSigner};

use crate::{
    base::{
        Shutdown,
        connection::{ConnectionTcpAccepter, FramedRecvExt as _, FramedSendExt as _},
    },
    core::session::message::{HelloMessage, SessionVersion, V1ChallengeMessage, V1RequestMessage, V1SignatureMessage},
    prelude::*,
};

use super::{
    message::{V1RequestType, V1ResultMessage, V1ResultType},
    model::{Session, SessionHandshakeType, SessionType},
};

pub struct SessionAccepter {
    tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync>,
    signer: Arc<OmniSigner>,
    random_bytes_provider: Arc<Mutex<dyn RandomBytesProvider + Send + Sync>>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    receivers: Arc<TokioMutex<HashMap<SessionType, mpsc::Receiver<Session>>>>,
    senders: Arc<TokioMutex<HashMap<SessionType, mpsc::Sender<Session>>>>,
    task_acceptors: Arc<TokioMutex<Vec<TaskAccepter>>>,
}

impl SessionAccepter {
    pub async fn new(
        tcp_accepter: Arc<dyn ConnectionTcpAccepter + Send + Sync>,
        signer: Arc<OmniSigner>,
        random_bytes_provider: Arc<Mutex<dyn RandomBytesProvider + Send + Sync>>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Self {
        let senders = Arc::new(TokioMutex::new(HashMap::<SessionType, mpsc::Sender<Session>>::new()));
        let receivers = Arc::new(TokioMutex::new(HashMap::<SessionType, mpsc::Receiver<Session>>::new()));

        for typ in [SessionType::NodeFinder].iter() {
            let (tx, rx) = mpsc::channel(20);
            senders.lock().await.insert(typ.clone(), tx);
            receivers.lock().await.insert(typ.clone(), rx);
        }

        let result = Self {
            tcp_accepter,
            signer,
            random_bytes_provider,
            sleeper,
            receivers,
            senders,
            task_acceptors: Arc::new(TokioMutex::new(Vec::new())),
        };
        result.run().await;

        result
    }

    async fn run(&self) {
        for _ in 0..3 {
            let task = TaskAccepter::new(
                self.senders.clone(),
                self.tcp_accepter.clone(),
                self.signer.clone(),
                self.random_bytes_provider.clone(),
                self.sleeper.clone(),
            );
            task.run().await;
            self.task_acceptors.lock().await.push(task);
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
        let mut task_acceptors = self.task_acceptors.lock().await;
        let task_acceptors: Vec<TaskAccepter> = task_acceptors.drain(..).collect();
        join_all(task_acceptors.iter().map(|task| task.shutdown())).await;

        self.tcp_accepter.shutdown().await;
    }
}

#[derive(Clone)]
struct TaskAccepter {
    inner: Inner,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
}

impl TaskAccepter {
    pub fn new(
        senders: Arc<TokioMutex<HashMap<SessionType, mpsc::Sender<Session>>>>,
        tcp_connector: Arc<dyn ConnectionTcpAccepter + Send + Sync>,
        signer: Arc<OmniSigner>,
        random_bytes_provider: Arc<Mutex<dyn RandomBytesProvider + Send + Sync>>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
    ) -> Self {
        let inner = Inner {
            senders,
            tcp_connector,
            signer,
            random_bytes_provider,
        };
        Self {
            inner,
            sleeper,
            join_handle: Arc::new(TokioMutex::new(None)),
        }
    }

    pub async fn run(&self) {
        let sleeper = self.sleeper.clone();
        let inner = self.inner.clone();
        let join_handle = tokio::spawn(async move {
            loop {
                sleeper.sleep(std::time::Duration::from_secs(1)).await;
                let res = inner.accept().await;
                if let Err(e) = res {
                    warn!(error_message = e.to_string(), "accept failed");
                }
            }
        });
        self.join_handle.lock().await.replace(join_handle);
    }
}

#[async_trait]
impl Shutdown for TaskAccepter {
    async fn shutdown(&self) {
        if let Some(join_handle) = self.join_handle.lock().await.take() {
            join_handle.abort();
            let _ = join_handle.fuse().await;
        }
    }
}

#[derive(Clone)]
struct Inner {
    senders: Arc<TokioMutex<HashMap<SessionType, mpsc::Sender<Session>>>>,
    tcp_connector: Arc<dyn ConnectionTcpAccepter + Send + Sync>,
    signer: Arc<OmniSigner>,
    random_bytes_provider: Arc<Mutex<dyn RandomBytesProvider + Send + Sync>>,
}

impl Inner {
    async fn accept(&self) -> Result<()> {
        let (stream, addr) = self.tcp_connector.accept().await?;

        let send_hello_message = HelloMessage { version: SessionVersion::V1 };
        stream.sender.lock().await.send_message(&send_hello_message).await?;
        let received_hello_message: HelloMessage = stream.receiver.lock().await.recv_message().await?;

        let version = send_hello_message.version | received_hello_message.version;

        if version.contains(SessionVersion::V1) {
            let send_nonce: [u8; 32] = self.random_bytes_provider.lock().get_bytes(32).as_slice().try_into()?;
            let send_challenge_message = V1ChallengeMessage { nonce: send_nonce };
            stream.sender.lock().await.send_message(&send_challenge_message).await?;
            let receive_challenge_message: V1ChallengeMessage = stream.receiver.lock().await.recv_message().await?;

            let send_signature = self.signer.sign(&receive_challenge_message.nonce)?;
            let send_signature_message = V1SignatureMessage { cert: send_signature };
            stream.sender.lock().await.send_message(&send_signature_message).await?;
            let received_signature_message: V1SignatureMessage = stream.receiver.lock().await.recv_message().await?;

            if received_signature_message.cert.verify(send_nonce.as_slice()).is_err() {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("Invalid signature"));
            }

            let received_session_request_message: V1RequestMessage = stream.receiver.lock().await.recv_message().await?;
            let typ = match received_session_request_message.request_type {
                V1RequestType::Unknown => {
                    return Err(Error::new(ErrorKind::UnsupportedType).with_message("unsupported request type"));
                }
                V1RequestType::NodeFinder => SessionType::NodeFinder,
                V1RequestType::FileExchanger => SessionType::FileExchanger,
            };
            if let Ok(permit) = self.senders.lock().await.get(&typ).unwrap().try_reserve() {
                let send_session_result_message = V1ResultMessage {
                    result_type: V1ResultType::Accept,
                };
                stream.sender.lock().await.send_message(&send_session_result_message).await?;

                let session = Session {
                    typ: typ.clone(),
                    address: OmniAddr::new(format!("tcp({addr})").as_str()),
                    handshake_type: SessionHandshakeType::Accepted,
                    cert: received_signature_message.cert,
                    stream,
                };
                permit.send(session);
            } else {
                let send_session_result_message = V1ResultMessage {
                    result_type: V1ResultType::Reject,
                };
                stream.sender.lock().await.send_message(&send_session_result_message).await?;
            }

            Ok(())
        } else {
            Err(Error::new(ErrorKind::UnsupportedType).with_message(format!("Unsupported session version: {}", version.bits())))
        }
    }
}
