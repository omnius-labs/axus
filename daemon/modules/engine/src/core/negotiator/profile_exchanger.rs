use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use omnius_core_base::clock::Clock;

use crate::{
    base::{
        connection::{FramedRecvExt as _, FramedSendExt as _},
        runtime::Shutdown,
    },
    core::{
        profile::{FetchedProfileReplaceResult, LocalProfiles},
        session::{
            SessionAccepter, SessionConnector,
            model::{Session, SessionType},
        },
    },
    generated::axus::profile::ExchangeMessage,
    model::{AssetKey, Profile, ProfileId},
    prelude::*,
    protocol::profile::ExchangeMessageCodec,
};

mod location_finder;
mod task_accepter;
#[cfg(test)]
mod tests;

pub use location_finder::*;
use task_accepter::TaskAccepter;

#[derive(Debug, Clone)]
pub struct ProfileExchangerOption {
    /// Sessions dialed and accepted together.
    pub max_session_count: usize,
}

impl Default for ProfileExchangerOption {
    fn default() -> Self {
        Self { max_session_count: 8 }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileExchangerIntervals {
    pub receive_timeout: Duration,
    pub accept_retry: Duration,
}

impl Default for ProfileExchangerIntervals {
    fn default() -> Self {
        Self {
            receive_timeout: Duration::from_secs(30),
            accept_retry: Duration::from_secs(1),
        }
    }
}

/// Finds and fetches the signed body of a Profile by its ID, and serves the Profiles this node owns.
pub struct ProfileExchanger {
    local_profiles: Arc<LocalProfiles>,
    session_connector: Arc<SessionConnector>,
    location_finder: Arc<dyn ProfileLocationFinder>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    intervals: ProfileExchangerIntervals,
    slots: Arc<Semaphore>,
    cancellation_token: CancellationToken,
    task_accepter: Arc<TaskAccepter>,
}

impl ProfileExchanger {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        local_profiles: Arc<LocalProfiles>,
        session_connector: Arc<SessionConnector>,
        session_accepter: Arc<SessionAccepter>,
        location_finder: Arc<dyn ProfileLocationFinder>,
        clock: Arc<dyn Clock<Utc> + Send + Sync>,
        option: ProfileExchangerOption,
        intervals: ProfileExchangerIntervals,
    ) -> Result<Self> {
        let slots = Arc::new(Semaphore::new(option.max_session_count));
        let cancellation_token = CancellationToken::new();
        let task_accepter = TaskAccepter::new(local_profiles.clone(), session_accepter, slots.clone(), intervals.clone(), cancellation_token.clone());
        let exchanger = Self {
            local_profiles,
            session_connector,
            location_finder,
            clock,
            intervals,
            slots,
            cancellation_token,
            task_accepter,
        };

        if let Err(error) = exchanger.refresh_provided().await {
            exchanger.shutdown().await;
            return Err(error);
        }

        Ok(exchanger)
    }

    pub fn my_node_id(&self) -> Vec<u8> {
        self.location_finder.my_node_id()
    }

    /// Replaces the Profile AssetKeys advertised by push with the Profiles this node owns.
    pub async fn refresh_provided(&self) -> Result<()> {
        let asset_keys = self.local_profiles.list().await?.into_iter().map(|profile| AssetKey::profile(profile.id())).collect();
        self.location_finder.set_provided(asset_keys);
        Ok(())
    }

    /// Reads a Profile this node owns or has fetched, without network access.
    pub async fn stored(&self, id: &ProfileId) -> Result<Option<Profile>> {
        if let Some(profile) = self.local_profiles.get(id).await? {
            return Ok(Some(profile));
        }
        Ok(self.local_profiles.fetched(id).await?.map(|fetched| fetched.profile))
    }

    /// Fetches a Profile by ID from the locations NodeFinder returns.
    ///
    /// An owned Profile is returned directly. NotFound, a bad body, and a stale or equivocating
    /// snapshot move to the next location; the serving node is not excluded for a bad body.
    pub async fn fetch(&self, id: &ProfileId) -> Result<Profile> {
        if let Some(profile) = self.local_profiles.get(id).await? {
            return Ok(profile);
        }

        let locations = self.location_finder.find(&AssetKey::profile(id)).await?;
        if locations.is_empty() {
            return Err(Error::new(ErrorKind::NotFound).with_message(format!("profile not found: {id}")));
        }

        let mut last_error = None;
        for node_profile in locations {
            if node_profile.id() == self.my_node_id() {
                continue;
            }
            for addr in node_profile.addrs() {
                let permit = self.slots.clone().acquire_owned().await.map_err(|e| Error::from_error(e, ErrorKind::UnexpectedError))?;
                let session = match self.session_connector.connect(addr, &SessionType::ProfileExchanger).await {
                    Ok(session) => session,
                    Err(error) => {
                        last_error = Some(error);
                        continue;
                    }
                };
                if !session.is_signed_by(&node_profile) {
                    warn!(address = addr.to_string(), "the peer's key does not match the node profile");
                    drop(permit);
                    continue;
                }

                let result = self.fetch_from_session(&session, id).await;
                drop(session);
                drop(permit);

                match result {
                    Ok(profile) => return Ok(profile),
                    Err(error) => last_error = Some(error),
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::new(ErrorKind::NotFound).with_message(format!("profile not found: {id}"))))
    }

    async fn fetch_from_session(&self, session: &Session, id: &ProfileId) -> Result<Profile> {
        session.stream.set_receive_timeout(self.intervals.receive_timeout);

        let request = ExchangeMessage::Request { profile_id: id.to_string() };
        session.stream.sender.lock().await.send_message_with::<ExchangeMessageCodec>(&request).await?;

        let response = session.stream.receiver.lock().await.recv_message_with::<ExchangeMessageCodec>().await?;
        let profile = match response {
            ExchangeMessage::Response { profile } => profile,
            ExchangeMessage::NotFound { profile_id } => {
                return Err(Error::new(ErrorKind::NotFound).with_message(format!("profile not found: {profile_id}")));
            }
            _ => return Err(Error::new(ErrorKind::InvalidFormat).with_message("unexpected message instead of a profile response")),
        };

        let profile = Profile::verified(profile)?;
        if profile.id() != id {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("the response carries another profile id"));
        }

        match self.local_profiles.store_fetched(&profile, self.clock.now()).await? {
            FetchedProfileReplaceResult::Replaced | FetchedProfileReplaceResult::Unchanged => Ok(profile),
            FetchedProfileReplaceResult::Stale => Err(Error::new(ErrorKind::Reject).with_message("the fetched profile is older than the stored one")),
            FetchedProfileReplaceResult::Equivocation => Err(Error::new(ErrorKind::Reject).with_message("the fetched profile equivocates")),
        }
    }
}

#[async_trait]
impl Shutdown for ProfileExchanger {
    async fn shutdown(&self) {
        self.cancellation_token.cancel();
        self.task_accepter.shutdown().await;
    }
}
