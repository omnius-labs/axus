use std::time::Duration;

use omnius_core_omnikit::generated::omni_hash::OmniHash;

use crate::{
    base::connection::{FramedRecvExt as _, FramedSendExt as _},
    core::session::model::Session,
    generated::axus::file::ExchangeMessage,
    prelude::*,
    protocol::file::ExchangeMessageCodec,
};

/// The exchange right after the use selection: the versions, then the root hash the dialing side wants to exchange blocks of.
pub struct ExchangeHandshake;

impl ExchangeHandshake {
    const VERSION_V1: u32 = 1;

    /// The dialing side conveys the root hash and waits for the peer to accept it.
    pub async fn dial(session: &Session, root_hash: &OmniHash, receive_timeout: Duration) -> Result<()> {
        session.stream.set_receive_timeout(receive_timeout);

        Self::send(session, &ExchangeMessage::Hello { version: Self::VERSION_V1 }).await?;
        Self::receive_hello(session).await?;
        Self::send(session, &ExchangeMessage::Open { root_hash: root_hash.clone() }).await?;
        match Self::receive(session).await? {
            ExchangeMessage::Accept => Ok(()),
            ExchangeMessage::Reject => Err(Error::new(ErrorKind::Reject).with_message("the peer rejected the root hash")),
            _ => Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected message instead of the answer to the root hash")),
        }
    }

    /// The accepting side learns the root hash and answers with `can_serve`'s judgment.
    /// Returns the root hash only when it accepted.
    pub async fn accept(session: &Session, receive_timeout: Duration, can_serve: impl FnOnce(&OmniHash) -> bool) -> Result<OmniHash> {
        session.stream.set_receive_timeout(receive_timeout);

        Self::receive_hello(session).await?;
        Self::send(session, &ExchangeMessage::Hello { version: Self::VERSION_V1 }).await?;
        let ExchangeMessage::Open { root_hash } = Self::receive(session).await? else {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected message instead of the root hash"));
        };
        if !can_serve(&root_hash) {
            Self::send(session, &ExchangeMessage::Reject).await?;
            return Err(Error::new(ErrorKind::Reject).with_message("the root hash is not served"));
        }
        Self::send(session, &ExchangeMessage::Accept).await?;
        Ok(root_hash)
    }

    async fn receive_hello(session: &Session) -> Result<()> {
        let ExchangeMessage::Hello { version } = Self::receive(session).await? else {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected message instead of the version"));
        };
        if version & Self::VERSION_V1 == 0 {
            return Err(Error::new(ErrorKind::UnsupportedType).with_message(format!("no common version: {version}")));
        }
        Ok(())
    }

    async fn send(session: &Session, message: &ExchangeMessage) -> Result<()> {
        session.stream.sender.lock().await.send_message_with::<ExchangeMessageCodec>(message).await
    }

    async fn receive(session: &Session) -> Result<ExchangeMessage> {
        session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use testresult::TestResult;

    use omnius_core_omnikit::{
        generated::{
            omni_hash::{OmniHash, OmniHashAlgorithmType},
            omni_sign::{OmniSignType, OmniSigner},
        },
        model::omni_addr::OmniAddr,
    };

    use crate::{
        base::connection::{FramedRecvExt as _, FramedSendExt as _, FramedStream},
        core::session::model::{Session, SessionHandshakeType, SessionType},
        generated::axus::file::ExchangeMessage,
        prelude::*,
        protocol::file::ExchangeMessageCodec,
    };

    use super::ExchangeHandshake;

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn session_pair() -> TestResult<(Session, Session)> {
        let (a, b) = tokio::io::duplex(64 * 1024);
        let (a_reader, a_writer) = tokio::io::split(a);
        let (b_reader, b_writer) = tokio::io::split(b);
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?;
        let dialing = Session {
            typ: SessionType::FileExchanger,
            address: OmniAddr::new("a"),
            handshake_type: SessionHandshakeType::Connected,
            cert: signer.sign(b"test")?,
            stream: FramedStream::new(a_reader, a_writer),
        };
        let accepting = Session {
            typ: SessionType::FileExchanger,
            address: OmniAddr::new("b"),
            handshake_type: SessionHandshakeType::Accepted,
            cert: signer.sign(b"test")?,
            stream: FramedStream::new(b_reader, b_writer),
        };
        Ok((dialing, accepting))
    }

    fn root() -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, b"root")
    }

    #[tokio::test]
    async fn the_accepting_side_learns_the_root_hash_of_the_dialing_side() -> TestResult {
        let (dialing, accepting) = session_pair()?;
        let root = root();

        let (dial, accept) = tokio::join!(
            ExchangeHandshake::dial(&dialing, &root, TIMEOUT),
            ExchangeHandshake::accept(&accepting, TIMEOUT, |root_hash| *root_hash == root)
        );

        dial?;
        assert_eq!(accept?, root);
        Ok(())
    }

    #[tokio::test]
    async fn a_root_hash_the_accepting_side_does_not_serve_is_rejected_on_both_sides() -> TestResult {
        let (dialing, accepting) = session_pair()?;
        let root = root();

        let (dial, accept) = tokio::join!(ExchangeHandshake::dial(&dialing, &root, TIMEOUT), ExchangeHandshake::accept(&accepting, TIMEOUT, |_| false));

        assert_eq!(dial.unwrap_err().kind(), &ErrorKind::Reject);
        assert_eq!(accept.unwrap_err().kind(), &ErrorKind::Reject);
        Ok(())
    }

    #[tokio::test]
    async fn a_peer_without_a_common_version_fails_the_handshake() -> TestResult {
        let (dialing, accepting) = session_pair()?;

        let peer = async {
            accepting
                .stream
                .sender
                .lock()
                .await
                .send_message_with::<ExchangeMessageCodec>(&ExchangeMessage::Hello { version: 2 })
                .await?;
            Result::Ok(())
        };
        let root = root();
        let (dial, peer) = tokio::join!(ExchangeHandshake::dial(&dialing, &root, TIMEOUT), peer);

        peer?;
        assert_eq!(dial.unwrap_err().kind(), &ErrorKind::UnsupportedType);
        Ok(())
    }

    #[tokio::test]
    async fn a_silent_peer_fails_at_the_receive_timeout() -> TestResult {
        let (dialing, accepting) = session_pair()?;

        let root = root();
        let result = ExchangeHandshake::dial(&dialing, &root, Duration::from_millis(100)).await;

        assert!(result.is_err());
        // The dialing side sent its version before waiting.
        let ExchangeMessage::Hello { .. } = accepting.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await? else {
            panic!("the first message is the version");
        };
        Ok(())
    }
}
