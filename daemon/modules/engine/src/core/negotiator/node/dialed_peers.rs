use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;

/// The peers a connector dialed whose Session has not ended yet.
///
/// A peer stays here from the successful dial until the Session it produced ends, so the connectors do not dial the
/// same peer twice while its handshake or Session is in progress.
#[derive(Default)]
pub struct DialedPeers {
    peers: Mutex<HashMap<Vec<u8>, usize>>,
}

impl DialedPeers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one dial to the peer and returns the guard that releases it.
    pub fn reserve(self: &Arc<Self>, peer_id: &[u8]) -> DialedPeer {
        *self.peers.lock().entry(peer_id.to_vec()).or_default() += 1;
        DialedPeer {
            peers: self.clone(),
            id: peer_id.to_vec(),
        }
    }

    /// The peers this node dialed and whose Session has not ended yet.
    pub fn ids(&self) -> Vec<Vec<u8>> {
        self.peers.lock().keys().cloned().collect()
    }

    fn release(&self, peer_id: &[u8]) {
        let mut peers = self.peers.lock();
        let Some(count) = peers.get_mut(peer_id) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            peers.remove(peer_id);
        }
    }
}

/// Releases the reservation when the Session the dial produced ends, however it ends.
pub struct DialedPeer {
    peers: Arc<DialedPeers>,
    id: Vec<u8>,
}

impl Drop for DialedPeer {
    fn drop(&mut self) {
        self.peers.release(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::*;

    #[test]
    fn a_peer_stays_reserved_until_every_dial_ends() -> TestResult {
        let peers = Arc::new(DialedPeers::new());
        let first = peers.reserve(&[1]);
        let second = peers.reserve(&[1]);
        assert_eq!(peers.ids(), vec![vec![1]]);

        drop(first);
        assert_eq!(peers.ids(), vec![vec![1]], "the second dial still reserves the peer");

        drop(second);
        assert!(peers.ids().is_empty());
        Ok(())
    }

    #[test]
    fn releasing_an_unknown_peer_is_ignored() -> TestResult {
        let peers = Arc::new(DialedPeers::new());
        peers.release(&[1]);
        assert!(peers.ids().is_empty());
        Ok(())
    }
}
