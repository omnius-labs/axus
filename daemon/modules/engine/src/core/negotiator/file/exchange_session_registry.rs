use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::core::session::model::SessionHandshakeType;

use super::ExchangeSession;

/// The result of registering a Session.
#[derive(Debug, PartialEq, Eq)]
pub enum Registration {
    Added,
    /// The existing Session was stopped and replaced.
    Replaced,
    /// The existing Session was kept and the new one was not registered.
    Rejected,
}

/// The node ID and the root hash a Session is for.
type SessionKey = (Vec<u8>, OmniHash);

/// Holds the established Sessions, at most one per pair of node ID and root hash.
#[derive(Default)]
pub struct ExchangeSessionRegistry {
    sessions: Mutex<HashMap<SessionKey, Arc<ExchangeSession>>>,
}

impl ExchangeSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// When both sides dialed the same pair, keeps the Session the smaller node ID dialed, as NodeFinder does.
    pub fn register(&self, my_id: &[u8], session: &Arc<ExchangeSession>) -> Registration {
        let mut sessions = self.sessions.lock();
        let key = (session.node_id.clone(), session.root_hash.clone());
        let Some(existing) = sessions.get(&key) else {
            sessions.insert(key, session.clone());
            return Registration::Added;
        };

        let preferred = SessionHandshakeType::preferred_between(my_id, &session.node_id);
        if !SessionHandshakeType::replaces(&preferred, existing.handshake_type(), session.handshake_type()) {
            return Registration::Rejected;
        }

        existing.cancellation_token.cancel();
        sessions.insert(key, session.clone());
        Registration::Replaced
    }

    /// Removes the Session only when it is the registered one, because a replacing Session may already hold the key.
    pub fn unregister(&self, session: &Arc<ExchangeSession>) {
        let key = (session.node_id.clone(), session.root_hash.clone());
        let mut sessions = self.sessions.lock();
        if sessions.get(&key).is_some_and(|current| Arc::ptr_eq(current, session)) {
            sessions.remove(&key);
        }
    }

    pub fn contains(&self, node_id: &[u8], root_hash: &OmniHash) -> bool {
        self.sessions.lock().contains_key(&(node_id.to_vec(), root_hash.clone()))
    }

    pub fn count_by_root_hash(&self, root_hash: &OmniHash) -> usize {
        self.sessions.lock().values().filter(|session| session.root_hash == *root_hash).count()
    }

    /// The node IDs that told us they want our blocks.
    pub fn wanting_node_ids(&self) -> Vec<Vec<u8>> {
        let mut node_ids: Vec<Vec<u8>> = self
            .sessions
            .lock()
            .values()
            .filter(|session| session.peer_wants())
            .map(|session| session.node_id.clone())
            .collect();
        node_ids.sort();
        node_ids.dedup();
        node_ids
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use omnius_core_omnikit::{
        generated::{
            omni_hash::{OmniHash, OmniHashAlgorithmType},
            omni_sign::{OmniSignType, OmniSigner},
        },
        model::omni_addr::OmniAddr,
    };

    use crate::{
        base::connection::FramedStream,
        core::session::model::{Session, SessionHandshakeType, SessionType},
    };

    use super::{ExchangeSession, ExchangeSessionRegistry, Registration};

    const SMALL: &[u8] = &[1];
    const LARGE: &[u8] = &[2];

    fn root(value: u8) -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, [value])
    }

    fn exchange_session(node_id: &[u8], root_hash: OmniHash, handshake_type: SessionHandshakeType) -> Arc<ExchangeSession> {
        let (local, _peer) = tokio::io::duplex(64);
        let (reader, writer) = tokio::io::split(local);
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test").unwrap();
        Arc::new(ExchangeSession::new(
            0,
            node_id.to_vec(),
            root_hash,
            Session {
                typ: SessionType::FileExchanger,
                address: OmniAddr::new("test"),
                handshake_type,
                cert: signer.sign(b"test").unwrap(),
                stream: FramedStream::new(reader, writer, SessionType::FileExchanger.max_frame_length()),
            },
        ))
    }

    #[test]
    fn a_session_is_unique_per_node_and_root_hash() {
        let registry = ExchangeSessionRegistry::new();
        let first = exchange_session(LARGE, root(1), SessionHandshakeType::Connected);

        assert_eq!(registry.register(SMALL, &first), Registration::Added);
        assert_eq!(
            registry.register(SMALL, &exchange_session(LARGE, root(2), SessionHandshakeType::Connected)),
            Registration::Added
        );
        assert_eq!(registry.count_by_root_hash(&root(1)), 1);
        assert_eq!(registry.count_by_root_hash(&root(2)), 1);
        assert!(registry.contains(LARGE, &root(1)));
    }

    #[test]
    fn the_session_dialed_by_the_smaller_node_id_is_kept_whichever_arrives_first() {
        // The local node is the smaller one: its Connected Session is preferred.
        let registry = ExchangeSessionRegistry::new();
        let accepted = exchange_session(LARGE, root(1), SessionHandshakeType::Accepted);
        let connected = exchange_session(LARGE, root(1), SessionHandshakeType::Connected);
        assert_eq!(registry.register(SMALL, &accepted), Registration::Added);
        assert_eq!(registry.register(SMALL, &connected), Registration::Replaced);
        assert!(accepted.cancellation_token.is_cancelled());

        let registry = ExchangeSessionRegistry::new();
        let accepted = exchange_session(LARGE, root(1), SessionHandshakeType::Accepted);
        let connected = exchange_session(LARGE, root(1), SessionHandshakeType::Connected);
        assert_eq!(registry.register(SMALL, &connected), Registration::Added);
        assert_eq!(registry.register(SMALL, &accepted), Registration::Rejected);
        assert!(!connected.cancellation_token.is_cancelled());
    }

    #[test]
    fn unregistering_a_replaced_session_keeps_the_replacing_one() {
        let registry = ExchangeSessionRegistry::new();
        let accepted = exchange_session(LARGE, root(1), SessionHandshakeType::Accepted);
        let connected = exchange_session(LARGE, root(1), SessionHandshakeType::Connected);
        registry.register(SMALL, &accepted);
        registry.register(SMALL, &connected);

        registry.unregister(&accepted);
        assert!(registry.contains(LARGE, &root(1)));
        registry.unregister(&connected);
        assert!(!registry.contains(LARGE, &root(1)));
    }

    #[test]
    fn wanting_nodes_are_listed_once() {
        let registry = ExchangeSessionRegistry::new();
        let a = exchange_session(LARGE, root(1), SessionHandshakeType::Connected);
        let b = exchange_session(LARGE, root(2), SessionHandshakeType::Connected);
        let c = exchange_session(SMALL, root(1), SessionHandshakeType::Connected);
        for session in [&a, &b, &c] {
            registry.register(&[9], session);
        }
        a.set_peer_wants(true);
        b.set_peer_wants(true);

        assert_eq!(registry.wanting_node_ids(), vec![LARGE.to_vec()]);
    }
}
