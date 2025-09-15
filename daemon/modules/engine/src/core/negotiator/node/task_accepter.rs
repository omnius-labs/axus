use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use futures::FutureExt;
use tokio::{
    sync::{Mutex as TokioMutex, RwLock as TokioRwLock, mpsc},
    task::JoinHandle,
};

use omnius_core_base::{clock::Clock, sleeper::Sleeper};

use crate::{
    base::Shutdown,
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
    option: NodeFinderOption,
    join_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
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

impl TaskAccepter {
    pub async fn new(
        sessions: Arc<TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>>,
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
        });

        v.clone().start().await?;

        Ok(v)
    }

    async fn start(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        *self.join_handle.lock().await = Some(tokio::spawn(async move {
            loop {
                this.sleeper.sleep(std::time::Duration::from_secs(1)).await;
                let res = this.accept().await;
                if let Err(e) = res {
                    warn!("{:?}", e);
                }
            }
        }));

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

        let session = self.session_accepter.accept(&SessionType::NodeFinder).await?;
        let status = SessionStatus::new(session, self.clock.clone());

        self.session_sender
            .lock()
            .await
            .send(status)
            .await
            .map_err(|e| Error::builder().kind(ErrorKind::UnexpectedError).source(e).build())?;

        Ok(())
    }
}
