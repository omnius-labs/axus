use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock as TokioRwLock;

use crate::core::session::model::SessionHandshakeType;

use super::SessionStatus;

/// 登録の結果。
#[derive(Debug, PartialEq, Eq)]
pub enum Registration {
    /// 同じ相手との Session がなく、そのまま登録した。
    Added,
    /// 既存の Session を止めて、新しい Session に入れ替えた。
    Replaced,
    /// 既存の Session を残し、新しい Session は登録しなかった。
    Rejected,
}

/// 確立済みの Session を、相手の node ID ごとに高々 1 つだけ保持する。
///
/// 互いに同時に接続すると、同じ相手との Session が 2 本できる。
/// このとき、node ID が小さい側から張った Session を残す規則にすると、
/// 両側が追加の message なしに同じ Session を選ぶ。
#[derive(Default)]
pub struct SessionRegistry {
    sessions: TokioRwLock<HashMap<Vec<u8>, Arc<SessionStatus>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(&self, my_id: &[u8], other_id: &[u8], status: &Arc<SessionStatus>) -> Registration {
        let mut sessions = self.sessions.write().await;

        let Some(existing) = sessions.get(other_id) else {
            sessions.insert(other_id.to_vec(), status.clone());
            return Registration::Added;
        };

        let preferred = Self::preferred_handshake_type(my_id, other_id);
        if !Self::replaces(&preferred, &existing.session.handshake_type, &status.session.handshake_type) {
            return Registration::Rejected;
        }

        existing.cancellation_token.cancel();
        sessions.insert(other_id.to_vec(), status.clone());
        Registration::Replaced
    }

    /// 自分が登録した Session のときだけ消す。
    /// 入れ替えで後から登録された Session は、元の Session の終了では消さない。
    pub async fn unregister(&self, other_id: &[u8], status: &Arc<SessionStatus>) {
        let mut sessions = self.sessions.write().await;
        if sessions.get(other_id).is_some_and(|current| Arc::ptr_eq(current, status)) {
            sessions.remove(other_id);
        }
    }

    pub async fn len(&self) -> usize {
        self.sessions.read().await.len()
    }

    pub async fn count_by_handshake_type(&self, handshake_type: &SessionHandshakeType) -> usize {
        self.sessions
            .read()
            .await
            .values()
            .filter(|status| status.session.handshake_type == *handshake_type)
            .count()
    }

    pub async fn ids(&self) -> Vec<Vec<u8>> {
        self.sessions.read().await.keys().cloned().collect()
    }

    pub async fn statuses(&self) -> Vec<(Vec<u8>, Arc<SessionStatus>)> {
        self.sessions.read().await.iter().map(|(id, status)| (id.clone(), status.clone())).collect()
    }

    /// 重複したときに残す Session の向き。node ID が小さい側では Connected、大きい側では Accepted になる。
    fn preferred_handshake_type(my_id: &[u8], other_id: &[u8]) -> SessionHandshakeType {
        if my_id < other_id {
            SessionHandshakeType::Connected
        } else {
            SessionHandshakeType::Accepted
        }
    }

    /// 新しい Session が既存の Session を入れ替えるかどうか。
    /// 優先する向きの Session が新しい側だけのときに入れ替え、それ以外は既存を残す。
    fn replaces(preferred: &SessionHandshakeType, existing: &SessionHandshakeType, new: &SessionHandshakeType) -> bool {
        existing != preferred && new == preferred
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use testresult::TestResult;

    use omnius_core_base::clock::{Clock, ClockUtc};
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::{
        base::connection::FramedStream,
        core::session::model::{Session, SessionType},
    };

    use super::*;

    const SMALL: &[u8] = &[1];
    const LARGE: &[u8] = &[2];

    #[test]
    fn the_session_dialed_by_the_smaller_id_is_preferred() {
        assert_eq!(SessionRegistry::preferred_handshake_type(SMALL, LARGE), SessionHandshakeType::Connected);
        assert_eq!(SessionRegistry::preferred_handshake_type(LARGE, SMALL), SessionHandshakeType::Accepted);
    }

    #[test]
    fn only_the_preferred_session_replaces_a_session_of_the_other_direction() {
        use SessionHandshakeType::{Accepted, Connected};

        assert!(SessionRegistry::replaces(&Connected, &Accepted, &Connected));
        assert!(!SessionRegistry::replaces(&Connected, &Connected, &Accepted));
        assert!(!SessionRegistry::replaces(&Accepted, &Accepted, &Connected));
        assert!(SessionRegistry::replaces(&Accepted, &Connected, &Accepted));
    }

    #[test]
    fn the_existing_session_stays_when_both_have_the_same_direction() {
        use SessionHandshakeType::{Accepted, Connected};

        for preferred in [Connected, Accepted] {
            assert!(!SessionRegistry::replaces(&preferred, &Connected, &Connected));
            assert!(!SessionRegistry::replaces(&preferred, &Accepted, &Accepted));
        }
    }

    #[tokio::test]
    async fn register_replaces_a_session_when_the_preferred_one_arrives_later() -> TestResult {
        // SMALL 側から見ると、優先する Session は Connected である
        let registry = SessionRegistry::new();
        let existing = session_status(SessionHandshakeType::Accepted)?;
        let preferred = session_status(SessionHandshakeType::Connected)?;

        assert_eq!(registry.register(SMALL, LARGE, &existing).await, Registration::Added);
        assert_eq!(registry.register(SMALL, LARGE, &preferred).await, Registration::Replaced);

        assert!(existing.cancellation_token.is_cancelled());
        assert!(!preferred.cancellation_token.is_cancelled());
        assert_eq!(registry.len().await, 1);

        Ok(())
    }

    #[tokio::test]
    async fn register_rejects_a_session_when_the_preferred_one_is_already_registered() -> TestResult {
        // SMALL 側から見ると、優先する Session は Connected である
        let registry = SessionRegistry::new();
        let preferred = session_status(SessionHandshakeType::Connected)?;
        let late = session_status(SessionHandshakeType::Accepted)?;

        assert_eq!(registry.register(SMALL, LARGE, &preferred).await, Registration::Added);
        assert_eq!(registry.register(SMALL, LARGE, &late).await, Registration::Rejected);

        assert!(!preferred.cancellation_token.is_cancelled());
        assert_eq!(registry.len().await, 1);

        Ok(())
    }

    #[tokio::test]
    async fn unregister_does_not_remove_a_session_that_replaced_it() -> TestResult {
        let registry = SessionRegistry::new();
        let replaced = session_status(SessionHandshakeType::Accepted)?;
        let replacing = session_status(SessionHandshakeType::Connected)?;

        registry.register(SMALL, LARGE, &replaced).await;
        registry.register(SMALL, LARGE, &replacing).await;

        registry.unregister(LARGE, &replaced).await;
        assert_eq!(registry.len().await, 1);

        registry.unregister(LARGE, &replacing).await;
        assert_eq!(registry.len().await, 0);

        Ok(())
    }

    fn session_status(handshake_type: SessionHandshakeType) -> Result<Arc<SessionStatus>, Box<dyn std::error::Error>> {
        let (local, _peer) = tokio::io::duplex(1024);
        let (reader, writer) = tokio::io::split(local);

        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?;
        let session = Session {
            typ: SessionType::NodeFinder,
            address: OmniAddr::create_tcp("127.0.0.1".parse()?, 1),
            handshake_type,
            cert: signer.sign(b"test")?,
            stream: FramedStream::new(reader, writer),
        };

        let clock: Arc<dyn Clock<Utc> + Send + Sync> = Arc::new(ClockUtc);
        Ok(Arc::new(SessionStatus::new(session, clock)))
    }
}
