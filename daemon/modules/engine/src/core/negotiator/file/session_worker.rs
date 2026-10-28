use std::{collections::HashSet, sync::Arc};

use tokio::{
    select,
    sync::mpsc,
    time::{Instant, MissedTickBehavior, interval, timeout},
};

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{
    base::connection::{FramedRecvExt as _, FramedSendExt as _, FramedStream},
    generated::axus::file::{ExchangeMessage, MAX_BLOCK_LENGTH, MAX_PENDING_REQUESTS},
    prelude::*,
    protocol::file::ExchangeMessageCodec,
};

use super::*;

/// Why a Session worker stopped.
#[derive(Debug, PartialEq, Eq)]
enum Closing {
    /// The Session ended, was replaced, or the exchanger stopped.
    Normal,
    /// The peer broke the protocol.
    Violation(&'static str),
    /// The peer did not respond in time.
    Timeout(&'static str),
    /// Neither side wanted the other's blocks for a while.
    Unwanted,
}

/// What the receiving side of a worker remembers about the peer and the requests in flight.
struct WorkerState {
    peer_choking: bool,
    /// What the last status message conveyed. A Session starts as choking the peer and not wanting its blocks.
    conveyed_want: bool,
    conveyed_choke: bool,
    status_sent_at: Option<Instant>,
    pending: Vec<HashSet<OmniHash>>,
    last_progress: Instant,
    unwanted_since: Option<Instant>,
}

impl WorkerState {
    fn new(now: Instant) -> Self {
        Self {
            peer_choking: true,
            conveyed_want: false,
            conveyed_choke: true,
            status_sent_at: None,
            pending: Vec::new(),
            last_progress: now,
            unwanted_since: None,
        }
    }
}

/// Sends and receives the messages of one Session. Which block to request is decided by `ExchangeContext`.
pub struct SessionWorker {
    ctx: Arc<ExchangeContext>,
    session: Arc<ExchangeSession>,
    serve_queue: ServeQueue,
}

impl SessionWorker {
    /// The number of blocks one request asks for, which is below the limit of the protocol.
    const REQUEST_BLOCK_COUNT: usize = 16;
    const CONTROL_QUEUE_LENGTH: usize = 32;

    pub fn new(ctx: Arc<ExchangeContext>, session: Arc<ExchangeSession>) -> Self {
        Self {
            ctx,
            session,
            serve_queue: ServeQueue::new(MAX_PENDING_REQUESTS as usize),
        }
    }

    /// Runs until the Session ends, then releases everything the Session held.
    pub async fn run(self) {
        let this = Arc::new(self);
        let (control_tx, control_rx) = mpsc::channel(Self::CONTROL_QUEUE_LENGTH);
        let send_loop = tokio::spawn({
            let this = this.clone();
            async move { this.send_loop(control_rx).await }
        });

        let closing = this.receive_loop(control_tx).await;
        this.session.cancellation_token.cancel();
        let _ = send_loop.await;

        let node_id = &this.session.node_id;
        let root_hash = &this.session.root_hash;
        match closing {
            Closing::Normal => debug!(session_id = this.session.id, "file exchange session closed"),
            Closing::Violation(reason) => {
                warn!(session_id = this.session.id, reason, "file exchange session closed by a violation");
                this.ctx.exclude_for_violation(node_id);
            }
            Closing::Timeout(reason) => {
                warn!(session_id = this.session.id, reason, "file exchange session closed by a timeout");
                this.ctx.exclude_for_timeout(node_id);
            }
            Closing::Unwanted => {
                debug!(session_id = this.session.id, "file exchange session closed because neither side wants blocks");
                this.ctx.set_aside_pair(node_id, root_hash, this.ctx.intervals.unwanted_cooldown);
            }
        }
        this.ctx.release_session_blocks(root_hash, this.session.id);
        this.ctx.registry.unregister(&this.session);
    }

    async fn send_loop(&self, mut control_rx: mpsc::Receiver<ExchangeMessage>) {
        let token = &self.session.cancellation_token;
        loop {
            let message = select! {
                biased;
                _ = token.cancelled() => break,
                _ = self.ctx.cancellation_token.cancelled() => break,
                control = control_rx.recv() => match control {
                    Some(message) => message,
                    None => break,
                },
                block_hash = self.serve_queue.next() => self.answer(block_hash).await,
            };
            let bytes = match &message {
                ExchangeMessage::Block { value, .. } => Some(value.len() as u64),
                _ => None,
            };
            // A peer that stops reading must not hold the Session forever.
            match timeout(self.ctx.intervals.request_timeout, self.send(&message)).await {
                Ok(Ok(())) => {}
                _ => break,
            }
            if let Some(bytes) = bytes {
                self.ctx.record_sent(&self.session.node_id, bytes);
            }
        }
        token.cancel();
    }

    /// The block for the requested hash, or the do-not-have response.
    async fn answer(&self, block_hash: OmniHash) -> ExchangeMessage {
        match self.ctx.read_block(&self.session.root_hash, &block_hash).await {
            Ok(Some(value)) => ExchangeMessage::Block { block_hash, value },
            Ok(None) => ExchangeMessage::NotHave { block_hash },
            Err(error) => {
                warn!(error_message = error.to_string(), "reading a block to provide failed");
                ExchangeMessage::NotHave { block_hash }
            }
        }
    }

    async fn send(&self, message: &ExchangeMessage) -> Result<()> {
        self.session.session.stream.sender.lock().await.send_message_with::<ExchangeMessageCodec>(message).await
    }

    async fn receive(&self) -> Result<ExchangeMessage> {
        self.session.session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await
    }

    async fn receive_loop(&self, control_tx: mpsc::Sender<ExchangeMessage>) -> Closing {
        let mut state = WorkerState::new(Instant::now());
        let mut tick = interval(self.ctx.intervals.poll);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let closing = select! {
                _ = self.session.cancellation_token.cancelled() => return Closing::Normal,
                _ = self.ctx.cancellation_token.cancelled() => return Closing::Normal,
                _ = tick.tick() => self.on_tick(&mut state, &control_tx).await,
                received = self.receive() => match received {
                    Ok(message) => self.on_message(&mut state, &control_tx, message).await,
                    Err(error) => Some(Self::closing_by_receive_error(&error)),
                },
            };
            if let Some(closing) = closing {
                return closing;
            }
        }
    }

    fn closing_by_receive_error(error: &Error) -> Closing {
        if FramedStream::is_receive_timeout(error) {
            return Closing::Timeout("no message within the receive deadline");
        }
        match error.kind() {
            ErrorKind::InvalidFormat | ErrorKind::SerdeError => Closing::Violation("invalid message or frame over the limit"),
            _ => Closing::Normal,
        }
    }

    async fn on_tick(&self, state: &mut WorkerState, control_tx: &mpsc::Sender<ExchangeMessage>) -> Option<Closing> {
        let now = Instant::now();
        let want = self.ctx.root_hashes().fetches(&self.session.root_hash);
        let choke = self.ctx.is_choked(&self.session.node_id);

        // The requests of a node we choke are discarded before the choke is conveyed.
        if choke && !state.conveyed_choke {
            self.serve_queue.clear();
        }
        let changed = want != state.conveyed_want || choke != state.conveyed_choke;
        let due = state
            .status_sent_at
            .is_none_or(|sent_at| now.saturating_duration_since(sent_at) >= self.ctx.intervals.status_cycle);
        if (changed || due) && control_tx.try_send(ExchangeMessage::Status { want, choke }).is_ok() {
            state.conveyed_want = want;
            state.conveyed_choke = choke;
            state.status_sent_at = Some(now);
        }

        if !want && !self.session.peer_wants() {
            let since = *state.unwanted_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= self.ctx.intervals.unwanted_close {
                return Some(Closing::Unwanted);
            }
        } else {
            state.unwanted_since = None;
        }

        // The deadline counts from the last response, so a Session receiving blocks one after another on a slow line stays open.
        if !state.pending.is_empty() && now.saturating_duration_since(state.last_progress) >= self.ctx.intervals.request_timeout {
            return Some(Closing::Timeout("no response within the request deadline"));
        }

        self.fill_requests(state, control_tx, want).await;
        None
    }

    /// Sends requests until the requests awaiting responses reach the limit or nothing is left to assign.
    async fn fill_requests(&self, state: &mut WorkerState, control_tx: &mpsc::Sender<ExchangeMessage>, want: bool) {
        while want && !state.peer_choking && state.pending.len() < MAX_PENDING_REQUESTS as usize {
            let assigned = match self
                .ctx
                .assign_blocks(&self.session.root_hash, self.session.id, &self.session.node_id, Self::REQUEST_BLOCK_COUNT)
                .await
            {
                Ok(assigned) => assigned,
                Err(error) => {
                    warn!(error_message = error.to_string(), "assigning blocks failed");
                    return;
                }
            };
            if assigned.is_empty() {
                return;
            }
            let request = ExchangeMessage::Request { block_hashes: assigned.clone() };
            if control_tx.try_send(request).is_err() {
                self.ctx.release_blocks(&self.session.root_hash, assigned);
                return;
            }
            if state.pending.is_empty() {
                state.last_progress = Instant::now();
            }
            state.pending.push(assigned.into_iter().collect());
        }
    }

    async fn on_message(&self, state: &mut WorkerState, control_tx: &mpsc::Sender<ExchangeMessage>, message: ExchangeMessage) -> Option<Closing> {
        match message {
            ExchangeMessage::Status { want, choke } => {
                self.session.set_peer_wants(want);
                if choke {
                    // A choke removes every awaiting response, so the blocks go back to other Sessions.
                    self.release_pending(state);
                } else if state.peer_choking {
                    state.last_progress = Instant::now();
                }
                state.peer_choking = choke;
                None
            }
            ExchangeMessage::Request { block_hashes } => {
                // A request may cross our choke notification, so it is ignored without blaming the peer.
                if state.conveyed_choke {
                    return None;
                }
                if !self.serve_queue.push(block_hashes) {
                    return Some(Closing::Violation("too many requests awaiting responses"));
                }
                None
            }
            ExchangeMessage::Block { block_hash, value } => {
                if let Some(closing) = self.on_block(state, block_hash, value).await {
                    return Some(closing);
                }
                let want = self.ctx.root_hashes().fetches(&self.session.root_hash);
                self.fill_requests(state, control_tx, want).await;
                None
            }
            ExchangeMessage::NotHave { block_hash } => {
                if Self::take_pending(state, &block_hash) {
                    self.ctx.mark_not_have(&self.session.root_hash, &self.session.node_id, &block_hash);
                    state.last_progress = Instant::now();
                }
                None
            }
            ExchangeMessage::Hello { .. } | ExchangeMessage::Open { .. } | ExchangeMessage::Accept | ExchangeMessage::Reject => {
                Some(Closing::Violation("message that belongs to the handshake"))
            }
        }
    }

    async fn on_block(&self, state: &mut WorkerState, block_hash: OmniHash, value: Vec<u8>) -> Option<Closing> {
        if !Self::take_pending(state, &block_hash) {
            return Some(Closing::Violation("block that was not requested"));
        }
        state.last_progress = Instant::now();
        if value.len() > MAX_BLOCK_LENGTH as usize {
            return Some(Closing::Violation("block over the maximum length"));
        }

        let bytes = value.len() as u64;
        match self.ctx.file_subscriber.write_block(&self.session.root_hash, &block_hash, value.into()).await {
            Ok(WriteBlockResult::Stored) => {
                self.ctx.record_received(&self.session.node_id, bytes);
                self.ctx.complete_block(&self.session.root_hash, &block_hash);
                None
            }
            Ok(WriteBlockResult::Unneeded) => {
                self.ctx.complete_block(&self.session.root_hash, &block_hash);
                None
            }
            Ok(WriteBlockResult::HashMismatch) => Some(Closing::Violation("block whose content hash does not match")),
            Err(error) => {
                warn!(error_message = error.to_string(), "saving a received block failed");
                self.ctx.release_blocks(&self.session.root_hash, [block_hash]);
                None
            }
        }
    }

    /// Removes the hash from the request awaiting its response. Returns false when no request is awaiting it.
    fn take_pending(state: &mut WorkerState, block_hash: &OmniHash) -> bool {
        let Some(index) = state.pending.iter().position(|request| request.contains(block_hash)) else {
            return false;
        };
        state.pending[index].remove(block_hash);
        if state.pending[index].is_empty() {
            state.pending.remove(index);
        }
        true
    }

    fn release_pending(&self, state: &mut WorkerState) {
        let released: Vec<OmniHash> = state.pending.drain(..).flatten().collect();
        self.ctx.release_blocks(&self.session.root_hash, released);
    }
}
