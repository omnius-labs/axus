use std::collections::VecDeque;

use parking_lot::Mutex;
use tokio::sync::Notify;

use omnius_core_omnikit::generated::omni_hash::OmniHash;

/// The next message that answers a request, in the order the requests arrived.
/// The presence bitmap of a request always precedes the blocks of that request, so the requesting node
/// can apply the bitmap of a request before any of its blocks arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServeItem {
    Presence(Vec<u8>),
    Block(OmniHash),
}

/// A request received from a peer that is not answered yet.
struct PendingAnswer {
    bits: Option<Vec<u8>>,
    block_hashes: VecDeque<OmniHash>,
}

/// The requests received from a peer that are not answered yet, answered one message at a time.
pub struct ServeQueue {
    max_requests: usize,
    requests: Mutex<VecDeque<PendingAnswer>>,
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

    /// The requests awaiting responses.
    pub fn len(&self) -> usize {
        self.requests.lock().len()
    }

    /// Adds a request with the bitmap that answers it. Returns false, without adding, when the peer already has
    /// the maximum number of requests awaiting responses.
    pub fn push(&self, bits: Vec<u8>, block_hashes: Vec<OmniHash>) -> bool {
        {
            let mut requests = self.requests.lock();
            if requests.len() >= self.max_requests {
                return false;
            }
            requests.push_back(PendingAnswer {
                bits: Some(bits),
                block_hashes: block_hashes.into(),
            });
        }
        self.notify.notify_one();
        true
    }

    /// Discards the requests, which a choked node does not answer.
    pub fn clear(&self) {
        self.requests.lock().clear();
    }

    /// Waits for the next message to answer, in the order the requests arrived.
    pub async fn next(&self) -> ServeItem {
        loop {
            if let Some(item) = self.pop() {
                return item;
            }
            self.notify.notified().await;
        }
    }

    fn pop(&self) -> Option<ServeItem> {
        let mut requests = self.requests.lock();
        loop {
            let (bits, block_hash, finished) = {
                let request = requests.front_mut()?;
                if request.bits.is_some() {
                    (request.bits.take(), None, false)
                } else if let Some(block_hash) = request.block_hashes.pop_front() {
                    (None, Some(block_hash), request.block_hashes.is_empty())
                } else {
                    (None, None, true)
                }
            };
            if let Some(bits) = bits {
                return Some(ServeItem::Presence(bits));
            }
            if let Some(block_hash) = block_hash {
                if finished {
                    requests.pop_front();
                }
                return Some(ServeItem::Block(block_hash));
            }
            requests.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

    use super::{ServeItem, ServeQueue};

    fn hash(value: u8) -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [value])
    }

    #[tokio::test]
    async fn the_bitmap_of_a_request_precedes_its_blocks_and_requests_are_answered_in_order() {
        let queue = ServeQueue::new(2);
        assert!(queue.push(vec![0b0000_0010], vec![hash(1)]));
        assert!(queue.push(vec![0b0000_0001], vec![hash(3)]));
        assert!(!queue.push(vec![0b0000_0001], vec![hash(4)]));

        assert_eq!(queue.next().await, ServeItem::Presence(vec![0b0000_0010]));
        assert_eq!(queue.next().await, ServeItem::Block(hash(1)));
        // The first request is answered completely, so a slot is free.
        assert!(queue.push(vec![0b0000_0001], vec![hash(5)]));
        assert_eq!(queue.next().await, ServeItem::Presence(vec![0b0000_0001]));
        assert_eq!(queue.next().await, ServeItem::Block(hash(3)));
        assert_eq!(queue.next().await, ServeItem::Presence(vec![0b0000_0001]));
        assert_eq!(queue.next().await, ServeItem::Block(hash(5)));
    }

    #[tokio::test]
    async fn a_request_with_no_block_still_answers_with_its_bitmap() {
        let queue = ServeQueue::new(2);
        assert!(queue.push(vec![0b0000_0000], Vec::new()));

        assert_eq!(queue.next().await, ServeItem::Presence(vec![0b0000_0000]));
        assert!(tokio::time::timeout(Duration::from_millis(50), queue.next()).await.is_err());
        assert_eq!(queue.len(), 0);
    }

    #[tokio::test]
    async fn next_waits_for_a_request_and_clear_discards_the_rest() {
        let queue = ServeQueue::new(4);
        assert!(tokio::time::timeout(Duration::from_millis(50), queue.next()).await.is_err());

        queue.push(vec![0b0000_0011], vec![hash(1), hash(2)]);
        queue.clear();
        assert!(tokio::time::timeout(Duration::from_millis(50), queue.next()).await.is_err());

        queue.push(vec![0b0000_0001], vec![hash(3)]);
        assert_eq!(queue.next().await, ServeItem::Presence(vec![0b0000_0001]));
        assert_eq!(queue.next().await, ServeItem::Block(hash(3)));
    }
}
