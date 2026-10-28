use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use parking_lot::Mutex;
use tokio::time::{Instant, timeout_at};
use tokio_util::bytes::Bytes;

use omnius_core_base::error::OmniError as _;
use omnius_core_omnikit::{Error, ErrorKind, Result, service::connection::codec::FramedRecv};

/// The time since the last received frame that a stream may stay silent.
/// The clock restarts when the timeout is set and at every received frame, and the local node's sends do not extend it.
#[derive(Clone, Default)]
pub struct ReceiveDeadline {
    state: Arc<Mutex<Option<(Duration, Instant)>>>,
}

impl ReceiveDeadline {
    /// Whether the error, or one of its causes, is an I/O error of the kind.
    pub fn has_io_error_kind(error: &(dyn std::error::Error + 'static), kind: std::io::ErrorKind) -> bool {
        let mut source = Some(error);
        while let Some(cause) = source {
            if cause.downcast_ref::<std::io::Error>().is_some_and(|io| io.kind() == kind) {
                return true;
            }
            source = cause.source();
        }
        false
    }

    /// Whether the error is a receive that exceeded its deadline.
    pub fn is_timeout(error: &(dyn std::error::Error + 'static)) -> bool {
        Self::has_io_error_kind(error, std::io::ErrorKind::TimedOut)
    }

    pub fn set(&self, timeout: Duration) {
        *self.state.lock() = Some((timeout, Instant::now()));
    }

    fn limit(&self) -> Option<Instant> {
        self.state.lock().map(|(timeout, received_at)| received_at + timeout)
    }

    fn touch(&self) {
        if let Some((_, received_at)) = self.state.lock().as_mut() {
            *received_at = Instant::now();
        }
    }
}

pub struct DeadlineReceiver<R> {
    inner: R,
    deadline: ReceiveDeadline,
}

impl<R> DeadlineReceiver<R> {
    pub fn new(inner: R, deadline: ReceiveDeadline) -> Self {
        Self { inner, deadline }
    }
}

#[async_trait]
impl<R> FramedRecv for DeadlineReceiver<R>
where
    R: FramedRecv + Send,
{
    fn set_max_frame_length(&mut self, max_frame_length: usize) {
        self.inner.set_max_frame_length(max_frame_length);
    }

    async fn recv(&mut self) -> Result<Bytes> {
        let received = match self.deadline.limit() {
            None => self.inner.recv().await,
            Some(limit) => timeout_at(limit, self.inner.recv())
                .await
                .map_err(|e| Error::from_error(std::io::Error::new(std::io::ErrorKind::TimedOut, e), ErrorKind::IoError).with_message("receive deadline exceeded"))?,
        };
        let bytes = received.map_err(Self::classify)?;
        self.deadline.touch();
        Ok(bytes)
    }
}

impl<R> DeadlineReceiver<R> {
    /// The frame codec reports a frame over the limit as invalid data, which is the peer's fault unlike a reset or a close.
    fn classify(error: Error) -> Error {
        if ReceiveDeadline::has_io_error_kind(&error, std::io::ErrorKind::InvalidData) {
            return Error::from_error(error, ErrorKind::InvalidFormat).with_message("received frame is invalid");
        }
        error
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use testresult::TestResult;
    use tokio::io::AsyncWriteExt as _;
    use tokio_util::bytes::Bytes;

    use omnius_core_omnikit::service::connection::codec::{FramedReceiver, FramedRecv as _};

    use super::{DeadlineReceiver, ReceiveDeadline};

    #[tokio::test]
    async fn silent_peer_fails_at_the_deadline() -> TestResult {
        let (_writer, reader) = tokio::io::duplex(64);
        let deadline = ReceiveDeadline::default();
        let mut receiver = DeadlineReceiver::new(FramedReceiver::new(reader, 16), deadline.clone());
        let started = tokio::time::Instant::now();
        deadline.set(Duration::from_millis(100));

        let error = receiver.recv().await.err().expect("a silent peer fails");
        assert!(ReceiveDeadline::is_timeout(&error));
        assert!(started.elapsed() >= Duration::from_millis(100));

        Ok(())
    }

    #[tokio::test]
    async fn deadline_counts_from_the_last_received_frame_not_from_the_call() -> TestResult {
        let (mut writer, reader) = tokio::io::duplex(64);
        let deadline = ReceiveDeadline::default();
        let mut receiver = DeadlineReceiver::new(FramedReceiver::new(reader, 16), deadline.clone());
        deadline.set(Duration::from_millis(300));

        writer.write_all(b"\x01\x00\x00\x00a").await?;
        assert_eq!(receiver.recv().await?, Bytes::from_static(b"a"));

        // The time spent before the call counts, because the clock restarted at the received frame and not at the call.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let started = tokio::time::Instant::now();
        assert!(receiver.recv().await.is_err());
        assert!(started.elapsed() < Duration::from_millis(200));

        Ok(())
    }

    #[tokio::test]
    async fn without_a_timeout_the_receive_waits_for_data() -> TestResult {
        let (mut writer, reader) = tokio::io::duplex(64);
        let mut receiver = DeadlineReceiver::new(FramedReceiver::new(reader, 16), ReceiveDeadline::default());

        let send = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            writer.write_all(b"\x01\x00\x00\x00a").await
        });
        assert_eq!(receiver.recv().await?, Bytes::from_static(b"a"));
        send.await??;

        Ok(())
    }

    #[tokio::test]
    async fn a_received_frame_restarts_the_deadline() -> TestResult {
        let (mut writer, reader) = tokio::io::duplex(64);
        let deadline = ReceiveDeadline::default();
        let mut receiver = DeadlineReceiver::new(FramedReceiver::new(reader, 16), deadline.clone());
        deadline.set(Duration::from_millis(300));

        for _ in 0..3 {
            tokio::time::sleep(Duration::from_millis(150)).await;
            writer.write_all(b"\x01\x00\x00\x00a").await?;
            assert_eq!(receiver.recv().await?, Bytes::from_static(b"a"));
        }

        Ok(())
    }
}
