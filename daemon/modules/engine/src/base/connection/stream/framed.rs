use std::sync::Arc;

use omnius_core_omnikit::service::connection::codec::{FramedReceiver, FramedRecv, FramedSend, FramedSender};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Mutex as TokioMutex,
};

#[derive(Clone)]
pub struct FramedStream {
    pub receiver: Arc<TokioMutex<dyn FramedRecv + Send + Unpin>>,
    pub sender: Arc<TokioMutex<dyn FramedSend + Send + Unpin>>,
}

impl FramedStream {
    pub const FILE_EXCHANGER_MAX_FRAME_LENGTH: usize = 64 * 1024 * 1024;

    pub fn new<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let receiver = Arc::new(TokioMutex::new(FramedReceiver::new(reader, Self::FILE_EXCHANGER_MAX_FRAME_LENGTH)));
        let sender = Arc::new(TokioMutex::new(FramedSender::new(writer, Self::FILE_EXCHANGER_MAX_FRAME_LENGTH)));
        Self { receiver, sender }
    }

    #[allow(unused)]
    pub async fn set_max_frame_length(&self, max_frame_length: usize) {
        self.receiver.lock().await.set_max_frame_length(max_frame_length);
        self.sender.lock().await.set_max_frame_length(max_frame_length);
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;
    use tokio_util::bytes::Bytes;

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

    fn stream_pair() -> (FramedStream, FramedStream) {
        let (local, peer) = tokio::io::duplex(128);
        let (local_reader, local_writer) = tokio::io::split(local);
        let (peer_reader, peer_writer) = tokio::io::split(peer);
        (FramedStream::new(local_reader, local_writer), FramedStream::new(peer_reader, peer_writer))
    }
}
