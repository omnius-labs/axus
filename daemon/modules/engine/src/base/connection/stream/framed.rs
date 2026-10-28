use std::{sync::Arc, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Mutex as TokioMutex,
};

use omnius_core_omnikit::service::connection::codec::{DeadlineReceiver, FramedReceiver, FramedRecv, FramedSend, FramedSender, ReceiveDeadline};

#[derive(Clone)]
pub struct FramedStream {
    pub receiver: Arc<TokioMutex<dyn FramedRecv + Send + Unpin>>,
    pub sender: Arc<TokioMutex<dyn FramedSend + Send + Unpin>>,
    receive_deadline: ReceiveDeadline,
}

impl FramedStream {
    pub const NODE_FINDER_MAX_FRAME_LENGTH: usize = crate::generated::axus::node::MAX_MESSAGE_LENGTH as usize;
    /// The maximum block length plus the hash and the tags a block message carries.
    pub const FILE_EXCHANGER_MAX_FRAME_LENGTH: usize = crate::generated::axus::file::MAX_BLOCK_LENGTH as usize + 256;

    pub fn new<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let receive_deadline = ReceiveDeadline::default();
        let receiver = Arc::new(TokioMutex::new(DeadlineReceiver::new(
            FramedReceiver::new(reader, Self::FILE_EXCHANGER_MAX_FRAME_LENGTH),
            receive_deadline.clone(),
        )));
        let sender = Arc::new(TokioMutex::new(FramedSender::new(writer, Self::FILE_EXCHANGER_MAX_FRAME_LENGTH)));
        Self {
            receiver,
            sender,
            receive_deadline,
        }
    }

    /// Makes a receive fail when no frame arrives within `timeout` of the last received frame, counting from now.
    pub fn set_receive_timeout(&self, timeout: Duration) {
        self.receive_deadline.set(timeout);
    }

    /// Whether the error is a receive that exceeded the deadline of `set_receive_timeout`.
    pub fn is_receive_timeout(error: &(dyn std::error::Error + 'static)) -> bool {
        ReceiveDeadline::is_timeout(error)
    }

    pub async fn set_max_frame_length(&self, max_frame_length: usize) {
        self.receiver.lock().await.set_max_frame_length(max_frame_length);
        self.sender.lock().await.set_max_frame_length(max_frame_length);
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;
    use tokio_util::bytes::Bytes;

    use omnius_core_base::error::OmniError as _;

    use super::FramedStream;

    #[tokio::test]
    async fn changed_limit_applies_to_receive_and_send() -> TestResult {
        let (local, peer) = stream_pair();
        local.set_max_frame_length(4).await;

        assert!(local.sender.lock().await.send(Bytes::from_static(b"hello")).await.is_err());
        local.sender.lock().await.send(Bytes::from_static(b"test")).await?;
        assert_eq!(peer.receiver.lock().await.recv().await?, Bytes::from_static(b"test"));

        peer.sender.lock().await.send(Bytes::from_static(b"hello")).await?;
        assert!(local.receiver.lock().await.recv().await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn raised_limit_accepts_a_buffered_frame() -> TestResult {
        let (local, peer) = stream_pair();
        local.set_max_frame_length(4).await;
        peer.sender.lock().await.send(Bytes::from_static(b"test")).await?;
        peer.sender.lock().await.send(Bytes::from_static(b"hello")).await?;
        assert_eq!(local.receiver.lock().await.recv().await?, Bytes::from_static(b"test"));

        local.set_max_frame_length(5).await;
        assert_eq!(local.receiver.lock().await.recv().await?, Bytes::from_static(b"hello"));
        local.sender.lock().await.send(Bytes::from_static(b"hello")).await?;
        assert_eq!(peer.receiver.lock().await.recv().await?, Bytes::from_static(b"hello"));

        Ok(())
    }

    #[tokio::test]
    async fn lowered_limit_rejects_a_buffered_frame() -> TestResult {
        let (local, peer) = stream_pair();
        local.set_max_frame_length(5).await;
        peer.sender.lock().await.send(Bytes::from_static(b"hello")).await?;
        peer.sender.lock().await.send(Bytes::from_static(b"world")).await?;
        assert_eq!(local.receiver.lock().await.recv().await?, Bytes::from_static(b"hello"));

        local.set_max_frame_length(4).await;
        assert!(local.receiver.lock().await.recv().await.is_err());
        assert!(local.sender.lock().await.send(Bytes::from_static(b"hello")).await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn a_frame_over_the_limit_is_an_invalid_format_error() -> TestResult {
        let (local, peer) = stream_pair();
        local.set_max_frame_length(4).await;
        peer.sender.lock().await.send(Bytes::from_static(b"hello")).await?;

        let error = local.receiver.lock().await.recv().await.err().expect("an over-limit frame fails");
        assert_eq!(error.kind(), &omnius_core_omnikit::ErrorKind::InvalidFormat);

        Ok(())
    }

    #[tokio::test]
    async fn a_closed_stream_is_not_an_invalid_format_error() -> TestResult {
        let (local, peer) = stream_pair();
        drop(peer);

        let error = local.receiver.lock().await.recv().await.err().expect("a closed stream fails");
        assert_eq!(error.kind(), &omnius_core_omnikit::ErrorKind::EndOfStream);

        Ok(())
    }

    fn stream_pair() -> (FramedStream, FramedStream) {
        let (local, peer) = tokio::io::duplex(128);
        let (local_reader, local_writer) = tokio::io::split(local);
        let (peer_reader, peer_writer) = tokio::io::split(peer);
        (FramedStream::new(local_reader, local_writer), FramedStream::new(peer_reader, peer_writer))
    }
}
