use std::{path::Path, sync::Arc};

use chrono::Utc;
use omnius_core_base::clock::Clock;

use crate::{
    core::profile::{key::ProfileKeyStore, repo::ProfileReplaceResult, repo::ProfileRepo},
    model::{Profile, ProfileDraft, ProfileId},
    prelude::*,
};

/// The display name of the Profile a fresh state directory starts with.
const DEFAULT_PROFILE_NAME: &str = "Anonymous";

/// Owns the local Profiles: their keys, signed bodies, update rules, and the active Profile.
pub struct LocalProfiles {
    repo: ProfileRepo,
    keys: ProfileKeyStore,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    /// Serializes updates so two drafts cannot sign from the same snapshot.
    update_lock: TokioMutex<()>,
}

impl LocalProfiles {
    /// Opens the state directory, recovers keys and bodies, and ensures one active Profile.
    pub async fn open(profile_dir: &Path, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Self> {
        let keys = ProfileKeyStore::new(profile_dir);
        let repo = ProfileRepo::new(profile_dir, clock.clone()).await?;
        let service = Self {
            repo,
            keys,
            clock,
            update_lock: TokioMutex::new(()),
        };
        service.recover().await?;
        Ok(service)
    }

    async fn recover(&self) -> Result<()> {
        let keys = self.keys.list().await?;
        let bodies = self.repo.list().await?;

        for body in &bodies {
            if !keys.iter().any(|(id, _)| id == body.id()) {
                return Err(Error::new(ErrorKind::InvalidFormat).with_message(format!("profile body has no key: {}", body.id())));
            }
        }

        let now = self.clock.now();
        for (id, signer) in &keys {
            if bodies.iter().any(|body| body.id() == id) {
                continue;
            }
            let profile = Profile::sign(signer, 1, now, ProfileDraft::default())?;
            self.repo.insert(&profile, now).await?;
        }

        if self.repo.list().await?.is_empty() {
            let (_, signer) = self.keys.create(DEFAULT_PROFILE_NAME).await?;
            let profile = Profile::sign(&signer, 1, now, ProfileDraft::default())?;
            self.repo.insert(&profile, now).await?;
        }

        if self.repo.active().await?.is_none()
            && let Some(oldest) = self.repo.list().await?.first()
        {
            self.repo.activate(oldest.id()).await?;
        }

        Ok(())
    }

    /// Creates an inactive Profile with a new key and an empty snapshot.
    pub async fn create(&self, name: &str) -> Result<Profile> {
        let (_, signer) = self.keys.create(name).await?;
        let now = self.clock.now();
        let profile = Profile::sign(&signer, 1, now, ProfileDraft::default())?;
        self.repo.insert(&profile, now).await?;
        Ok(profile)
    }

    /// Every local Profile, ordered by creation time and then by Profile ID.
    pub async fn list(&self) -> Result<Vec<Profile>> {
        self.repo.list().await
    }

    pub async fn get(&self, id: &ProfileId) -> Result<Option<Profile>> {
        self.repo.get(id).await
    }

    pub async fn active(&self) -> Result<Option<Profile>> {
        self.repo.active().await
    }

    pub async fn activate(&self, id: &ProfileId) -> Result<()> {
        self.repo.activate(id).await
    }

    /// Closes the database pool so the service releases its state directory.
    pub async fn shutdown(&self) {
        self.repo.shutdown().await;
    }

    /// Signs and stores a complete draft as the next version of `id`.
    pub async fn update(&self, id: &ProfileId, draft: ProfileDraft) -> Result<Profile> {
        let _guard = self.update_lock.lock().await;

        let signer = self.keys.load(id).await?;
        let stored = self.repo.get(id).await?.ok_or_else(|| Error::new(ErrorKind::NotFound).with_message("profile not found"))?;
        let now = self.clock.now();
        if now.timestamp() < stored.updated_time() {
            return Err(Error::new(ErrorKind::Reject).with_message("current time is earlier than the stored profile update time"));
        }

        let version = stored
            .version()
            .checked_add(1)
            .ok_or_else(|| Error::new(ErrorKind::InvalidFormat).with_message("profile version overflow"))?;
        let profile = Profile::sign(&signer, version, now, draft)?;
        match self.repo.replace_if_newer(&profile).await? {
            ProfileReplaceResult::Replaced => Ok(profile),
            ProfileReplaceResult::Ignored | ProfileReplaceResult::Equivocation => Err(Error::new(ErrorKind::Reject).with_message("profile update was not stored")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::DateTime;
    use omnius_core_base::clock::{Clock as _, FakeClockUtc};
    use testresult::TestResult;

    use super::LocalProfiles;
    use crate::{
        model::{ProfileDraft, ProfileId},
        prelude::*,
    };

    fn at(seconds: i64) -> DateTime<chrono::Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    async fn open(dir: &std::path::Path, clock: &Arc<FakeClockUtc>) -> Result<LocalProfiles> {
        LocalProfiles::open(dir, clock.clone()).await
    }

    #[tokio::test]
    async fn local_profiles_startup_a_fresh_state_creates_an_active_anonymous_profile() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let service = open(dir.path(), &clock).await?;

        let active = service.active().await?.unwrap();
        assert_eq!(active.id().name(), "Anonymous");
        assert_eq!(active.version(), 1);
        assert_eq!(active.updated_time(), 1_000_000);
        assert_eq!(service.list().await?, vec![active.clone()]);
        assert!(dir.path().join("keys").join(active.id().as_str()).join("signer").exists());
        assert!(dir.path().join("sqlite.db").exists());

        let listed = service.list().await?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id(), active.id());
        Ok(())
    }

    #[tokio::test]
    async fn local_profiles_startup_reopening_does_not_duplicate_the_anonymous_profile() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let first = open(dir.path(), &clock).await?;
        let active = first.active().await?.unwrap();
        drop(first);

        let second = open(dir.path(), &clock).await?;
        assert_eq!(second.active().await?, Some(active.clone()));
        assert_eq!(second.list().await?, vec![active]);
        Ok(())
    }

    #[tokio::test]
    async fn local_profiles_startup_a_key_without_a_body_is_recovered_as_an_empty_profile() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let first = open(dir.path(), &clock).await?;
        let anonymous = first.active().await?.unwrap();
        drop(first);
        let (recovered_id, _) = super::super::key::ProfileKeyStore::new(dir.path()).create("Recovered").await?;

        let service = open(dir.path(), &clock).await?;
        let recovered = service.get(&recovered_id).await?.unwrap();
        assert_eq!(recovered.id(), &recovered_id);
        assert_eq!(recovered.version(), 1);
        assert_eq!(recovered.trusted(), []);
        assert_eq!(service.active().await?, Some(anonymous));
        Ok(())
    }

    #[tokio::test]
    async fn local_profiles_startup_a_body_without_a_key_fails_startup() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let service = open(dir.path(), &clock).await?;
        let second = service.create("Second").await?;
        drop(service);
        tokio::fs::remove_dir_all(dir.path().join("keys").join(second.id().as_str())).await?;

        assert!(open(dir.path(), &clock).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn local_profiles_startup_a_state_without_an_active_profile_activates_the_oldest() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let service = open(dir.path(), &clock).await?;
        clock.advance(std::time::Duration::from_secs(10));
        let second = service.create("Second").await?;
        let anonymous = service.active().await?.unwrap();
        service.repo.deactivate_all().await?;
        drop(service);

        let reopened = open(dir.path(), &clock).await?;
        assert_eq!(reopened.active().await?, Some(anonymous));
        assert_ne!(reopened.active().await?, Some(second));
        Ok(())
    }

    #[tokio::test]
    async fn local_profiles_create_list_get_active_activate_and_update() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let service = open(dir.path(), &clock).await?;
        let anonymous = service.active().await?.unwrap();
        assert_eq!(anonymous.id().name(), "Anonymous");

        clock.advance(std::time::Duration::from_secs(1));
        let second = service.create("Second").await?;
        assert_eq!(second.id().name(), "Second");
        assert_eq!(service.list().await?.len(), 2);
        assert_eq!(service.get(second.id()).await?, Some(second.clone()));
        assert_eq!(service.active().await?, Some(anonymous.clone()));

        service.activate(second.id()).await?;
        assert_eq!(service.active().await?, Some(second.clone()));

        clock.advance(std::time::Duration::from_secs(1));
        let updated = service
            .update(
                second.id(),
                ProfileDraft {
                    messages: vec![crate::generated::axus::model::ProfileMessage {
                        posted_time: clock.now().timestamp(),
                        body: crate::generated::axus::model::ProfileMessageBody::Inline { value: b"hello".to_vec() },
                    }],
                    ..Default::default()
                },
            )
            .await?;
        assert_eq!(updated.version(), 2);
        assert_eq!(updated.updated_time(), clock.now().timestamp());
        assert_eq!(updated.messages().len(), 1);
        assert_eq!(service.active().await?, Some(updated.clone()));

        drop(service);
        let reopened = open(dir.path(), &clock).await?;
        assert_eq!(reopened.active().await?, Some(updated));
        Ok(())
    }

    #[tokio::test]
    async fn local_profiles_a_clock_that_moves_back_is_rejected() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let service = open(dir.path(), &clock).await?;
        let active = service.active().await?.unwrap();
        service.update(active.id(), ProfileDraft::default()).await?;

        clock.set(at(999_999));
        assert!(service.update(active.id(), ProfileDraft::default()).await.is_err());

        clock.set(at(1_000_001));
        let updated = service.update(active.id(), ProfileDraft::default()).await?;
        assert_eq!(updated.version(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn local_profiles_updates_are_serialized_and_increment_the_version() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let service = Arc::new(open(dir.path(), &clock).await?);
        let active = service.active().await?.unwrap();

        let first = {
            let service = service.clone();
            let id = active.id().clone();
            async move { service.update(&id, ProfileDraft::default()).await }
        };
        let second = {
            let service = service.clone();
            let id = active.id().clone();
            async move { service.update(&id, ProfileDraft::default()).await }
        };
        let (first, second) = tokio::join!(first, second);
        let mut versions = vec![first?.version(), second?.version()];
        versions.sort();

        assert_eq!(versions, vec![2, 3]);
        assert_eq!(service.get(active.id()).await?.unwrap().version(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn local_profiles_activating_an_unknown_profile_is_rejected() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(at(1_000_000)));
        let service = open(dir.path(), &clock).await?;

        assert!(service.activate(&ProfileId::new("Missing", b"missing")?).await.is_err());
        Ok(())
    }
}
