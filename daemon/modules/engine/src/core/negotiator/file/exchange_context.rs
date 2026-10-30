use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use parking_lot::Mutex;
use tokio::{sync::Semaphore, task::JoinHandle, time::Instant};
use tokio_util::sync::CancellationToken;

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{generated::axus::file::MAX_PENDING_REQUESTS, model::AssetKey, prelude::*};

use super::*;

/// What the root hashes of this node are right now: which ones it provides and which ones it fetches.
/// The upkeep task refreshes it from the Stores, so that the Session workers need no query to decide.
#[derive(Default)]
pub struct ProvidedRootHashes {
    provided: HashSet<OmniHash>,
    /// The root hashes of Downloading subscriptions, in the order of their priority.
    fetching: Vec<OmniHash>,
}

impl ProvidedRootHashes {
    pub fn new(provided: HashSet<OmniHash>, fetching: Vec<OmniHash>) -> Self {
        Self { provided, fetching }
    }

    pub fn provides(&self, root_hash: &OmniHash) -> bool {
        self.provided.contains(root_hash)
    }

    pub fn fetches(&self, root_hash: &OmniHash) -> bool {
        self.fetching.contains(root_hash)
    }

    pub fn fetching(&self) -> &[OmniHash] {
        &self.fetching
    }

    /// The provided root hashes that are not fetched.
    pub fn serving(&self) -> Vec<OmniHash> {
        let mut serving: Vec<OmniHash> = self.provided.iter().filter(|root_hash| !self.fetching.contains(root_hash)).cloned().collect();
        serving.sort_by_key(|root_hash| root_hash.to_string());
        serving
    }
}

/// The state shared by the tasks and the Session workers of one FileExchanger.
pub struct ExchangeContext {
    pub my_node_id: Vec<u8>,
    pub file_publisher: Arc<FilePublisher>,
    pub file_subscriber: Arc<FileSubscriber>,
    pub location_finder: Arc<dyn LocationFinder>,
    pub registry: ExchangeSessionRegistry,
    pub option: FileExchangerOption,
    pub intervals: FileExchangerIntervals,
    pub cancellation_token: CancellationToken,
    assigner: Mutex<BlockAssigner>,
    wanted_refreshed_at: Mutex<HashMap<OmniHash, Instant>>,
    choke: Mutex<ChokeManager>,
    fetch_score: Mutex<FetchScore>,
    excluded_nodes: Mutex<Expirations<Vec<u8>>>,
    closed_pairs: Mutex<Expirations<(Vec<u8>, OmniHash)>>,
    root_hashes: Mutex<ProvidedRootHashes>,
    rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
    next_session_id: AtomicU64,
    dialing: Mutex<HashSet<(Vec<u8>, OmniHash)>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    /// The connection slots. A Session holds a permit from before its handshake until its worker ends.
    pub fetch_slots: Arc<Semaphore>,
    pub serve_slots: Arc<Semaphore>,
    pub accept_slots: Arc<Semaphore>,
}

impl ExchangeContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        file_publisher: Arc<FilePublisher>,
        file_subscriber: Arc<FileSubscriber>,
        location_finder: Arc<dyn LocationFinder>,
        rng: Arc<Mutex<dyn rand::Rng + Send + Sync>>,
        option: FileExchangerOption,
        intervals: FileExchangerIntervals,
    ) -> Self {
        Self {
            my_node_id: location_finder.my_node_id(),
            file_publisher,
            file_subscriber,
            location_finder,
            registry: ExchangeSessionRegistry::new(),
            assigner: Mutex::new(BlockAssigner::new(
                intervals.rank_absent_retry,
                TailOption {
                    remaining: intervals.tail_remaining,
                    stall_remaining: intervals.tail_stall_remaining,
                    stall: intervals.tail_stall,
                    copies: intervals.tail_copies,
                },
            )),
            wanted_refreshed_at: Mutex::new(HashMap::new()),
            choke: Mutex::new(ChokeManager::new(intervals.ranking_window, option.unchoke_count, intervals.unchoke_rotation)),
            fetch_score: Mutex::new(FetchScore::new(intervals.fetch_window, intervals.fetch_min_blocks)),
            excluded_nodes: Mutex::new(Expirations::new()),
            closed_pairs: Mutex::new(Expirations::new()),
            root_hashes: Mutex::new(ProvidedRootHashes::default()),
            rng,
            next_session_id: AtomicU64::new(1),
            dialing: Mutex::new(HashSet::new()),
            workers: Mutex::new(Vec::new()),
            fetch_slots: Arc::new(Semaphore::new(option.max_connected_session_for_fetch_count)),
            serve_slots: Arc::new(Semaphore::new(option.max_connected_session_for_serve_count)),
            accept_slots: Arc::new(Semaphore::new(option.max_accepted_session_count)),
            option,
            intervals,
            cancellation_token: CancellationToken::new(),
        }
    }

    pub fn next_session_id(&self) -> u64 {
        self.next_session_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Marks the pair as being dialed, so that two connection tasks do not dial the same pair. Returns false when it already is.
    pub fn begin_dial(&self, node_id: &[u8], root_hash: &OmniHash) -> bool {
        self.dialing.lock().insert((node_id.to_vec(), root_hash.clone()))
    }

    pub fn end_dial(&self, node_id: &[u8], root_hash: &OmniHash) {
        self.dialing.lock().remove(&(node_id.to_vec(), root_hash.clone()));
    }

    /// Whether a Session with the node for the root hash may be accepted or dialed.
    pub fn can_exchange(&self, node_id: &[u8], root_hash: &OmniHash) -> bool {
        node_id != self.my_node_id.as_slice()
            && !self.is_excluded(node_id)
            && self.root_hashes().provides(root_hash)
            && self.registry.count_by_root_hash(root_hash) < self.option.max_session_per_root_hash_count
    }

    /// Registers the Session and starts its worker, or drops the Session when an existing one is kept.
    /// The permit is held until the worker ends.
    pub fn start_session(self: &Arc<Self>, session: Arc<ExchangeSession>, permit: tokio::sync::OwnedSemaphorePermit) {
        match self.registry.register(&self.my_node_id, &session) {
            Registration::Added => {}
            Registration::Replaced => debug!(session_id = session.id, "file exchange session replaced"),
            Registration::Rejected => {
                debug!(session_id = session.id, "file exchange session dropped because the same pair already has one");
                return;
            }
        }
        let worker = SessionWorker::new(self.clone(), session);
        let mut workers = self.workers.lock();
        workers.retain(|handle| !handle.is_finished());
        workers.push(tokio::spawn(async move {
            worker.run().await;
            drop(permit);
        }));
    }

    /// Waits for the workers, which stop when the cancellation token is cancelled.
    pub async fn join_workers(&self) {
        let workers = std::mem::take(&mut *self.workers.lock());
        for worker in workers {
            let _ = worker.await;
        }
    }

    pub fn exclude_for_violation(&self, node_id: &[u8]) {
        self.excluded_nodes.lock().insert(node_id.to_vec(), Instant::now() + self.intervals.violation_exclusion);
    }

    pub fn exclude_for_timeout(&self, node_id: &[u8]) {
        self.excluded_nodes.lock().insert(node_id.to_vec(), Instant::now() + self.intervals.timeout_exclusion);
    }

    pub fn is_excluded(&self, node_id: &[u8]) -> bool {
        self.excluded_nodes.lock().contains(&node_id.to_vec(), Instant::now())
    }

    /// Keeps the pair out of the dialing candidates for `duration`. Incoming dials are still accepted.
    pub fn set_aside_pair(&self, node_id: &[u8], root_hash: &OmniHash, duration: std::time::Duration) {
        self.closed_pairs.lock().insert((node_id.to_vec(), root_hash.clone()), Instant::now() + duration);
    }

    pub fn is_pair_set_aside(&self, node_id: &[u8], root_hash: &OmniHash) -> bool {
        self.closed_pairs.lock().contains(&(node_id.to_vec(), root_hash.clone()), Instant::now())
    }

    pub fn root_hashes(&self) -> parking_lot::MutexGuard<'_, ProvidedRootHashes> {
        self.root_hashes.lock()
    }

    /// Refreshes which root hashes are provided and fetched, drops the assignments of the root hashes no longer fetched,
    /// and advertises the provided ones.
    pub async fn refresh_root_hashes(&self) -> Result<()> {
        let mut provided: HashSet<OmniHash> = self.file_publisher.get_published_root_hashes().await?.into_iter().collect();
        provided.extend(self.file_subscriber.get_provided_root_hashes().await?);
        let fetching = self.file_subscriber.get_subscribed_root_hashes().await?;

        let fetching_set: HashSet<OmniHash> = fetching.iter().cloned().collect();
        self.assigner.lock().retain_roots(&fetching_set);
        self.wanted_refreshed_at.lock().retain(|root_hash, _| fetching_set.contains(root_hash));

        let mut asset_keys: Vec<AssetKey> = provided.iter().map(|hash| AssetKey::file(hash.clone())).collect();
        asset_keys.sort();
        self.location_finder.set_asset_keys(asset_keys);

        *self.root_hashes.lock() = ProvidedRootHashes::new(provided, fetching);
        Ok(())
    }

    /// Reads a block this node provides, from the publisher or from a subscription.
    pub async fn read_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = self.file_publisher.read_block(root_hash, block_hash).await? {
            return Ok(Some(bytes.to_vec()));
        }
        self.file_subscriber.read_provided_block(root_hash, block_hash).await
    }

    /// Whether this node provides the block, which answers a request without reading the block body.
    pub async fn contains_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<bool> {
        if self.file_publisher.contains_block(root_hash, block_hash).await? {
            return Ok(true);
        }
        self.file_subscriber.contains_provided_block(root_hash, block_hash).await
    }

    /// Assigns blocks of the root hash to the Session. The blocks to fetch are read again from the Store
    /// when nothing is assignable and the previous reading is old.
    pub async fn assign_blocks(&self, root_hash: &OmniHash, session_id: u64, node_id: &[u8], limit: usize) -> Result<Vec<WantedBlock>> {
        let assigned = self.assigner.lock().assign(root_hash, session_id, node_id, limit, Instant::now());
        if !assigned.is_empty() {
            return Ok(assigned);
        }

        let now = Instant::now();
        let due = self
            .wanted_refreshed_at
            .lock()
            .get(root_hash)
            .is_none_or(|refreshed_at| now.saturating_duration_since(*refreshed_at) >= self.intervals.wanted_refresh);
        if !due {
            return Ok(assigned);
        }
        let wanted: Vec<WantedBlock> = self
            .file_subscriber
            .get_missing_blocks(root_hash)
            .await?
            .into_iter()
            .map(|(block_hash, rank)| WantedBlock { block_hash, rank })
            .collect();
        self.wanted_refreshed_at.lock().insert(root_hash.clone(), now);
        let mut assigner = self.assigner.lock();
        assigner.update_wanted(root_hash, wanted, now);
        Ok(assigner.assign(root_hash, session_id, node_id, limit, now))
    }

    pub fn complete_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) {
        self.assigner.lock().complete(root_hash, block_hash, Instant::now());
    }

    /// Whether the Session is still expected to answer with the block. It stops being expected when another
    /// Session saved the same block in the tail, so the Session retracts it instead of keeping it pending.
    pub fn is_assigned(&self, root_hash: &OmniHash, session_id: u64, block_hash: &OmniHash) -> bool {
        self.assigner.lock().is_assigned(root_hash, session_id, block_hash)
    }

    pub fn release_blocks(&self, root_hash: &OmniHash, session_id: u64, block_hashes: impl IntoIterator<Item = OmniHash>) {
        self.assigner.lock().release(root_hash, session_id, block_hashes);
    }

    pub fn release_session_blocks(&self, root_hash: &OmniHash, session_id: u64) {
        self.assigner.lock().release_session(root_hash, session_id);
    }

    /// Releases the blocks the node does not hold and stops asking it for their ranks for a while.
    pub fn mark_rank_absent(&self, root_hash: &OmniHash, session_id: u64, node_id: &[u8], ranks: impl IntoIterator<Item = u32>, block_hashes: impl IntoIterator<Item = OmniHash>) {
        let now = Instant::now();
        let mut assigner = self.assigner.lock();
        assigner.release(root_hash, session_id, block_hashes);
        assigner.mark_rank_absent(root_hash, node_id, ranks, now);
    }

    pub fn is_choked(&self, node_id: &[u8]) -> bool {
        self.choke.lock().is_choked(node_id)
    }

    pub fn record_received(&self, node_id: &[u8], bytes: u64) {
        let now = Instant::now();
        self.choke.lock().record_received(node_id, bytes, now);
        self.fetch_score.lock().record(node_id, bytes, now);
    }

    /// The bytes per second the node provided in the recent period, or None while it provided too few blocks.
    pub fn fetch_throughput(&self, node_id: &[u8]) -> Option<f64> {
        self.fetch_score.lock().throughput(node_id, Instant::now())
    }

    /// The throughput a node without a record is given, so that it is neither preferred nor avoided.
    pub fn fetch_median(&self) -> Option<f64> {
        self.fetch_score.lock().median(Instant::now())
    }

    /// How many requests the Session may keep awaiting responses. A slow node keeps fewer, which bounds its share
    /// without excluding it, and a node with no record keeps the maximum.
    pub fn pending_limit(&self, node_id: &[u8]) -> usize {
        let now = Instant::now();
        let score = self.fetch_score.lock();
        let Some(throughput) = score.throughput(node_id, now) else {
            return MAX_PENDING_REQUESTS as usize;
        };
        let median = score.median(now).unwrap_or(throughput);
        if median <= 0.0 {
            return MAX_PENDING_REQUESTS as usize;
        }
        let slowest = self.intervals.pending_min.min(MAX_PENDING_REQUESTS as usize);
        let ratio = (throughput / median).clamp(0.0, 1.0);
        slowest + ((MAX_PENDING_REQUESTS as usize - slowest) as f64 * ratio) as usize
    }

    pub fn record_sent(&self, node_id: &[u8], bytes: u64) {
        self.choke.lock().record_sent(node_id, bytes);
    }

    /// Chooses the nodes to unchoke among the nodes that want our blocks.
    pub fn decide_choke(&self) {
        let wanting = self.registry.wanting_node_ids();
        let mut rng = self.rng.lock();
        self.choke.lock().decide(&wanting, Instant::now(), &mut *rng);
    }
}
