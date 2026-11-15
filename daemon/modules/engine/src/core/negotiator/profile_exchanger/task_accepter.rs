use std::sync::Arc;

use async_trait::async_trait;
use tokio::{select, sync::Mutex as TokioMutex, task::JoinHandle};
use tracing::warn;

use crate::{
    base::{
        connection::{FramedRecvExt as _, FramedSendExt as _},
        runtime::Shutdown,
    },
    core::{
        profile::LocalProfiles,
        session::{SessionAccepter, model::SessionType},
    },
    generated::axus::profile::ExchangeMessage,
    model::ProfileId,
    prelude::*,
    protocol::profile::ExchangeMessageCodec,
};

use super::ProfileExchangerIntervals;

/// Accepts the Sessions peers dialed for a Profile and runs one serve worker per Session.
pub struct TaskAccepter {
    local_profiles: Arc<LocalProfiles>,
    session_accepter: Arc<SessionAccepter>,
    intervals: ProfileExchangerIntervals,
    cancellation_token: tokio_util::sync::CancellationToken,
    main_handle: TokioMutex<Option<JoinHandle<()>>>,
    join_handles: TokioMutex<Vec<JoinHandle<()>>>,
}

#[async_trait]
impl Shutdown for TaskAccepter {
    async fn shutdown(&self) {
        self.cancellation_token.cancel();
        if let Some(main_handle) = self.main_handle.lock().await.take() {
            let _ = main_handle.await;
        }
        let handles = std::mem::take(&mut *self.join_handles.lock().await);
        for handle in handles {
            let _ = handle.await;
        }
    }
}

impl TaskAccepter {
    pub fn new(
        local_profiles: Arc<LocalProfiles>,
        session_accepter: Arc<SessionAccepter>,
        intervals: ProfileExchangerIntervals,
        cancellation_token: tokio_util::sync::CancellationToken,
    ) -> Arc<Self> {
        let v = Arc::new(Self {
            local_profiles,
            session_accepter,
            intervals,
            cancellation_token,
            main_handle: TokioMutex::new(None),
            join_handles: TokioMutex::new(Vec::new()),
        });
        let this = v.clone();
        let join_handle = tokio::spawn(async move {
            while !this.cancellation_token.is_cancelled() {
                select! {
                    result = this.accept() => {
                        if let Err(error) = result {
                            warn!(error_message = error.to_string(), "profile exchange accept failed");
                            select! {
                                _ = tokio::time::sleep(this.intervals.accept_retry) => {}
                                _ = this.cancellation_token.cancelled() => break,
                            }
                        }
                    }
                    _ = this.cancellation_token.cancelled() => break,
                }
            }
        });
        *v.main_handle.try_lock().expect("the task was just created") = Some(join_handle);
        v
    }

    #[cfg(test)]
    pub async fn active_worker_count(&self) -> usize {
        self.join_handles.lock().await.iter().filter(|handle| !handle.is_finished()).count()
    }

    async fn accept(&self) -> Result<()> {
        let session = select! {
            session = self.session_accepter.accept(&SessionType::ProfileExchanger) => session?,
            _ = self.cancellation_token.cancelled() => return Ok(()),
        };

        let local_profiles = self.local_profiles.clone();
        let receive_timeout = self.intervals.receive_timeout;
        let cancellation_token = self.cancellation_token.clone();
        let mut workers = self.join_handles.lock().await;
        workers.retain(|handle| !handle.is_finished());
        workers.push(tokio::spawn(async move {
            let result = Self::serve_session(session, local_profiles, receive_timeout, cancellation_token).await;
            if let Err(error) = result {
                debug!(error_message = error.to_string(), "profile exchange serve session ended");
            }
        }));

        Ok(())
    }

    /// Answers Requests until the peer closes, the receive deadline passes, or shutdown begins.
    async fn serve_session(
        session: crate::core::session::model::Session,
        local_profiles: Arc<LocalProfiles>,
        receive_timeout: std::time::Duration,
        cancellation_token: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        session.stream.set_receive_timeout(receive_timeout);

        loop {
            let mut receiver = session.stream.receiver.lock().await;
            let message = select! {
                message = receiver.recv_message_with::<ExchangeMessageCodec>() => message?,
                _ = cancellation_token.cancelled() => return Ok(()),
            };
            drop(receiver);

            let ExchangeMessage::Request { profile_id } = message else {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected message instead of a profile request"));
            };
            let id = ProfileId::parse(&profile_id)?;
            let response = match local_profiles.get(&id).await? {
                Some(profile) => ExchangeMessage::Response { profile: profile.to_wire() },
                None => ExchangeMessage::NotFound { profile_id: profile_id.clone() },
            };
            select! {
                result = async {
                    session.stream.sender.lock().await.send_message_with::<ExchangeMessageCodec>(&response).await
                } => result?,
                _ = cancellation_token.cancelled() => return Ok(()),
            }
        }
    }
}
