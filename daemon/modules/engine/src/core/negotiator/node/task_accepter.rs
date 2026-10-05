use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::{
    select,
    sync::{Mutex as TokioMutex, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use omnius_core_base::{clock::Clock, sleeper::Sleeper};

use crate::{
    base::runtime::Shutdown,
    core::session::{
        SessionAccepter,
        model::{SessionHandshakeType, SessionType},
    },
    prelude::*,
};

use super::*;

#[derive(Clone)]
pub struct TaskAccepter {
    sessions: Arc<SessionRegistry>,
    session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
    session_accepter: Arc<SessionAccepter>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    option: NodeFinderOption,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    token: CancellationToken,
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

impl TaskAccepter {
    pub async fn new(
        sessions: Arc<SessionRegistry>,
        session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
        session_accepter: Arc<SessionAccepter>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: NodeFinderOption,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            sessions,
            session_sender,
            session_accepter,
            clock,
            sleeper,
            option,
            join_handle: Arc::new(TokioMutex::new(None)),
            token: CancellationToken::new(),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            while !this.token.is_cancelled() {
                select! {
                    _ = async {
                        this.sleeper.sleep(std::time::Duration::from_secs(1)).await;
                        let res = this.accept().await;
                        if let Err(e) = res {
                            warn!("{:?}", e);
                        }
                    } => {}
                    _ = this.token.cancelled() => break,
                }
            }
        }));

        Ok(())
    }

    async fn accept(&self) -> Result<()> {
        let session_count = self.sessions.count_by_handshake_type(&SessionHandshakeType::Accepted).await;
        if session_count >= self.option.max_accepted_session_count {
            return Ok(());
        }

        let session = self.session_accepter.accept(&SessionType::NodeFinder).await?;
        let status = SessionStatus::new(session, self.clock.clone());

        self.session_sender
            .lock()
            .await
            .send(status)
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::UnexpectedError))?;

        Ok(())
    }
}
