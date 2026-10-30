use std::{collections::HashSet, sync::Arc};

use tokio::{
    select,
    sync::mpsc,
    time::{Instant, MissedTickBehavior, interval, timeout},
};

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{
    base::connection::{FramedRecvExt as _, FramedSendExt as _, FramedStream},
    generated::axus::file::{ExchangeMessage, MAX_BLOCK_LENGTH, MAX_PENDING_REQUESTS, MAX_REQUEST_BLOCK_HASHES},
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

/// A request waiting for its presence bitmap and blocks.
/// The bitmap indexes the blocks in the order of the request, so the order is kept until the request ends.
struct PendingRequest {
    blocks: Vec<(OmniHash, u32)>,
    remaining: HashSet<OmniHash>,
    /// The hashes the presence bitmap said the peer holds, among the ones this Session still wants.
    /// A hash here that never arrives is a block the peer promised and did not send.
    claimed: HashSet<OmniHash>,
    /// When the request last received an answer, which decides when it is retracted for staying unanswered.
    last_answer_at: Instant,
    /// Whether the presence bitmap of this request was applied. It is not until then, because the bitmap
    /// answers the oldest request and has to be matched with it even when every block was retracted meanwhile.
    bitmap_applied: bool,
}

impl PendingRequest {
    fn new(blocks: Vec<(OmniHash, u32)>, sent_at: Instant) -> Self {
        let remaining = blocks.iter().map(|(hash, _)| hash.clone()).collect();
        Self {
            blocks,
            remaining,
            claimed: HashSet::new(),
            last_answer_at: sent_at,
            bitmap_applied: false,
        }
    }

    /// Whether nothing is left to answer and the request can be dropped.
    fn is_done(&self) -> bool {
        self.bitmap_applied && self.remaining.is_empty()
    }

    /// Whether the request still waits for a block. A request whose blocks were all retracted before its presence
    /// bitmap arrived keeps a shell, which answers the bitmap but does not take a request slot.
    fn is_active(&self) -> bool {
        !self.remaining.is_empty()
    }

    /// The rank of the block, when the request is still waiting for it.
    fn take(&mut self, block_hash: &OmniHash, now: Instant) -> Option<u32> {
        if !self.remaining.remove(block_hash) {
            return None;
        }
        self.last_answer_at = now;
        self.blocks.iter().find(|(hash, _)| hash == block_hash).map(|(_, rank)| *rank)
    }
}

/// The presence bitmap of a request: one bit per requested hash in the order of the request,
/// where a set bit means the peer holds the block. Bit `i` of hash `i` is the bit `i % 8` of byte `i / 8`.
struct PresenceBitmap;

impl PresenceBitmap {
    fn encode(present: &[bool]) -> Vec<u8> {
        let mut bits = vec![0u8; present.len().div_ceil(8)];
        for (index, present) in present.iter().enumerate() {
            if *present {
                bits[index / 8] |= 1 << (index % 8);
            }
        }
        bits
    }

    fn decode(bits: &[u8], count: usize) -> Vec<bool> {
        (0..count).map(|index| bits.get(index / 8).is_some_and(|byte| byte & (1 << (index % 8)) != 0)).collect()
    }
}

/// What the receiving side of a worker remembers about the peer and the requests in flight.
struct WorkerState {
    peer_choking: bool,
    /// What the last status message conveyed. A Session starts as choking the peer and not wanting its blocks.
    conveyed_want: bool,
    conveyed_choke: bool,
    status_sent_at: Option<Instant>,
    pending: Vec<PendingRequest>,
    /// The hashes this Session retracted with a cancel. A block for one of them may already be in flight,
    /// so it is dropped until the grace ends instead of being a violation.
    retracted: Expirations<OmniHash>,
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
            retracted: Expirations::new(),
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
                    Some(message) => Some(message),
                    None => break,
                },
                item = self.serve_queue.next() => match item {
                    ServeItem::Presence(bits) => Some(ExchangeMessage::HaveMap { bits }),
                    ServeItem::Block(block_hash) => self.answer(block_hash).await,
                },
            };
            let Some(message) = message else {
                continue;
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

    /// The block for the requested hash. Nothing is sent when the block cannot be read, although the presence
    /// bitmap named it, and the requester retries it later.
    async fn answer(&self, block_hash: OmniHash) -> Option<ExchangeMessage> {
        match self.ctx.read_block(&self.session.root_hash, &block_hash).await {
            Ok(Some(value)) => Some(ExchangeMessage::Block { block_hash, value }),
            Ok(None) => None,
            Err(error) => {
                warn!(error_message = error.to_string(), "reading a block to provide failed");
                None
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

        self.retract_blocks(state, control_tx, now);
        self.fill_requests(state, control_tx, want).await;
        None
    }

    /// Retracts the blocks the Session no longer needs: the ones another Session saved first, and the ones whose
    /// request waited past the retraction time. The peer stops sending them, and the blocks go to other Sessions.
    fn retract_blocks(&self, state: &mut WorkerState, control_tx: &mpsc::Sender<ExchangeMessage>, now: Instant) {
        // A peer that never answers never reaches the drop path of a block, so the grace is pruned here too.
        state.retracted.prune(now);
        let retract_after = self.ctx.intervals.request_retract_after;
        // The blocks another Session saved first, and the blocks this Session waited for past the retraction time.
        let mut retracted: Vec<OmniHash> = Vec::new();
        let mut undelivered: Vec<(OmniHash, u32)> = Vec::new();
        for request in &state.pending {
            let unanswered = now.saturating_duration_since(request.last_answer_at) >= retract_after;
            for (hash, rank) in &request.blocks {
                if !request.remaining.contains(hash) {
                    continue;
                }
                if unanswered {
                    // A block the peer promised and did not send stops the rank for that node; a block of a request
                    // without a bitmap yet is only given back, because the peer never said it holds it.
                    if request.claimed.contains(hash) {
                        undelivered.push((hash.clone(), *rank));
                    }
                    retracted.push(hash.clone());
                } else if !self.ctx.is_assigned(&self.session.root_hash, self.session.id, hash) {
                    retracted.push(hash.clone());
                }
            }
        }
        if retracted.is_empty() {
            return;
        }

        for hash in &retracted {
            state.retracted.insert(hash.clone(), now + self.ctx.intervals.cancel_grace);
        }
        for request in &mut state.pending {
            for hash in &retracted {
                request.remaining.remove(hash);
            }
        }
        state.pending.retain(|request| !request.is_done());

        if !undelivered.is_empty() {
            let ranks: Vec<u32> = undelivered.iter().map(|(_, rank)| *rank).collect();
            let block_hashes: Vec<OmniHash> = undelivered.iter().map(|(hash, _)| hash.clone()).collect();
            self.ctx
                .mark_rank_absent(&self.session.root_hash, self.session.id, &self.session.node_id, ranks, block_hashes.clone());
            // The remaining retracted blocks go back to the assignment without stopping their ranks.
            let rest: Vec<OmniHash> = retracted.iter().filter(|hash| !block_hashes.contains(hash)).cloned().collect();
            if !rest.is_empty() {
                self.ctx.release_blocks(&self.session.root_hash, self.session.id, rest);
            }
        } else {
            // The blocks go back to the assignment, so another Session, or this one later, can ask for them again.
            self.ctx.release_blocks(&self.session.root_hash, self.session.id, retracted.iter().cloned());
        }

        // One message carries at most the hashes a request may carry, so a tick with many retracted
        // blocks sends several cancels.
        for block_hashes in retracted.chunks(MAX_REQUEST_BLOCK_HASHES as usize) {
            if control_tx
                .try_send(ExchangeMessage::Cancel {
                    block_hashes: block_hashes.to_vec(),
                })
                .is_err()
            {
                // The cancel is best effort: the block is dropped when it arrives within the grace anyway.
                debug!(session_id = self.session.id, "a cancel could not be queued");
            }
        }
    }

    /// Applies the presence bitmap of the oldest request and releases the blocks the peer does not hold.
    fn on_have_map(&self, state: &mut WorkerState, bits: &[u8]) -> Option<Closing> {
        let (absent, present) = {
            // A bitmap can arrive for a request this node already dropped, for example when a choke cleared it
            // before the answer was sent. It answers nothing, so it is ignored.
            let Some(request) = state.pending.first_mut() else {
                debug!(session_id = self.session.id, "a presence bitmap answers no request");
                return None;
            };
            if request.bitmap_applied {
                debug!(session_id = self.session.id, "a presence bitmap answers a request twice");
                return None;
            }
            request.bitmap_applied = true;
            request.last_answer_at = Instant::now();
            if bits.len() != request.blocks.len().div_ceil(8) {
                return Some(Closing::Violation("presence bitmap that does not match the request"));
            }
            // Only the hashes this Session still wants decide the answer. The bits of the hashes it retracted
            // meanwhile are ignored, because the answer is about blocks nobody asks for anymore.
            let presence = PresenceBitmap::decode(bits, request.blocks.len());
            let mut absent = Vec::new();
            let mut present = Vec::new();
            for ((hash, rank), present_bit) in request.blocks.iter().zip(presence) {
                if !request.remaining.contains(hash) {
                    continue;
                }
                if present_bit {
                    present.push(hash.clone());
                } else {
                    absent.push((hash.clone(), *rank));
                }
            }
            (absent, present)
        };

        state.last_progress = Instant::now();
        state.pending[0].claimed = present.into_iter().collect();
        for (hash, _) in &absent {
            state.pending[0].remaining.remove(hash);
        }
        if !absent.is_empty() {
            if state.pending[0].claimed.is_empty() {
                // The peer holds none of the blocks this Session still wants, so it is not asked for those ranks
                // for a while.
                let ranks: Vec<u32> = absent.iter().map(|(_, rank)| *rank).collect();
                let block_hashes: Vec<OmniHash> = absent.iter().map(|(hash, _)| hash.clone()).collect();
                self.ctx
                    .mark_rank_absent(&self.session.root_hash, self.session.id, &self.session.node_id, ranks, block_hashes);
            } else {
                self.ctx.release_blocks(&self.session.root_hash, self.session.id, absent.into_iter().map(|(hash, _)| hash));
            }
        }
        state.pending.retain(|request| !request.is_done());
        None
    }

    /// Sends requests until the requests awaiting responses reach the limit or nothing is left to assign.
    /// The limit shrinks for a node whose blocks arrive slowly.
    async fn fill_requests(&self, state: &mut WorkerState, control_tx: &mpsc::Sender<ExchangeMessage>, want: bool) {
        let limit = self.ctx.pending_limit(&self.session.node_id);
        let mut active = state.pending.iter().filter(|request| request.is_active()).count();
        // The shells left by retracted requests still answer their bitmaps, so the queue of sent requests stays
        // bounded on its own rather than through the limit of the active ones.
        while want && !state.peer_choking && active < limit && state.pending.len() < MAX_PENDING_REQUESTS as usize {
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
            let blocks: Vec<(OmniHash, u32)> = assigned.into_iter().map(|wanted| (wanted.block_hash, wanted.rank)).collect();
            let request = ExchangeMessage::Request {
                block_hashes: blocks.iter().map(|(hash, _)| hash.clone()).collect(),
            };
            if control_tx.try_send(request).is_err() {
                self.ctx.release_blocks(&self.session.root_hash, self.session.id, blocks.into_iter().map(|(hash, _)| hash));
                return;
            }
            if state.pending.is_empty() {
                state.last_progress = Instant::now();
            }
            state.pending.push(PendingRequest::new(blocks, Instant::now()));
            active += 1;
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
                if self.serve_queue.len() >= MAX_PENDING_REQUESTS as usize {
                    return Some(Closing::Violation("too many requests awaiting responses"));
                }
                let present = self.describe(&block_hashes).await;
                let held: Vec<OmniHash> = block_hashes.into_iter().zip(present.iter()).filter_map(|(hash, present)| present.then_some(hash)).collect();
                // The queue keeps the bitmap and the blocks of one request together, so the answers of two
                // requests never interleave and the requesting node reads the bitmap of the oldest request.
                if !self.serve_queue.push(PresenceBitmap::encode(&present), held) {
                    return Some(Closing::Violation("too many requests awaiting responses"));
                }
                None
            }
            ExchangeMessage::HaveMap { bits } => {
                if let Some(closing) = self.on_have_map(state, &bits) {
                    return Some(closing);
                }
                let want = self.ctx.root_hashes().fetches(&self.session.root_hash);
                self.fill_requests(state, control_tx, want).await;
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
            ExchangeMessage::Cancel { block_hashes } => {
                // The peer no longer needs these blocks, which it may have requested before its own retraction.
                self.serve_queue.cancel(&block_hashes);
                None
            }
            ExchangeMessage::Hello { .. } | ExchangeMessage::Open { .. } | ExchangeMessage::Accept | ExchangeMessage::Reject => {
                Some(Closing::Violation("message that belongs to the handshake"))
            }
        }
    }

    async fn on_block(&self, state: &mut WorkerState, block_hash: OmniHash, value: Vec<u8>) -> Option<Closing> {
        let now = Instant::now();
        // A block the Session waits for is accepted even while a cancel of an earlier copy is still remembered.
        let waited = Self::take_pending(state, &block_hash);
        if !waited {
            // A block sent before the peer processed our cancel may arrive afterwards, even more than once when the
            // Session asked for it again meanwhile. It is not blamed, and the content hash decides whether it is
            // stored, because a slow peer may finish a block after the retraction.
            if !state.retracted.contains(&block_hash, now) {
                return Some(Closing::Violation("block that was not requested"));
            }
        }
        state.last_progress = now;
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
                self.ctx.release_blocks(&self.session.root_hash, self.session.id, [block_hash]);
                None
            }
        }
    }

    /// Which of the requested hashes this node provides, which the presence bitmap answers without reading them.
    async fn describe(&self, block_hashes: &[OmniHash]) -> Vec<bool> {
        match self.ctx.contains_blocks(&self.session.root_hash, block_hashes).await {
            Ok(present) => present,
            Err(error) => {
                warn!(error_message = error.to_string(), "checking a block to provide failed");
                vec![false; block_hashes.len()]
            }
        }
    }

    /// Removes the hash from the request awaiting its response. Returns false when no request is awaiting it.
    fn take_pending(state: &mut WorkerState, block_hash: &OmniHash) -> bool {
        let now = Instant::now();
        for index in 0..state.pending.len() {
            if state.pending[index].take(block_hash, now).is_some() {
                if state.pending[index].is_done() {
                    state.pending.remove(index);
                }
                return true;
            }
        }
        false
    }

    fn release_pending(&self, state: &mut WorkerState) {
        let released: Vec<OmniHash> = state.pending.drain(..).flat_map(|request| request.blocks.into_iter().map(|(hash, _)| hash)).collect();
        self.ctx.release_blocks(&self.session.root_hash, self.session.id, released);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};
    use tokio::time::Instant;

    use super::{PendingRequest, PresenceBitmap};

    fn hash(value: u8) -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [value])
    }

    #[test]
    fn a_request_stays_active_until_every_block_is_answered() {
        let start = Instant::now();
        let mut request = PendingRequest::new(vec![(hash(1), 0), (hash(2), 0)], start);
        assert!(request.is_active());
        assert!(!request.is_done());

        assert_eq!(request.take(&hash(1), start + Duration::from_secs(1)), Some(0));
        // The answer decides when the request counts as unanswered again.
        assert_eq!(request.last_answer_at, start + Duration::from_secs(1));
        assert!(request.is_active());

        assert_eq!(request.take(&hash(2), start + Duration::from_secs(2)), Some(0));
        assert!(!request.is_active());
        // The request is kept until its presence bitmap arrives, so the answers stay matched with the requests.
        assert!(!request.is_done());
        request.bitmap_applied = true;
        assert!(request.is_done());
    }

    #[test]
    fn the_bitmap_of_64_hashes_is_8_bytes_and_round_trips() {
        let mut present = vec![false; 64];
        present[0] = true;
        present[7] = true;
        present[63] = true;

        let bits = PresenceBitmap::encode(&present);
        assert_eq!(bits.len(), 8);
        assert_eq!(bits[0], 0b1000_0001);
        assert_eq!(bits[7], 0b1000_0000);
        assert_eq!(PresenceBitmap::decode(&bits, 64), present);
    }

    #[test]
    fn a_bitmap_shorter_than_the_request_reads_the_missing_bits_as_absent() {
        let bits = PresenceBitmap::encode(&[true, false, true]);

        assert_eq!(bits, vec![0b0000_0101]);
        assert_eq!(PresenceBitmap::decode(&bits, 5), vec![true, false, true, false, false]);
    }
}
