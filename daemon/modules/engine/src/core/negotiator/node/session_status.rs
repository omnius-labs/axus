use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{core::negotiator::node::NodeMessage, core::session::model::Session, model::NodeProfile};

use super::DialedPeer;

/// The work waiting for one Session's writer.
///
/// A task pushes a message when it has one, so a lookup answer does not wait for the communication cycle, and the writer sends a Status when the cycle passes with nothing to send.
const OUTBOX_CAPACITY: usize = 64;

#[derive(Clone)]
pub struct SessionStatus {
    pub session: Session,
    pub node_profile: Arc<Mutex<Option<NodeProfile>>>,
    pub outbox: mpsc::Sender<NodeMessage>,
    outbox_receiver: Arc<TokioMutex<Option<mpsc::Receiver<NodeMessage>>>>,
    /// 同じ相手との重複を解消するとき、この Session の送受信だけを止める
    pub cancellation_token: CancellationToken,
    /// Keeps the dialed peer recorded until this Session ends, however it ends.
    _dialed_peer: Option<Arc<DialedPeer>>,
}

impl SessionStatus {
    pub fn new(session: Session) -> Self {
        let (outbox, outbox_receiver) = mpsc::channel(OUTBOX_CAPACITY);
        Self {
            session,
            node_profile: Arc::new(Mutex::new(None)),
            outbox,
            outbox_receiver: Arc::new(TokioMutex::new(Some(outbox_receiver))),
            cancellation_token: CancellationToken::new(),
            _dialed_peer: None,
        }
    }

    /// Records that this node dialed the Session's peer, and releases the record when the Session ends.
    ///
    /// Without the release the peer would stay out of the connector's candidates after the Session ended, so a Session
    /// that failed would keep the two nodes apart until something else released the record.
    pub fn with_dialed_peer(mut self, dialed_peer: DialedPeer) -> Self {
        self._dialed_peer = Some(Arc::new(dialed_peer));
        self
    }

    /// The messages to write. The writer task takes this once.
    pub async fn take_outbox_receiver(&self) -> Option<mpsc::Receiver<NodeMessage>> {
        self.outbox_receiver.lock().await.take()
    }
}
