use std::{path::Path, str::FromStr, sync::Arc};

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use sqlx::QueryBuilder;
use sqlx::sqlite::SqlitePool;

use omnius_core_base::clock::Clock;
use omnius_core_migration::sqlite::{MigrationRequest, SqliteMigrator};

use crate::{model::NodeProfile, prelude::*};

/// A node the store knows, with what the connection task needs to decide when to dial it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownNode {
    pub profile: NodeProfile,
    /// When this node was last observed alive, as unix seconds.
    pub last_seen: i64,
    /// How many dials in a row failed.
    pub fail_count: i64,
    /// When this node was last dialed, as unix seconds.
    pub last_attempt: i64,
}

impl KnownNode {
    /// The shortest wait after one failed dial, doubled for every further failure.
    pub const MIN_RETRY: std::time::Duration = std::time::Duration::from_secs(30);
    /// The longest wait after repeated failed dials.
    pub const MAX_RETRY: std::time::Duration = std::time::Duration::from_secs(30 * 60);

    /// Whether this node may be dialed now.
    ///
    /// A node that has never failed is always due; after a failure the wait doubles per failure up to `MAX_RETRY`.
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        if self.fail_count <= 0 {
            return true;
        }

        let exponent = u32::try_from(self.fail_count.min(16)).unwrap_or(16);
        let retry = (Self::MIN_RETRY * 2u32.saturating_pow(exponent - 1)).min(Self::MAX_RETRY);
        let wait = retry.as_secs() as i64;
        now.timestamp().saturating_sub(self.last_attempt) >= wait
    }
}

pub struct NodeFinderRepo {
    db: Arc<SqlitePool>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
    /// The parsed profiles of the last read, dropped whenever the store is written.
    cache: Mutex<Option<Vec<KnownNode>>>,
}

impl NodeFinderRepo {
    pub async fn new(state_dir: &str, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Self> {
        let path = Path::new(state_dir).join("sqlite.db");
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

        Ok(Self {
            db,
            clock,
            cache: Mutex::new(None),
        })
    }

    async fn migrate(db: &SqlitePool) -> Result<()> {
        let requests = vec![
            MigrationRequest {
                name: "2024-03-19_init".to_string(),
                queries: r#"
CREATE TABLE IF NOT EXISTS node_profiles (
    node_id BLOB NOT NULL PRIMARY KEY,
    value TEXT NOT NULL,
    profile_updated_time INTEGER NOT NULL,
    weight INTEGER NOT NULL,
    created_time TIMESTAMP NOT NULL,
    updated_time TIMESTAMP NOT NULL
);
"#
                .to_string(),
            },
            MigrationRequest {
                name: "2026-10-29_last_seen".to_string(),
                queries: r#"
CREATE TABLE node_profiles_next (
    node_id BLOB NOT NULL PRIMARY KEY,
    value TEXT NOT NULL,
    profile_updated_time INTEGER NOT NULL,
    last_seen INTEGER NOT NULL,
    fail_count INTEGER NOT NULL,
    last_attempt INTEGER NOT NULL,
    created_time TIMESTAMP NOT NULL,
    updated_time TIMESTAMP NOT NULL
);
INSERT INTO node_profiles_next (node_id, value, profile_updated_time, last_seen, fail_count, last_attempt, created_time, updated_time)
    SELECT node_id, value, profile_updated_time, CAST(strftime('%s', updated_time) AS INTEGER), 0, 0, created_time, updated_time FROM node_profiles;
DROP TABLE node_profiles;
ALTER TABLE node_profiles_next RENAME TO node_profiles;
"#
                .to_string(),
            },
        ];

        SqliteMigrator::migrate(db, requests).await?;

        Ok(())
    }

    /// The known nodes, ordered by when they were last seen.
    ///
    /// The parsed profiles are kept until the next write, because every cycle reads them and verifying the signature again each time costs more than the query.
    pub async fn fetch_node_profiles(&self) -> Result<Vec<KnownNode>> {
        if let Some(cache) = self.cache.lock().as_ref() {
            return Ok(cache.clone());
        }

        let res: Vec<(String, i64, i64, i64)> = sqlx::query_as(
            r#"
SELECT value, last_seen, fail_count, last_attempt
    FROM node_profiles
    ORDER BY last_seen DESC, updated_time DESC
"#,
        )
        .fetch_all(self.db.as_ref())
        .await?;

        let known: Vec<KnownNode> = res
            .into_iter()
            .filter_map(|(value, last_seen, fail_count, last_attempt)| match NodeProfile::from_str(value.as_str()) {
                Ok(profile) => Some(KnownNode {
                    profile,
                    last_seen,
                    fail_count,
                    last_attempt,
                }),
                Err(error) => {
                    warn!(error_message = error.to_string(), "skipping invalid stored node profile; keeping database row");
                    None
                }
            })
            .collect();

        *self.cache.lock() = Some(known.clone());

        Ok(known)
    }

    /// node ID ごとに 1 行だけ保持し、`NodeProfile::updated_time` が新しいものだけで置き換える。
    /// 受信しただけでは到達可能と分からないため、`last_seen` と `fail_count` は変えない（新規行は未到達の 0）
    pub async fn insert_or_update_node_profiles(&self, items: &[&NodeProfile]) -> Result<()> {
        const CHUNK_SIZE: i64 = 100;

        for chunk in items.chunks(CHUNK_SIZE as usize) {
            let mut query_builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
                r#"
INSERT INTO node_profiles (node_id, value, profile_updated_time, last_seen, fail_count, last_attempt, created_time, updated_time)
"#,
            );

            let now = self.clock.now().naive_utc();
            let rows: Vec<(Vec<u8>, String, i64)> = chunk.iter().map(|v| Ok((v.id().to_vec(), v.to_uri()?, v.updated_time()))).collect::<Result<_>>()?;

            query_builder.push_values(rows, |mut b, (node_id, value, profile_updated_time)| {
                b.push_bind(node_id);
                b.push_bind(value);
                b.push_bind(profile_updated_time);
                // 受信しただけでは到達可能と分からないので、未到達として 0 で入れる
                b.push_bind(0);
                b.push_bind(0);
                b.push_bind(0);
                b.push_bind(now);
                b.push_bind(now);
            });
            query_builder.push(
                r#"
ON CONFLICT (node_id) DO UPDATE SET
    value = excluded.value,
    profile_updated_time = excluded.profile_updated_time,
    updated_time = excluded.updated_time
    WHERE excluded.profile_updated_time > node_profiles.profile_updated_time
"#,
            );
            query_builder.build().execute(self.db.as_ref()).await?;
        }

        self.cache.lock().take();

        Ok(())
    }

    /// Records that a node was reached, which clears its failure count.
    pub async fn mark_reachable(&self, node_id: &[u8]) -> Result<()> {
        let now = self.clock.now().timestamp();
        sqlx::query(
            r#"
UPDATE node_profiles
    SET last_seen = ?, fail_count = 0, last_attempt = ?
    WHERE node_id = ?
"#,
        )
        .bind(now)
        .bind(now)
        .bind(node_id)
        .execute(self.db.as_ref())
        .await?;

        self.cache.lock().take();

        Ok(())
    }

    /// Records that a dial to a node failed, which lengthens the wait before the next one.
    pub async fn mark_unreachable(&self, node_id: &[u8]) -> Result<()> {
        let now = self.clock.now().timestamp();
        sqlx::query(
            r#"
UPDATE node_profiles
    SET fail_count = fail_count + 1, last_attempt = ?
    WHERE node_id = ?
"#,
        )
        .bind(now)
        .bind(node_id)
        .execute(self.db.as_ref())
        .await?;

        self.cache.lock().take();

        Ok(())
    }

    /// Deletes the nodes seen longest ago until at most `limit` rows remain.
    pub async fn shrink(&self, limit: usize) -> Result<()> {
        let total: i64 = sqlx::query_scalar(
            r#"
SELECT COUNT(1)
    FROM node_profiles
"#,
        )
        .fetch_one(self.db.as_ref())
        .await?;

        let count_to_delete = total - limit as i64;

        if count_to_delete > 0 {
            sqlx::query(
                r#"
DELETE FROM node_profiles
    WHERE rowid IN (
        SELECT rowid FROM node_profiles
        ORDER BY last_seen ASC, rowid ASC
        LIMIT ?
    )
"#,
            )
            .bind(count_to_delete)
            .execute(self.db.as_ref())
            .await?;

            self.cache.lock().take();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::DateTime;
    use testresult::TestResult;

    use omnius_core_base::clock::{Clock as _, FakeClockUtc};
    use omnius_core_omnikit::{
        generated::omni_sign::{OmniSignType, OmniSigner},
        model::omni_addr::OmniAddr,
    };

    use crate::{
        model::NodeProfile,
        protocol::{MessageCodec as _, NodeProfileCodec},
    };

    use super::NodeFinderRepo;

    #[tokio::test]
    async fn connections_use_durable_pragmas() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock).await?;
        // 同時に確保して、pool が新しく開く connection も確認する。
        let mut connections = Vec::new();
        for _ in 0..3 {
            connections.push(repo.db.acquire().await?);
        }
        for connection in &mut connections {
            let mode: String = sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&mut **connection).await?;
            let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous").fetch_one(&mut **connection).await?;
            assert_eq!(mode, "wal");
            assert_eq!(synchronous, 2);
            #[cfg(target_os = "macos")]
            {
                let fullfsync: i64 = sqlx::query_scalar("PRAGMA fullfsync").fetch_one(&mut **connection).await?;
                assert_eq!(fullfsync, 1);
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn invalid_stored_profiles_are_skipped_without_deleting_rows() -> TestResult {
        use crate::{generated::axus::model as wire, protocol::uri::UriConverter};
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock).await?;
        let valid = NodeProfile::random_for_test(vec![OmniAddr::new("addr")]);
        repo.insert_or_update_node_profiles(&[&valid]).await?;

        let unsigned = UriConverter::encode(
            "node",
            &wire::NodeProfileSigningPayload {
                public_key: vec![2; 32],
                addrs: vec![],
                updated_time: 0,
            },
        )?;
        let forged = UriConverter::encode(
            "node",
            &wire::NodeProfile {
                signature: vec![0; 64],
                ..NodeProfileCodec::to_wire(&NodeProfile::random_for_test(vec![]))
            },
        )?;
        for (node_id, uri) in [(vec![2u8], unsigned), (vec![3u8], forged)] {
            sqlx::query(
                "INSERT INTO node_profiles (node_id, value, profile_updated_time, last_seen, fail_count, last_attempt, created_time, updated_time) VALUES (?, ?, 0, 0, 0, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
            )
            .bind(node_id)
            .bind(uri)
            .execute(repo.db.as_ref())
            .await?;
        }
        assert_eq!(repo.fetch_node_profiles().await?.into_iter().map(|known| known.profile).collect::<Vec<_>>(), vec![valid]);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_profiles").fetch_one(repo.db.as_ref()).await?;
        assert_eq!(count, 3);
        Ok(())
    }

    #[tokio::test]
    async fn newer_profile_replaces_the_stored_profile_of_the_same_node() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock).await?;
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?;
        let at = |seconds: i64| DateTime::from_timestamp(seconds, 0).unwrap();
        let first = NodeProfile::sign(&signer, vec![OmniAddr::new("old")], at(100))?;
        let second = NodeProfile::sign(&signer, vec![OmniAddr::new("new")], at(200))?;
        let other = NodeProfile::random_for_test(vec![OmniAddr::new("other")]);

        repo.insert_or_update_node_profiles(&[&first, &other]).await?;
        repo.mark_reachable(second.id()).await?;
        repo.insert_or_update_node_profiles(&[&second]).await?;

        let stored: Vec<NodeProfile> = repo.fetch_node_profiles().await?.into_iter().map(|known| known.profile).collect();
        assert_eq!(stored.len(), 2);
        assert!(stored.contains(&second));
        assert!(stored.contains(&other));
        Ok(())
    }

    #[tokio::test]
    async fn older_or_equal_profile_does_not_replace_the_stored_profile() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock).await?;
        let signer = OmniSigner::new(OmniSignType::Ed25519_Sha3_256_Base64Url, "test")?;
        let at = |seconds: i64| DateTime::from_timestamp(seconds, 0).unwrap();
        let current = NodeProfile::sign(&signer, vec![OmniAddr::new("current")], at(200))?;
        let older = NodeProfile::sign(&signer, vec![OmniAddr::new("older")], at(100))?;
        let same_time = NodeProfile::sign(&signer, vec![OmniAddr::new("same-time")], at(200))?;

        repo.insert_or_update_node_profiles(&[&current]).await?;
        repo.insert_or_update_node_profiles(&[&older, &same_time]).await?;

        assert_eq!(repo.fetch_node_profiles().await?.into_iter().map(|known| known.profile).collect::<Vec<_>>(), vec![current]);
        Ok(())
    }

    #[tokio::test]
    async fn the_entry_seen_longest_ago_is_evicted_first() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock.clone()).await?;
        let profiles: Vec<NodeProfile> = (0..3).map(|_| NodeProfile::random_for_test(vec![OmniAddr::new("test")])).collect();
        for profile in &profiles {
            repo.insert_or_update_node_profiles(&[profile]).await?;
            clock.advance(std::time::Duration::from_secs(10));
        }

        // 2 番目の node だけ、最後に到達できた時刻を新しくする
        repo.mark_reachable(profiles[1].id()).await?;
        repo.shrink(2).await?;

        let stored: Vec<NodeProfile> = repo.fetch_node_profiles().await?.into_iter().map(|known| known.profile).collect();
        assert_eq!(stored.len(), 2);
        assert!(!stored.contains(&profiles[0]), "the node seen longest ago is evicted first");
        Ok(())
    }

    #[tokio::test]
    async fn a_profile_seen_only_through_gossip_is_evicted_before_a_reached_one() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock.clone()).await?;

        // 先に到達済みの node、後から gossip だけで知った node
        let reached = NodeProfile::random_for_test(vec![OmniAddr::new("reached")]);
        repo.insert_or_update_node_profiles(&[&reached]).await?;
        repo.mark_reachable(reached.id()).await?;
        clock.advance(std::time::Duration::from_secs(10));
        let gossiped = NodeProfile::random_for_test(vec![OmniAddr::new("gossiped")]);
        repo.insert_or_update_node_profiles(&[&gossiped]).await?;

        let stored = repo.fetch_node_profiles().await?;
        assert_eq!(stored[0].profile, reached, "a reached node ranks above a gossiped one");
        assert_eq!(stored[0].last_seen, clock.now().timestamp() - 10);
        assert_eq!(stored[1].last_seen, 0);

        repo.shrink(1).await?;
        let stored: Vec<NodeProfile> = repo.fetch_node_profiles().await?.into_iter().map(|known| known.profile).collect();
        assert_eq!(stored, vec![reached]);
        Ok(())
    }

    #[tokio::test]
    async fn a_failed_dial_delays_the_next_attempt() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock.clone()).await?;
        let profile = NodeProfile::random_for_test(vec![OmniAddr::new("test")]);
        repo.insert_or_update_node_profiles(&[&profile]).await?;

        let known = repo.fetch_node_profiles().await?.remove(0);
        assert!(known.is_due(clock.now()));

        repo.mark_unreachable(profile.id()).await?;
        let known = repo.fetch_node_profiles().await?.remove(0);
        assert!(!known.is_due(clock.now()), "the first failure waits before the next dial");

        clock.advance(super::KnownNode::MIN_RETRY);
        let known = repo.fetch_node_profiles().await?.remove(0);
        assert!(known.is_due(clock.now()));

        repo.mark_unreachable(profile.id()).await?;
        clock.advance(super::KnownNode::MIN_RETRY);
        let known = repo.fetch_node_profiles().await?.remove(0);
        assert!(!known.is_due(clock.now()), "the second failure waits twice as long");

        repo.mark_reachable(profile.id()).await?;
        let known = repo.fetch_node_profiles().await?.remove(0);
        assert!(known.is_due(clock.now()), "reaching the node clears the failure count");
        assert_eq!(known.fail_count, 0);
        Ok(())
    }

    #[tokio::test]
    pub async fn simple_test() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().as_os_str().to_str().unwrap();

        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap().into()));
        let repo = NodeFinderRepo::new(path, clock).await?;

        let vs: Vec<NodeProfile> = vec![
            NodeProfile::random_for_test(vec![OmniAddr::new("test")]),
            NodeProfile::random_for_test(vec![OmniAddr::new("test")]),
        ];
        let vs_ref: Vec<&NodeProfile> = vs.iter().collect();
        repo.insert_or_update_node_profiles(&vs_ref).await?;

        let res: Vec<NodeProfile> = repo.fetch_node_profiles().await?.into_iter().map(|known| known.profile).collect();
        assert_eq!(res, vs);

        repo.shrink(1).await?;
        let res: Vec<NodeProfile> = repo.fetch_node_profiles().await?.into_iter().map(|known| known.profile).collect();
        assert_eq!(res, vs.into_iter().skip(1).collect::<Vec<_>>());

        repo.shrink(0).await?;
        let res = repo.fetch_node_profiles().await?;
        assert!(res.is_empty());

        Ok(())
    }
}
