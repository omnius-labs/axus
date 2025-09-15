use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use tokio::{
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc},
    task::JoinHandle,
};
use tracing::warn;

use omnius_core_base::{clock::Clock, sleeper::Sleeper};

use crate::{
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
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        let join_handle = tokio::spawn(async move {
            loop {
                this.sleeper.sleep(std::time::Duration::from_secs(1)).await;
                let res = this.accept().await;
                if let Err(e) = res {
                    warn!(error_message = e.to_string(), "connect failed");
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
            .map_err(|e| Error::builder().kind(ErrorKind::UnexpectedError).source(e).build())?;

        Ok(())
    }
}
