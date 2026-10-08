use std::{path::Path, str::FromStr, sync::Arc};

use chrono::Utc;
use sqlx::QueryBuilder;
use sqlx::sqlite::SqlitePool;

use omnius_core_base::clock::Clock;
use omnius_core_migration::sqlite::{MigrationRequest, SqliteMigrator};

use crate::{model::NodeProfile, prelude::*};

pub struct NodeFinderRepo {
    db: Arc<SqlitePool>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
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

        Ok(Self { db, clock })
    }

    async fn migrate(db: &SqlitePool) -> Result<()> {
        let requests = vec![MigrationRequest {
            name: "2024-03-19_init".to_string(),
            queries: r#"
CREATE TABLE IF NOT EXISTS node_profiles (
    value TEXT NOT NULL PRIMARY KEY,
    weight INTEGER NOT NULL,
    created_time TIMESTAMP NOT NULL,
    updated_time TIMESTAMP NOT NULL
);
"#
            .to_string(),
        }];

        SqliteMigrator::migrate(db, requests).await?;

        Ok(())
    }

    pub async fn fetch_node_profiles(&self) -> Result<Vec<NodeProfile>> {
        let res: Vec<(String,)> = sqlx::query_as(
            r#"
SELECT value
    FROM node_profiles
    ORDER BY weight DESC, updated_time DESC
"#,
        )
        .fetch_all(self.db.as_ref())
        .await?;

        let res: Vec<NodeProfile> = res
            .into_iter()
            .filter_map(|(v,)| match NodeProfile::from_str(v.as_str()) {
                Ok(profile) => Some(profile),
                Err(error) => {
                    warn!(error_message = error.to_string(), "skipping invalid stored node profile; keeping database row");
                    None
                }
            })
            .collect();
        Ok(res)
    }

    pub async fn insert_or_ignore_node_profiles(&self, items: &[&NodeProfile], weight: i64) -> Result<()> {
        const CHUNK_SIZE: i64 = 100;

        for chunk in items.chunks(CHUNK_SIZE as usize) {
            let mut query_builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
                r#"
INSERT OR IGNORE INTO node_profiles (value, weight, created_time, updated_time)
"#,
            );

            let now = self.clock.now().naive_utc();
            let rows: Vec<String> = chunk.iter().map(|v| v.to_uri()).collect::<Result<_>>()?;

            query_builder.push_values(rows, |mut b, row| {
                b.push_bind(row);
                b.push_bind(weight);
                b.push_bind(now);
                b.push_bind(now);
            });
            query_builder.build().execute(self.db.as_ref()).await?;
        }

        Ok(())
    }

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
        ORDER BY updated_time ASC, rowid ASC
        LIMIT ?
    )
"#,
            )
            .bind(count_to_delete)
            .execute(self.db.as_ref())
            .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::DateTime;
    use testresult::TestResult;

    use omnius_core_base::clock::FakeClockUtc;
    use omnius_core_omnikit::model::omni_addr::OmniAddr;

    use crate::model::NodeProfile;

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
        use crate::protocol::{tests::legacy, uri::UriConverter};
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock).await?;
        let valid = NodeProfile::new(vec![1], vec![OmniAddr::new("addr")]);
        repo.insert_or_ignore_node_profiles(&[&valid], 0).await?;
        let old = legacy::NodeProfile::new(vec![2; 257], vec![]);
        let uri = UriConverter::encode("node", &old)?;
        sqlx::query("INSERT INTO node_profiles (value, weight, created_time, updated_time) VALUES (?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)")
            .bind(uri)
            .execute(repo.db.as_ref())
            .await?;
        assert_eq!(repo.fetch_node_profiles().await?, vec![valid]);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_profiles").fetch_one(repo.db.as_ref()).await?;
        assert_eq!(count, 2);
        Ok(())
    }

    #[tokio::test]
    async fn oversized_profile_returns_an_error_without_changing_stored_profiles() -> TestResult {
        let dir = tempfile::tempdir()?;
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let repo = NodeFinderRepo::new(dir.path().to_str().unwrap(), clock).await?;
        let valid = NodeProfile::new(vec![1], vec![OmniAddr::new("addr"); NodeProfile::MAX_WIRE_ADDRS]);
        repo.insert_or_ignore_node_profiles(&[&valid], 1).await?;

        let oversized = NodeProfile::new(vec![2], vec![OmniAddr::new("addr"); NodeProfile::MAX_WIRE_ADDRS + 1]);
        assert!(repo.insert_or_ignore_node_profiles(&[&oversized], 1).await.is_err());
        assert_eq!(repo.fetch_node_profiles().await?, vec![valid]);
        Ok(())
    }

    #[tokio::test]
    pub async fn simple_test() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().as_os_str().to_str().unwrap();

        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap().into()));
        let repo = NodeFinderRepo::new(path, clock).await?;

        let vs: Vec<NodeProfile> = vec![
            NodeProfile::new(vec![0], vec![OmniAddr::new("test")]),
            NodeProfile::new(vec![1], vec![OmniAddr::new("test")]),
        ];
        let vs_ref: Vec<&NodeProfile> = vs.iter().collect();
        repo.insert_or_ignore_node_profiles(&vs_ref, 1).await?;

        let res = repo.fetch_node_profiles().await?;
        assert_eq!(res, vs);

        repo.shrink(1).await?;
        let res = repo.fetch_node_profiles().await?;
        assert_eq!(res, vs.into_iter().skip(1).collect::<Vec<_>>());

        repo.shrink(0).await?;
        let res = repo.fetch_node_profiles().await?;
        assert_eq!(res, vec![]);

        Ok(())
    }
}
