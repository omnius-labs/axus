use std::{path::Path, sync::Arc};

use chrono::{DateTime, Utc};
use sqlx::sqlite::SqlitePool;

use omnius_core_base::clock::Clock;
use omnius_core_migration::sqlite::{MigrationRequest, SqliteMigrator};

use crate::{
    model::{Profile, ProfileId},
    prelude::*,
};

const DATABASE_FILE_NAME: &str = "sqlite.db";

/// The result of storing a received snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileReplaceResult {
    /// The stored snapshot was replaced or inserted.
    Replaced,
    /// The incoming snapshot is not newer, so the stored one was kept.
    Ignored,
    /// The incoming snapshot has the same update time and version but another body.
    Equivocation,
}

/// Stores signed Profile bodies and the active flag in SQLite.
pub(crate) struct ProfileRepo {
    db: Arc<SqlitePool>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

impl ProfileRepo {
    pub async fn new(profile_dir: &Path, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Self> {
        tokio::fs::create_dir_all(profile_dir).await?;
        let path = profile_dir.join(DATABASE_FILE_NAME);
        let path = path.to_str().ok_or_else(|| Error::new(ErrorKind::UnexpectedError).with_message("Invalid path"))?;

        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
            .busy_timeout(std::time::Duration::from_secs(10));

        #[cfg(target_os = "macos")]
        let options = options.pragma("fullfsync", "ON");

        let db = Arc::new(SqlitePool::connect_with(options).await?);
        Self::migrate(db.as_ref()).await?;

        Ok(Self { db, clock })
    }

    async fn migrate(db: &SqlitePool) -> Result<()> {
        let requests = vec![MigrationRequest {
            name: "2026-11-14_init".to_string(),
            queries: r#"
CREATE TABLE IF NOT EXISTS profiles (
    id TEXT NOT NULL PRIMARY KEY,
    value BLOB NOT NULL,
    version INTEGER NOT NULL,
    updated_time INTEGER NOT NULL,
    created_time INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1))
);
CREATE UNIQUE INDEX IF NOT EXISTS unique_index_active_profile ON profiles (active) WHERE active = 1;
"#
            .to_string(),
        }];

        SqliteMigrator::migrate(db, requests).await?;

        Ok(())
    }

    pub async fn get(&self, id: &ProfileId) -> Result<Option<Profile>> {
        let value: Option<(Vec<u8>,)> = sqlx::query_as(
            r#"
SELECT value
    FROM profiles
    WHERE id = ?
"#,
        )
        .bind(id.as_str())
        .fetch_optional(self.db.as_ref())
        .await?;

        value.map(|(value,)| Profile::import(&value)).transpose()
    }

    /// Every stored Profile, ordered by creation time and then by Profile ID.
    pub async fn list(&self) -> Result<Vec<Profile>> {
        let values: Vec<(Vec<u8>,)> = sqlx::query_as(
            r#"
SELECT value
    FROM profiles
    ORDER BY created_time ASC, id ASC
"#,
        )
        .fetch_all(self.db.as_ref())
        .await?;

        values.into_iter().map(|(value,)| Profile::import(&value)).collect()
    }

    pub async fn active(&self) -> Result<Option<Profile>> {
        let value: Option<(Vec<u8>,)> = sqlx::query_as(
            r#"
SELECT value
    FROM profiles
    WHERE active = 1
"#,
        )
        .fetch_optional(self.db.as_ref())
        .await?;

        value.map(|(value,)| Profile::import(&value)).transpose()
    }

    /// Inserts a body as inactive. Callers activate it separately.
    pub async fn insert(&self, profile: &Profile, created_time: DateTime<Utc>) -> Result<()> {
        let value = profile.export()?;
        sqlx::query(
            r#"
INSERT INTO profiles (id, value, version, updated_time, created_time, active)
    VALUES (?, ?, ?, ?, ?, 0)
"#,
        )
        .bind(profile.id().as_str())
        .bind(value)
        .bind(i64::try_from(profile.version())?)
        .bind(profile.updated_time())
        .bind(created_time.timestamp())
        .execute(self.db.as_ref())
        .await?;

        Ok(())
    }

    /// Makes `id` the only active row.
    pub async fn activate(&self, id: &ProfileId) -> Result<()> {
        let mut tx = self.db.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(
            r#"
UPDATE profiles
    SET active = 0
    WHERE active = 1
"#,
        )
        .execute(&mut *tx)
        .await?;
        let changed = sqlx::query(
            r#"
UPDATE profiles
    SET active = 1
    WHERE id = ?
"#,
        )
        .bind(id.as_str())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed == 0 {
            return Err(Error::new(ErrorKind::NotFound).with_message("profile not found"));
        }
        tx.commit().await?;

        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn deactivate_all(&self) -> Result<()> {
        sqlx::query("UPDATE profiles SET active = 0").execute(self.db.as_ref()).await?;
        Ok(())
    }

    /// Replaces the stored snapshot only when `profile` is newer.
    pub async fn replace_if_newer(&self, profile: &Profile) -> Result<ProfileReplaceResult> {
        let now = self.clock.now();
        if !profile.is_updated_within_skew(now) {
            return Err(Error::new(ErrorKind::InvalidFormat).with_message("profile update time is too far in the future"));
        }
        let value = profile.export()?;

        let mut tx = self.db.begin_with("BEGIN IMMEDIATE").await?;
        let current: Option<(Vec<u8>, i64, i64)> = sqlx::query_as(
            r#"
SELECT value, version, updated_time
    FROM profiles
    WHERE id = ?
"#,
        )
        .bind(profile.id().as_str())
        .fetch_optional(&mut *tx)
        .await?;

        let result = match current {
            None => {
                sqlx::query(
                    r#"
INSERT INTO profiles (id, value, version, updated_time, created_time, active)
    VALUES (?, ?, ?, ?, ?, 0)
"#,
                )
                .bind(profile.id().as_str())
                .bind(&value)
                .bind(i64::try_from(profile.version())?)
                .bind(profile.updated_time())
                .bind(now.timestamp())
                .execute(&mut *tx)
                .await?;
                ProfileReplaceResult::Replaced
            }
            Some((current_value, current_version, current_updated_time)) => {
                let newer =
                    profile.updated_time() > current_updated_time || (profile.updated_time() == current_updated_time && i64::try_from(profile.version())? > current_version);
                let same_snapshot = profile.updated_time() == current_updated_time && i64::try_from(profile.version())? == current_version;
                if newer {
                    sqlx::query(
                        r#"
UPDATE profiles
    SET value = ?, version = ?, updated_time = ?
    WHERE id = ?
"#,
                    )
                    .bind(&value)
                    .bind(i64::try_from(profile.version())?)
                    .bind(profile.updated_time())
                    .bind(profile.id().as_str())
                    .execute(&mut *tx)
                    .await?;
                    ProfileReplaceResult::Replaced
                } else if same_snapshot && value != current_value {
                    ProfileReplaceResult::Equivocation
                } else {
                    ProfileReplaceResult::Ignored
                }
            }
        };

        tx.commit().await?;

        if result == ProfileReplaceResult::Equivocation {
            warn!(
                profile_id = %profile.id(),
                updated_time = profile.updated_time(),
                version = profile.version(),
                "ignoring an equivocating profile snapshot; the first body is kept"
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use omnius_core_base::clock::{Clock as _, FakeClockUtc};
    use omnius_core_omnikit::generated::omni_sign::{OmniSignType, OmniSigner};
    use testresult::TestResult;

    use super::{ProfileReplaceResult, ProfileRepo};
    use crate::{
        model::{Profile, ProfileDraft, ProfileId},
        prelude::*,
    };

    fn at(seconds: i64) -> DateTime<chrono::Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    fn signer(name: &str) -> OmniSigner {
        OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, name).unwrap()
    }

    fn profile(signer: &OmniSigner, version: u64, updated_time: i64) -> Profile {
        Profile::sign(signer, version, at(updated_time), ProfileDraft::default()).unwrap()
    }

    async fn create_repo(dir: &std::path::Path) -> Result<(ProfileRepo, std::sync::Arc<FakeClockUtc>)> {
        let clock = std::sync::Arc::new(FakeClockUtc::new(at(1_000_000)));
        Ok((ProfileRepo::new(dir, clock.clone()).await?, clock))
    }

    #[tokio::test]
    async fn profile_repo_a_stored_profile_survives_a_restart() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let profile = profile(&signer("Anonymous"), 1, 1_000_000);
        repo.insert(&profile, clock.now()).await?;

        let (repo, _) = create_repo(dir.path()).await?;
        assert_eq!(repo.get(profile.id()).await?, Some(profile));
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_exactly_one_profile_is_active() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let first = profile(&signer("First"), 1, 1_000_000);
        let second = profile(&signer("Second"), 1, 1_000_000);
        repo.insert(&first, clock.now()).await?;
        repo.insert(&second, clock.now()).await?;

        repo.activate(first.id()).await?;
        assert_eq!(repo.active().await?, Some(first.clone()));
        repo.activate(second.id()).await?;
        assert_eq!(repo.active().await?, Some(second));

        let (active_count,): (i64,) = sqlx::query_as("SELECT COUNT(1) FROM profiles WHERE active = 1").fetch_one(repo.db.as_ref()).await?;
        assert_eq!(active_count, 1);
        assert!(repo.activate(&ProfileId::new("Missing", b"missing")?).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_a_partial_index_rejects_a_second_active_row() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let first = profile(&signer("First"), 1, 1_000_000);
        let second = profile(&signer("Second"), 1, 1_000_000);
        repo.insert(&first, clock.now()).await?;
        repo.insert(&second, clock.now()).await?;
        repo.activate(first.id()).await?;

        assert!(sqlx::query("UPDATE profiles SET active = 1").execute(repo.db.as_ref()).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_a_newer_snapshot_replaces_an_older_one() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let signer = signer("Anonymous");
        let first = profile(&signer, 1, 1_000_000);
        let second = profile(&signer, 2, 1_000_001);
        repo.insert(&first, clock.now()).await?;

        assert_eq!(repo.replace_if_newer(&second).await?, ProfileReplaceResult::Replaced);
        assert_eq!(repo.get(first.id()).await?, Some(second));
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_an_older_snapshot_is_ignored() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let signer = signer("Anonymous");
        let current = profile(&signer, 2, 1_000_001);
        let older = profile(&signer, 1, 1_000_000);
        let same_time_lower_version = profile(&signer, 1, 1_000_001);
        repo.insert(&current, clock.now()).await?;

        assert_eq!(repo.replace_if_newer(&older).await?, ProfileReplaceResult::Ignored);
        assert_eq!(repo.replace_if_newer(&same_time_lower_version).await?, ProfileReplaceResult::Ignored);
        assert_eq!(repo.get(current.id()).await?, Some(current));
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_an_equal_update_time_uses_the_higher_version() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let signer = signer("Anonymous");
        let first = profile(&signer, 1, 1_000_000);
        let second = profile(&signer, 2, 1_000_000);
        repo.insert(&first, clock.now()).await?;

        assert_eq!(repo.replace_if_newer(&second).await?, ProfileReplaceResult::Replaced);
        assert_eq!(repo.get(first.id()).await?, Some(second));
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_an_equal_time_and_version_keeps_the_first_body() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let signer = signer("Anonymous");
        let first = profile(&signer, 1, 1_000_000);
        let other = Profile::sign(
            &signer,
            1,
            at(1_000_000),
            ProfileDraft {
                messages: vec![crate::generated::axus::model::ProfileMessage {
                    posted_time: 1_000_000,
                    body: crate::generated::axus::model::ProfileMessageBody::Inline { value: b"different".to_vec() },
                }],
                ..Default::default()
            },
        )?;
        repo.insert(&first, clock.now()).await?;

        assert_eq!(repo.replace_if_newer(&other).await?, ProfileReplaceResult::Equivocation);
        assert_eq!(repo.get(first.id()).await?, Some(first.clone()));
        assert_eq!(repo.replace_if_newer(&first).await?, ProfileReplaceResult::Ignored);
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_an_update_time_more_than_five_minutes_ahead_is_rejected() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let now = clock.now().timestamp();
        let signer = signer("Anonymous");

        let far_ahead = profile(&signer, 1, now + Profile::MAX_FUTURE_UPDATE_SKEW.as_secs() as i64 + 1);
        assert!(repo.replace_if_newer(&far_ahead).await.is_err());
        let at_limit = profile(&signer, 1, now + Profile::MAX_FUTURE_UPDATE_SKEW.as_secs() as i64);
        assert_eq!(repo.replace_if_newer(&at_limit).await?, ProfileReplaceResult::Replaced);
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_a_replacement_preserves_the_creation_time() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let signer = signer("Anonymous");
        let first = profile(&signer, 1, 1_000_000);
        repo.insert(&first, at(900_000)).await?;
        clock.advance(std::time::Duration::from_secs(10));
        let second = profile(&signer, 2, 1_000_001);
        repo.replace_if_newer(&second).await?;

        let (created_time,): (i64,) = sqlx::query_as("SELECT created_time FROM profiles WHERE id = ?")
            .bind(first.id().as_str())
            .fetch_one(repo.db.as_ref())
            .await?;
        assert_eq!(created_time, 900_000);
        Ok(())
    }

    #[tokio::test]
    async fn profile_repo_a_stored_body_with_a_broken_signature_is_rejected() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, clock) = create_repo(dir.path()).await?;
        let profile = profile(&signer("Anonymous"), 1, 1_000_000);
        repo.insert(&profile, clock.now()).await?;
        sqlx::query("UPDATE profiles SET value = ? WHERE id = ?")
            .bind(b"broken".to_vec())
            .bind(profile.id().as_str())
            .execute(repo.db.as_ref())
            .await?;

        assert!(repo.get(profile.id()).await.is_err());
        assert!(repo.list().await.is_err());
        Ok(())
    }
}
