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
    slots: Arc<tokio::sync::Semaphore>,
    intervals: ProfileExchangerIntervals,
    cancellation_token: tokio_util::sync::CancellationToken,
    join_handles: TokioMutex<Vec<JoinHandle<()>>>,
}

#[async_trait]
impl Shutdown for TaskAccepter {
    async fn shutdown(&self) {
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
        slots: Arc<tokio::sync::Semaphore>,
        intervals: ProfileExchangerIntervals,
        cancellation_token: tokio_util::sync::CancellationToken,
    ) -> Arc<Self> {
        let v = Arc::new(Self {
            local_profiles,
            session_accepter,
            slots,
            intervals,
            cancellation_token,
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
        v.join_handles.try_lock().expect("the task was just created").push(join_handle);
        v
    }

    #[cfg(test)]
    pub fn session_slots(&self) -> &Arc<tokio::sync::Semaphore> {
        &self.slots
    }

    async fn accept(&self) -> Result<()> {
        let permit = select! {
            permit = self.slots.clone().acquire_owned() => permit.map_err(|e| Error::from_error(e, ErrorKind::UnexpectedError))?,
            _ = self.cancellation_token.cancelled() => return Ok(()),
        };
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
            drop(permit);
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
            session.stream.sender.lock().await.send_message_with::<ExchangeMessageCodec>(&response).await?;
        }
    }
}
