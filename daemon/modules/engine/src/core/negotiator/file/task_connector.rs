use std::sync::Arc;

use async_trait::async_trait;
use rand::seq::SliceRandom as _;
use tokio::{select, sync::Mutex as TokioMutex, task::JoinHandle};
use tracing::warn;

use omnius_core_base::sleeper::Sleeper;
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{
    base::runtime::Shutdown,
    core::session::{SessionConnector, model::SessionType},
    model::{AssetKey, NodeProfile},
    prelude::*,
};

use super::*;

/// Dials the nodes that hold the root hashes this node fetches or provides.
pub struct TaskConnector {
    ctx: Arc<ExchangeContext>,
    session_connector: Arc<SessionConnector>,
    sleeper: Arc<dyn Sleeper + Send + Sync>,
    rng: Arc<parking_lot::Mutex<dyn rand::Rng + Send + Sync>>,
    join_handles: TokioMutex<Vec<JoinHandle<()>>>,
}

#[async_trait]
impl Shutdown for TaskConnector {
    async fn shutdown(&self) {
        let handles = std::mem::take(&mut *self.join_handles.lock().await);
        for handle in handles {
            let _ = handle.await;
        }
    }
}

impl TaskConnector {
    pub fn new(
        ctx: Arc<ExchangeContext>,
        session_connector: Arc<SessionConnector>,
        sleeper: Arc<dyn Sleeper + Send + Sync>,
        rng: Arc<parking_lot::Mutex<dyn rand::Rng + Send + Sync>>,
    ) -> Arc<Self> {
        let v = Arc::new(Self {
            ctx,
            session_connector,
            sleeper,
            rng,
            join_handles: TokioMutex::new(Vec::new()),
        });
        let this = v.clone();
        let join_handle = tokio::spawn(async move {
            while !this.ctx.cancellation_token.is_cancelled() {
                select! {
                    _ = async {
                        this.sleeper.sleep(this.ctx.intervals.connect).await;
                        if let Err(error) = this.connect().await {
                            warn!(error_message = error.to_string(), "connect failed");
                        }
                    } => {}
                    _ = this.ctx.cancellation_token.cancelled() => break,
                }
            }
        });
        v.join_handles.try_lock().expect("the task was just created").push(join_handle);
        v
    }

    /// Dials one new peer, taking the fetch targets first in the order of their priority.
    /// Fetch and serve targets use separate connection slots.
    async fn connect(&self) -> Result<()> {
        let (fetching, mut serving) = {
            let root_hashes = self.ctx.root_hashes();
            (root_hashes.fetching().to_vec(), root_hashes.serving())
        };
        serving.shuffle(&mut *self.rng.lock());

        for (slots, root_hashes) in [(self.ctx.fetch_slots.clone(), fetching), (self.ctx.serve_slots.clone(), serving)] {
            for root_hash in root_hashes {
                let Ok(permit) = slots.clone().try_acquire_owned() else {
                    break;
                };
                if self.ctx.registry.count_by_root_hash(&root_hash) >= self.ctx.option.max_session_per_root_hash_count {
                    continue;
                }
                if let Some(session) = self.dial(&root_hash).await? {
                    self.ctx.start_session(session, permit);
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    async fn dial(&self, root_hash: &OmniHash) -> Result<Option<Arc<ExchangeSession>>> {
        let mut node_profiles = self.ctx.location_finder.find(&AssetKey::file(root_hash.clone())).await?;
        node_profiles.shuffle(&mut *self.rng.lock());

        for node_profile in node_profiles {
            let node_id = node_profile.id();
            if !self.is_candidate(node_id, root_hash) || !self.ctx.begin_dial(node_id, root_hash) {
                continue;
            }
            let session = self.dial_node(root_hash, &node_profile).await;
            self.ctx.end_dial(node_id, root_hash);
            if let Some(session) = session {
                return Ok(Some(session));
            }
        }

        Ok(None)
    }

    fn is_candidate(&self, node_id: &[u8], root_hash: &OmniHash) -> bool {
        node_id != self.ctx.my_node_id.as_slice()
            && !self.ctx.is_excluded(node_id)
            && !self.ctx.is_pair_set_aside(node_id, root_hash)
            && !self.ctx.registry.contains(node_id, root_hash)
    }

    async fn dial_node(&self, root_hash: &OmniHash, node_profile: &NodeProfile) -> Option<Arc<ExchangeSession>> {
        for addr in node_profile.addrs().iter() {
            let Ok(session) = self.session_connector.connect(addr, &SessionType::FileExchanger).await else {
                continue;
            };
            // The address may belong to another node now, which cannot be the node the lookup found.
            if !session.is_signed_by(node_profile) {
                warn!(address = addr.to_string(), "the peer's key does not match the node profile");
                continue;
            }
            match ExchangeHandshake::dial(&session, root_hash, self.ctx.intervals.receive_timeout()).await {
                Ok(()) => {
                    let id = self.ctx.next_session_id();
                    return Some(Arc::new(ExchangeSession::new(id, node_profile.id().to_vec(), root_hash.clone(), session)));
                }
                Err(error) if error.kind() == &ErrorKind::Reject => {
                    self.ctx.set_aside_pair(node_profile.id(), root_hash, self.ctx.intervals.reject_retry);
                    return None;
                }
                Err(error) => warn!(error_message = error.to_string(), "file exchange handshake failed"),
            }
        }

        None
    }
}
