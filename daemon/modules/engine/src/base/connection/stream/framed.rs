use std::sync::Arc;

use omnius_core_omnikit::service::connection::codec::{FramedReceiver, FramedRecv, FramedSend, FramedSender};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Mutex as TokioMutex,
};

const MAX_FRAME_LENGTH: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct FramedStream {
    pub receiver: Arc<TokioMutex<dyn FramedRecv + Send + Unpin>>,
    pub sender: Arc<TokioMutex<dyn FramedSend + Send + Unpin>>,
}

impl FramedStream {
    pub fn new<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let receiver = Arc::new(TokioMutex::new(FramedReceiver::new(reader, MAX_FRAME_LENGTH)));
        let sender = Arc::new(TokioMutex::new(FramedSender::new(writer, MAX_FRAME_LENGTH)));
        Self { receiver, sender }
    }
}
