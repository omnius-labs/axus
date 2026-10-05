use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use tokio::{
    select,
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::warn;

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
    sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
    session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
    session_accepter: Arc<SessionAccepter>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    option: FileExchangerOption,
    join_handles: Arc<TokioMutex<Vec<JoinHandle<()>>>>,
    token: CancellationToken,
}

#[async_trait]
impl Shutdown for TaskAccepter {
    async fn shutdown(&self) {
        self.token.cancel();
        let handles = std::mem::take(&mut *self.join_handles.lock().await);
        for handle in handles {
            let _ = handle.await;
        }
    }
}

impl TaskAccepter {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
        session_sender: Arc<TokioMutex<mpsc::Sender<SessionStatus>>>,
        session_accepter: Arc<SessionAccepter>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        option: FileExchangerOption,
    ) -> Result<Arc<Self>> {
        let v = Arc::new(Self {
            sessions,
            session_sender,
            session_accepter,
            sleeper,
            clock,
            option,
            join_handles: Arc::new(TokioMutex::new(vec![])),
            token: CancellationToken::new(),
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        let join_handle = tokio::spawn(async move {
            while !this.token.is_cancelled() {
                select! {
                    _ = async {
                        this.sleeper.sleep(std::time::Duration::from_secs(1)).await;
                        let res = this.accept().await;
                        if let Err(e) = res {
                            warn!(error_message = e.to_string(), "connect failed");
                        }
                    } => {}
                    _ = this.token.cancelled() => break,
                }
            }
        });
        self.join_handles.lock().await.push(join_handle);

        Ok(())
    }

    async fn accept(&self) -> Result<()> {
        let session_count = self
            .sessions
            .read()
            .await
            .iter()
            .filter(|(_, status)| status.session.handshake_type == SessionHandshakeType::Accepted)
            .count();
        if session_count >= self.option.max_accepted_session_count {
            return Ok(());
        }

        let session = self.session_accepter.accept(&SessionType::FileExchanger).await?;
        let status = SessionStatus::new(ExchangeType::Unknown, session, None, self.clock.clone());

        self.session_sender
            .lock()
            .await
            .send(status)
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::UnexpectedError))?;

        Ok(())
    }
}
