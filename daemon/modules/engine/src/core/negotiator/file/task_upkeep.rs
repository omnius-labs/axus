use std::sync::Arc;

use async_trait::async_trait;
use tokio::{select, sync::Mutex as TokioMutex, task::JoinHandle};
use tracing::warn;

use omnius_core_base::sleeper::Sleeper;

use crate::base::runtime::Shutdown;

use super::*;

/// Keeps what the Session workers read up to date: the root hashes that are provided and fetched, and the nodes to unchoke.
pub struct TaskUpkeep {
    join_handles: TokioMutex<Vec<JoinHandle<()>>>,
}

#[async_trait]
impl Shutdown for TaskUpkeep {
    async fn shutdown(&self) {
        let handles = std::mem::take(&mut *self.join_handles.lock().await);
        for handle in handles {
            let _ = handle.await;
        }
    }
}

impl TaskUpkeep {
    pub fn new(ctx: Arc<ExchangeContext>, sleeper: Arc<dyn Sleeper + Send + Sync>) -> Arc<Self> {
        let refresh = tokio::spawn({
            let ctx = ctx.clone();
            let sleeper = sleeper.clone();
            async move {
                while !ctx.cancellation_token.is_cancelled() {
                    select! {
                        _ = async {
                            if let Err(error) = ctx.refresh_root_hashes().await {
                                warn!(error_message = error.to_string(), "refreshing the root hashes failed");
                            }
                            sleeper.sleep(ctx.intervals.connect).await;
                        } => {}
                        _ = ctx.cancellation_token.cancelled() => break,
                    }
                }
            }
        });
        let choke = tokio::spawn(async move {
            while !ctx.cancellation_token.is_cancelled() {
                select! {
                    _ = async {
                        ctx.decide_choke();
                        sleeper.sleep(ctx.intervals.status_cycle).await;
                    } => {}
                    _ = ctx.cancellation_token.cancelled() => break,
                }
            }
        });
        Arc::new(Self {
            join_handles: TokioMutex::new(vec![refresh, choke]),
        })
    }
}
