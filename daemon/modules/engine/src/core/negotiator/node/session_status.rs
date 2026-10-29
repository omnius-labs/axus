use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{core::negotiator::node::NodeMessage, core::session::model::Session, model::NodeProfile};

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
        }
    }

    /// The messages to write. The writer task takes this once.
    pub async fn take_outbox_receiver(&self) -> Option<mpsc::Receiver<NodeMessage>> {
        self.outbox_receiver.lock().await.take()
    }
}
