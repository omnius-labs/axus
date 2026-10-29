use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex as TokioMutex, oneshot};

use crate::model::NodeProfile;

/// The answer to one lookup request, once the peer replied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupAnswer {
    pub locations: Vec<Arc<NodeProfile>>,
    pub closer_nodes: Vec<Arc<NodeProfile>>,
}

/// The pending answers, keyed by the peer's node ID and the lookup ID.
type Pending = HashMap<(Vec<u8>, u64), oneshot::Sender<LookupAnswer>>;

/// Correlates a lookup request with its answer on one Session.
///
/// The peer echoes the lookup ID inside the same Session, so the pair of the peer's node ID and the ID identifies the request without a separate connection.
#[derive(Default)]
pub struct LookupRegistry {
    pending: TokioMutex<Pending>,
}

impl LookupRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts waiting for the answer to `lookup_id` from `peer`.
    pub async fn register(&self, peer: &[u8], lookup_id: u64) -> oneshot::Receiver<LookupAnswer> {
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert((peer.to_vec(), lookup_id), sender);
        receiver
    }

    /// Stops waiting, which the lookup does when the answer arrives too late.
    pub async fn remove(&self, peer: &[u8], lookup_id: u64) {
        self.pending.lock().await.remove(&(peer.to_vec(), lookup_id));
    }

    /// Hands the answer to the lookup that waits for it. Returns whether a lookup was waiting.
    pub async fn complete(&self, peer: &[u8], lookup_id: u64, answer: LookupAnswer) -> bool {
        match self.pending.lock().await.remove(&(peer.to_vec(), lookup_id)) {
            Some(sender) => sender.send(answer).is_ok(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use omnius_core_omnikit::model::omni_addr::OmniAddr;
    use testresult::TestResult;

    use super::*;

    fn profile(name: &str) -> Arc<NodeProfile> {
        Arc::new(NodeProfile::random_for_test(vec![OmniAddr::new(name)]))
    }

    #[tokio::test]
    async fn an_answer_reaches_the_lookup_that_registered_the_id() -> TestResult {
        let registry = LookupRegistry::new();
        let mut receiver = registry.register(b"peer", 7).await;

        let answer = LookupAnswer {
            locations: vec![profile("a")],
            closer_nodes: vec![],
        };
        assert!(registry.complete(b"peer", 7, answer.clone()).await);
        assert_eq!(receiver.try_recv()?, answer);
        Ok(())
    }

    #[tokio::test]
    async fn an_answer_from_another_peer_is_not_delivered() -> TestResult {
        let registry = LookupRegistry::new();
        let _receiver = registry.register(b"peer", 7).await;

        let answer = LookupAnswer {
            locations: vec![profile("a")],
            closer_nodes: vec![],
        };
        assert!(!registry.complete(b"other", 7, answer).await);
        Ok(())
    }

    #[tokio::test]
    async fn a_removed_id_is_not_delivered() -> TestResult {
        let registry = LookupRegistry::new();
        let _receiver = registry.register(b"peer", 7).await;
        registry.remove(b"peer", 7).await;

        let answer = LookupAnswer {
            locations: vec![],
            closer_nodes: vec![],
        };
        assert!(!registry.complete(b"peer", 7, answer).await);
        Ok(())
    }
}
