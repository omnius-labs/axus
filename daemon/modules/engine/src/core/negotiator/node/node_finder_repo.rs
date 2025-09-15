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
        let path = path
            .to_str()
            .ok_or_else(|| Error::builder().kind(ErrorKind::UnexpectedError).message("Invalid path").build())?;

        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(10));

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

        let res: Vec<NodeProfile> = res.into_iter().filter_map(|(v,)| NodeProfile::from_str(v.as_str()).ok()).collect();
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
            let rows: Vec<String> = chunk.iter().map(|v| v.to_string()).collect();

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
    use omnius_core_omnikit::model::OmniAddr;

    use crate::model::NodeProfile;

    use super::NodeFinderRepo;

    #[tokio::test]
    pub async fn simple_test() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().as_os_str().to_str().unwrap();

        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z").unwrap().into()));
        let repo = NodeFinderRepo::new(path, clock).await?;

        let vs: Vec<NodeProfile> = vec![
            NodeProfile {
                id: vec![0],
                addrs: vec![OmniAddr::new("test")],
            },
            NodeProfile {
                id: vec![1],
                addrs: vec![OmniAddr::new("test")],
            },
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
