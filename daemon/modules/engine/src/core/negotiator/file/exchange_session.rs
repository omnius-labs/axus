use std::sync::atomic::{AtomicBool, Ordering};

use tokio_util::sync::CancellationToken;

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::core::session::model::{Session, SessionHandshakeType};

/// An established FileExchanger Session. It is unique per pair of node ID and root hash.
pub struct ExchangeSession {
    pub id: u64,
    pub node_id: Vec<u8>,
    pub root_hash: OmniHash,
    pub session: Session,
    pub cancellation_token: CancellationToken,
    peer_wants: AtomicBool,
}

impl ExchangeSession {
    pub fn new(id: u64, node_id: Vec<u8>, root_hash: OmniHash, session: Session) -> Self {
        Self {
            id,
            node_id,
            root_hash,
            session,
            cancellation_token: CancellationToken::new(),
            peer_wants: AtomicBool::new(false),
        }
    }

    pub fn handshake_type(&self) -> &SessionHandshakeType {
        &self.session.handshake_type
    }

    /// Whether the peer said it wants our blocks. A new Session starts as not wanting.
    pub fn peer_wants(&self) -> bool {
        self.peer_wants.load(Ordering::Relaxed)
    }

    pub fn set_peer_wants(&self, wants: bool) {
        self.peer_wants.store(wants, Ordering::Relaxed);
    }
}
