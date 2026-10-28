use std::collections::VecDeque;

use parking_lot::Mutex;
use tokio::sync::Notify;

use omnius_core_omnikit::generated::omni_hash::OmniHash;

/// The requests received from a peer that are not answered yet, answered one block at a time.
pub struct ServeQueue {
    max_requests: usize,
    requests: Mutex<VecDeque<VecDeque<OmniHash>>>,
    notify: Notify,
}

impl ServeQueue {
    pub fn new(max_requests: usize) -> Self {
        Self {
            max_requests,
            requests: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
        }
    }

    /// Adds a request. Returns false, without adding, when the peer already has the maximum number of requests awaiting responses.
    pub fn push(&self, block_hashes: Vec<OmniHash>) -> bool {
        {
            let mut requests = self.requests.lock();
            if requests.len() >= self.max_requests {
                return false;
            }
            requests.push_back(block_hashes.into());
        }
        self.notify.notify_one();
        true
    }

    /// Discards the requests, which a choked node does not answer.
    pub fn clear(&self) {
        self.requests.lock().clear();
    }

    /// Waits for the next hash to answer, in the order the requests arrived.
    pub async fn next(&self) -> OmniHash {
        loop {
            if let Some(block_hash) = self.pop() {
                return block_hash;
            }
            self.notify.notified().await;
        }
    }

    fn pop(&self) -> Option<OmniHash> {
        let mut requests = self.requests.lock();
        while let Some(request) = requests.front_mut() {
            if let Some(block_hash) = request.pop_front() {
                if request.is_empty() {
                    requests.pop_front();
                }
                return Some(block_hash);
            }
            requests.pop_front();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

    use super::ServeQueue;

    fn hash(value: u8) -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [value])
    }

    #[tokio::test]
    async fn hashes_are_answered_in_request_order_and_a_full_queue_refuses_requests() {
        let queue = ServeQueue::new(2);
        assert!(queue.push(vec![hash(1), hash(2)]));
        assert!(queue.push(vec![hash(3)]));
        assert!(!queue.push(vec![hash(4)]));

        assert_eq!(queue.next().await, hash(1));
        assert_eq!(queue.next().await, hash(2));
        // The first request is answered completely, so a slot is free.
        assert!(queue.push(vec![hash(5)]));
        assert_eq!(queue.next().await, hash(3));
        assert_eq!(queue.next().await, hash(5));
    }

    #[tokio::test]
    async fn next_waits_for_a_request_and_clear_discards_the_rest() {
        let queue = ServeQueue::new(4);
        assert!(tokio::time::timeout(Duration::from_millis(50), queue.next()).await.is_err());

        queue.push(vec![hash(1), hash(2)]);
        queue.clear();
        assert!(tokio::time::timeout(Duration::from_millis(50), queue.next()).await.is_err());

        queue.push(vec![hash(3)]);
        assert_eq!(queue.next().await, hash(3));
    }
}
