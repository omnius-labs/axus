use std::sync::Arc;

use async_trait::async_trait;
use tokio::{select, sync::Mutex as TokioMutex, task::JoinHandle};
use tracing::warn;

use crate::{
    base::runtime::Shutdown,
    core::session::{SessionAccepter, model::SessionType},
    prelude::*,
};

use super::*;

/// Accepts the Sessions that peers dialed, and keeps the ones for the root hashes this node provides.
pub struct TaskAccepter {
    ctx: Arc<ExchangeContext>,
    session_accepter: Arc<SessionAccepter>,
    join_handles: TokioMutex<Vec<JoinHandle<()>>>,
}

#[async_trait]
impl Shutdown for TaskAccepter {
    async fn shutdown(&self) {
        let handles = std::mem::take(&mut *self.join_handles.lock().await);
        for handle in handles {
            let _ = handle.await;
        }
    }
}

impl TaskAccepter {
    pub fn new(ctx: Arc<ExchangeContext>, session_accepter: Arc<SessionAccepter>) -> Arc<Self> {
        let v = Arc::new(Self {
            ctx,
            session_accepter,
            join_handles: TokioMutex::new(Vec::new()),
        });
        let this = v.clone();
        let join_handle = tokio::spawn(async move {
            while !this.ctx.cancellation_token.is_cancelled() {
                select! {
                    res = this.accept() => {
                        if let Err(error) = res {
                            warn!(error_message = error.to_string(), "accept failed");
                            // A failing accepter must not spin.
                            tokio::time::sleep(this.ctx.intervals.connect).await;
                        }
                    }
                    _ = this.ctx.cancellation_token.cancelled() => break,
                }
            }
        });
        v.join_handles.try_lock().expect("the task was just created").push(join_handle);
        v
    }

    async fn accept(&self) -> Result<()> {
        // A permit is taken before accepting, so the accepted Sessions never exceed the limit.
        let permit = self
            .ctx
            .accept_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| Error::from_error(e, ErrorKind::UnexpectedError))?;
        let session = self.session_accepter.accept(&SessionType::FileExchanger).await?;

        // The handshake waits for the peer, so it must not stop the next accept.
        let ctx = self.ctx.clone();
        let mut workers = self.join_handles.lock().await;
        workers.retain(|handle| !handle.is_finished());
        workers.push(tokio::spawn(async move {
            let node_id = session.peer_node_id();
            let result = select! {
                result = ExchangeHandshake::accept(&session, ctx.intervals.receive_timeout(), |root_hash| ctx.can_exchange(&node_id, root_hash)) => result,
                _ = ctx.cancellation_token.cancelled() => return,
            };
            match result {
                Ok(root_hash) => {
                    let id = ctx.next_session_id();
                    ctx.start_session(Arc::new(ExchangeSession::new(id, node_id, root_hash, session)), permit);
                }
                Err(error) => warn!(error_message = error.to_string(), "file exchange handshake failed"),
            }
        }));

        Ok(())
    }
}
