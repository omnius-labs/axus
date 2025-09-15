use std::sync::Arc;

use chrono::{Duration, Utc};

use omnius_core_base::clock::Clock;
use omnius_core_omnikit::model::OmniHash;

use crate::{base::collections::VolatileHashSet, core::session::model::Session, prelude::*};

#[allow(unused)]
#[derive(Clone)]
pub struct SessionStatus {
    pub exchange_type: ExchangeType,
    pub session: Session,
    pub root_hash: Arc<Mutex<Option<OmniHash>>>,
    pub sent_want_block_hashes: Arc<Mutex<VolatileHashSet<Arc<OmniHash>>>>,
    pub sent_block_hashes: Arc<Mutex<VolatileHashSet<Arc<OmniHash>>>>,
    pub received_want_block_hashes: Arc<Mutex<VolatileHashSet<Arc<OmniHash>>>>,
}

#[allow(unused)]
impl SessionStatus {
    pub fn new(exchange_type: ExchangeType, session: Session, root_hash: Option<OmniHash>, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Self {
        Self {
            exchange_type,
            session,
            root_hash: Arc::new(Mutex::new(root_hash)),
            sent_want_block_hashes: Arc::new(Mutex::new(VolatileHashSet::new(Duration::minutes(30), clock.clone()))),
            sent_block_hashes: Arc::new(Mutex::new(VolatileHashSet::new(Duration::minutes(30), clock.clone()))),
            received_want_block_hashes: Arc::new(Mutex::new(VolatileHashSet::new(Duration::minutes(30), clock.clone()))),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeType {
    Unknown,
    Publish,
    Subscribe,
}
