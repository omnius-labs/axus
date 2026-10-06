use std::{path::Path, str::FromStr as _, sync::Arc};

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{QueryBuilder, sqlite::SqlitePool};

use omnius_core_base::clock::Clock;
use omnius_core_migration::sqlite::{MigrationRequest, SqliteMigrator};
use omnius_core_omnikit::generated::omni_hash::OmniHash;

use super::*;

#[allow(unused)]
pub struct FileSubscriberRepo {
    db: Arc<SqlitePool>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

impl FileSubscriberRepo {
    pub async fn new<P: AsRef<Path>>(state_dir: P, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Self> {
        let path = state_dir.as_ref().join("sqlite.db");
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
        Self::migrate(&db).await?;

        Ok(Self { db, clock })
    }

    async fn migrate(db: &SqlitePool) -> Result<()> {
        let requests = vec![MigrationRequest {
            name: "2025-06-08_init".to_string(),
            queries: r#"
CREATE TABLE IF NOT EXISTS files (
    id TEXT NOT NULL PRIMARY KEY,
    root_hash TEXT NOT NULL,
    file_path TEXT NOT NULL,
    rank INTEGER NOT NULL,
    block_count_downloaded INTEGER NOT NULL,
    block_count_total INTEGER NOT NULL,
    attrs TEXT,
    priority INTEGER NOT NULL,
    status TEXT NOT NULL,
    failed_reason TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
CREATE TABLE IF NOT EXISTS blocks (
    root_hash TEXT NOT NULL,
    block_hash TEXT NOT NULL,
    rank INTEGER NOT NULL,
    `index` INTEGER NOT NULL,
    downloaded INTEGER NOT NULL,
    PRIMARY KEY (root_hash, block_hash, rank, `index`)
);
CREATE INDEX IF NOT EXISTS index_root_hash_rank_index_for_blocks ON blocks (root_hash, rank ASC, `index` ASC, downloaded);
"#
            .to_string(),
        }];

        SqliteMigrator::migrate(db, requests).await?;

        Ok(())
    }

    pub async fn get_downloading_root_hashes(&self) -> Result<Vec<OmniHash>> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT DISTINCT root_hash FROM files WHERE status = 'Downloading'")
            .fetch_all(self.db.as_ref())
            .await?;
        rows.into_iter().map(|(hash,)| OmniHash::from_str(&hash).map_err(Into::into)).collect()
    }

    pub async fn transition(&self, id: &str, expected: &SubscribedFileStatus, status: &SubscribedFileStatus, reason: Option<&str>) -> Result<bool> {
        Ok(sqlx::query("UPDATE files SET status = ?, failed_reason = ?, updated_at = ? WHERE id = ? AND status = ?")
            .bind(status)
            .bind(reason)
            .bind(self.clock.now().naive_utc())
            .bind(id)
            .bind(expected)
            .execute(self.db.as_ref())
            .await?
            .rows_affected()
            != 0)
    }

    pub async fn cancel(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE files SET status = 'Canceled', updated_at = ? WHERE id = ? AND status IN ('Downloading', 'Decoding')")
            .bind(self.clock.now().naive_utc())
            .bind(id)
            .execute(self.db.as_ref())
            .await?;
        Ok(())
    }

    pub async fn mark_downloaded(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<bool> {
        let mut tx = self.db.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("UPDATE blocks SET downloaded = 1 WHERE root_hash = ? AND block_hash = ?")
            .bind(root_hash.to_string())
            .bind(block_hash.to_string())
            .execute(&mut *tx)
            .await?;
        let changed = Self::refresh_progress(&mut tx, root_hash, self.clock.now().naive_utc()).await?;
        tx.commit().await?;
        Ok(changed)
    }

    async fn refresh_progress(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, root_hash: &OmniHash, now: NaiveDateTime) -> Result<bool> {
        sqlx::query("UPDATE files SET block_count_downloaded = (SELECT COUNT(*) FROM blocks WHERE blocks.root_hash = files.root_hash AND blocks.rank = files.rank AND downloaded = 1), updated_at = ? WHERE root_hash = ? AND status = 'Downloading'")
            .bind(now).bind(root_hash.to_string()).execute(&mut **tx).await?;
        Ok(
            sqlx::query("UPDATE files SET status = 'Decoding' WHERE root_hash = ? AND status = 'Downloading' AND block_count_downloaded = block_count_total")
                .bind(root_hash.to_string())
                .execute(&mut **tx)
                .await?
                .rows_affected()
                != 0,
        )
    }

    pub async fn advance_layer(&self, file: &SubscribedFile, blocks: &[SubscribedBlock]) -> Result<bool> {
        let mut tx = self.db.begin_with("BEGIN IMMEDIATE").await?;
        let changed = sqlx::query(
            "UPDATE files SET rank = ?, block_count_total = ?, block_count_downloaded = 0, status = 'Downloading', updated_at = ? WHERE id = ? AND status = 'Decoding'",
        )
        .bind(file.rank)
        .bind(file.block_count_total)
        .bind(self.clock.now().naive_utc())
        .bind(&file.id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            != 0;
        if !changed {
            return Ok(false);
        }
        for block in blocks {
            // Another subscription may have received this hash after Store checked the key.
            sqlx::query("INSERT INTO blocks (root_hash, block_hash, rank, `index`, downloaded) VALUES (?, ?, ?, ?, (? OR EXISTS (SELECT 1 FROM blocks WHERE root_hash = ? AND block_hash = ? AND downloaded = 1))) ON CONFLICT DO NOTHING")
                .bind(block.root_hash.to_string())
                .bind(block.block_hash.to_string())
                .bind(block.rank)
                .bind(block.index)
                .bind(block.downloaded)
                .bind(block.root_hash.to_string())
                .bind(block.block_hash.to_string())
                .execute(&mut *tx)
                .await?;
        }
        Self::refresh_progress(&mut tx, &file.root_hash, self.clock.now().naive_utc()).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn get_committed_files(&self) -> Result<Vec<SubscribedFile>> {
        let res: Vec<SubscribedFileRow> = sqlx::query_as(
            r#"
SELECT *
    FROM files
"#,
        )
        .fetch_all(self.db.as_ref())
        .await?;

        let res: Vec<SubscribedFile> = res.into_iter().filter_map(|r| r.into().ok()).collect();
        Ok(res)
    }

    pub async fn find_file_by_id(&self, id: &str) -> Result<Option<SubscribedFile>> {
        let res: Option<SubscribedFileRow> = sqlx::query_as(
            r#"
SELECT id, root_hash, file_path, rank, block_count_downloaded, block_count_total, attrs, priority, status, failed_reason, created_at, updated_at
    FROM files
    WHERE id = ?
"#,
        )
        .bind(id)
        .fetch_optional(self.db.as_ref())
        .await?;

        res.map(|r| r.into()).transpose()
    }

    pub async fn find_file_by_root_hash(&self, root_hash: &OmniHash) -> Result<Option<SubscribedFile>> {
        let res: Option<SubscribedFileRow> = sqlx::query_as(
            r#"
SELECT id, root_hash, file_path, rank, block_count_downloaded, block_count_total, attrs, priority, status, failed_reason, created_at, updated_at
    FROM files
    WHERE root_hash = ?
"#,
        )
        .bind(root_hash.to_string())
        .fetch_optional(self.db.as_ref())
        .await?;

        res.map(|r| r.into()).transpose()
    }

    pub async fn find_file_by_decoding_next(&self) -> Result<Option<SubscribedFile>> {
        let res: Option<SubscribedFileRow> = sqlx::query_as(
            r#"
SELECT id, root_hash, file_path, rank, block_count_downloaded, block_count_total, attrs, priority, status, failed_reason, created_at, updated_at
    FROM files
    WHERE status = 'Decoding'
    ORDER BY priority ASC, created_at ASC
    LIMIT 1
"#,
        )
        .fetch_optional(self.db.as_ref())
        .await?;

        res.map(|r| r.into()).transpose()
    }

    pub async fn delete_file(&self, id: &str) -> Result<()> {
        let mut tx = self.db.begin_with("BEGIN EXCLUSIVE").await?;

        let res: Option<SubscribedFileRow> = sqlx::query_as(
            r#"
SELECT *
    FROM files
    WHERE id = ?
"#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(res) = res else {
            return Err(Error::new(ErrorKind::NotFound).with_message(format!("{id} is not found")));
        };

        let file = res.into()?;

        sqlx::query(
            r#"
DELETE FROM files
    WHERE id = ?
"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        let (file_count,): (i64,) = sqlx::query_as(
            r#"
SELECT COUNT(1)
    FROM files
    WHERE root_hash = ?
    LIMIT 1
"#,
        )
        .bind(file.root_hash.to_string())
        .fetch_one(&mut *tx)
        .await?;

        if file_count == 0 {
            sqlx::query(
                r#"
DELETE FROM blocks
    WHERE root_hash = ?
"#,
            )
            .bind(file.root_hash.to_string())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    pub async fn find_blocks_by_root_hash_and_block_hash(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<Vec<SubscribedBlock>> {
        let res: Vec<SubscribedBlockRow> = sqlx::query_as(
            r#"
SELECT *
    FROM blocks
    WHERE root_hash = ? AND block_hash = ?
"#,
        )
        .bind(root_hash.to_string())
        .bind(block_hash.to_string())
        .fetch_all(self.db.as_ref())
        .await?;

        let res: Vec<SubscribedBlock> = res.into_iter().filter_map(|r| r.into().ok()).collect();

        Ok(res)
    }

    pub async fn find_blocks_by_root_hash_and_rank(&self, root_hash: &OmniHash, rank: u32) -> Result<Vec<SubscribedBlock>> {
        let res: Vec<SubscribedBlockRow> = sqlx::query_as(
            r#"
SELECT *
    FROM blocks
    WHERE root_hash = ? AND rank = ?
    ORDER BY `index` ASC
"#,
        )
        .bind(root_hash.to_string())
        .bind(rank)
        .fetch_all(self.db.as_ref())
        .await?;

        let res: Vec<SubscribedBlock> = res.into_iter().filter_map(|r| r.into().ok()).collect();

        Ok(res)
    }

    pub async fn insert_file_and_blocks(&self, file: &SubscribedFile, blocks: &[SubscribedBlock]) -> Result<()> {
        let mut tx = self.db.begin().await?;

        let row = SubscribedFileRow::from(file)?;
        sqlx::query(
            r#"
INSERT INTO files (id, root_hash, file_path, rank, block_count_downloaded, block_count_total, attrs, priority, status, failed_reason, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
        )
        .bind(row.id)
        .bind(row.root_hash)
        .bind(row.file_path)
        .bind(row.rank)
        .bind(row.block_count_downloaded)
        .bind(row.block_count_total)
        .bind(row.attrs)
        .bind(row.priority)
        .bind(row.status)
        .bind(row.failed_reason)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&mut *tx)
        .await?;

        const CHUNK_SIZE: i64 = 100;

        for chunk in blocks.chunks(CHUNK_SIZE as usize) {
            let mut query_builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
                r#"
INSERT INTO blocks (root_hash, block_hash, rank, `index`, downloaded)
"#,
            );

            let rows: Vec<SubscribedBlockRow> = chunk.iter().filter_map(|item| SubscribedBlockRow::from(item).ok()).collect();

            query_builder.push_values(rows, |mut b, row| {
                b.push_bind(row.root_hash);
                b.push_bind(row.block_hash);
                b.push_bind(row.rank);
                b.push_bind(row.index);
                b.push_bind(row.downloaded);
            });
            query_builder.push(
                r#"
    ON CONFLICT DO NOTHING
"#,
            );
            query_builder.build().execute(&mut *tx).await?;
        }

        Self::refresh_progress(&mut tx, &file.root_hash, self.clock.now().naive_utc()).await?;
        tx.commit().await?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct SubscribedFileRow {
    pub id: String,
    pub root_hash: String,
    pub file_path: String,
    pub rank: i64,
    pub block_count_downloaded: i64,
    pub block_count_total: i64,
    pub attrs: Option<String>,
    pub priority: i64,
    pub status: SubscribedFileStatus,
    pub failed_reason: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl SubscribedFileRow {
    pub fn into(self) -> Result<SubscribedFile> {
        Ok(SubscribedFile {
            id: self.id,
            root_hash: OmniHash::from_str(self.root_hash.as_str()).unwrap(),
            file_path: self.file_path,
            rank: self.rank as u32,
            block_count_downloaded: self.block_count_downloaded as u32,
            block_count_total: self.block_count_total as u32,
            attrs: self.attrs,
            priority: self.priority,
            status: self.status,
            failed_reason: self.failed_reason,
            created_at: DateTime::from_naive_utc_and_offset(self.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(self.updated_at, Utc),
        })
    }

    #[allow(unused)]
    pub fn from(item: &SubscribedFile) -> Result<Self> {
        Ok(Self {
            id: item.id.to_string(),
            root_hash: item.root_hash.to_string(),
            file_path: item.file_path.clone(),
            rank: item.rank as i64,
            block_count_downloaded: item.block_count_downloaded as i64,
            block_count_total: item.block_count_total as i64,
            attrs: item.attrs.as_ref().map(|n| n.to_string()),
            priority: item.priority,
            status: item.status.clone(),
            failed_reason: item.failed_reason.clone(),
            created_at: item.created_at.naive_utc(),
            updated_at: item.updated_at.naive_utc(),
        })
    }
}

#[derive(sqlx::FromRow)]
pub struct SubscribedBlockRow {
    pub root_hash: String,
    pub block_hash: String,
    pub rank: u32,
    pub index: u32,
    pub downloaded: bool,
}

impl SubscribedBlockRow {
    pub fn into(self) -> Result<SubscribedBlock> {
        Ok(SubscribedBlock {
            root_hash: OmniHash::from_str(self.root_hash.as_str()).unwrap(),
            block_hash: OmniHash::from_str(self.block_hash.as_str()).unwrap(),
            rank: self.rank,
            index: self.index,
            downloaded: self.downloaded,
        })
    }

    #[allow(unused)]
    pub fn from(item: &SubscribedBlock) -> Result<Self> {
        Ok(Self {
            root_hash: item.root_hash.to_string(),
            block_hash: item.block_hash.to_string(),
            rank: item.rank,
            index: item.index,
            downloaded: item.downloaded,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{DateTime, Utc};
    use testresult::TestResult;

    use omnius_core_base::clock::{Clock, FakeClockUtc};
    use omnius_core_omnikit::generated::omni_hash::{OmniHash, OmniHashAlgorithmType};

    use crate::core::negotiator::file::model::{SubscribedBlock, SubscribedFile, SubscribedFileStatus};

    use super::FileSubscriberRepo;

    #[tokio::test]
    async fn connections_use_durable_pragmas() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, _) = create_repo(dir.path()).await?;
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
    async fn file_queries_round_trip() -> TestResult {
        let dir = tempfile::tempdir()?;
        let (repo, now) = create_repo(dir.path()).await?;
        let root_hash = hash(b"root");

        let file = subscribed_file("1", &root_hash, now);
        repo.insert_file_and_blocks(&file, &[]).await?;
        let found = repo.find_file_by_id("1").await?.unwrap();
        assert_eq!(found.rank, 2);
        assert_eq!(found.block_count_downloaded, 0);
        assert_eq!(found.block_count_total, 3);
        assert_eq!(found.priority, 5);
        assert_eq!(repo.find_file_by_root_hash(&root_hash).await?.unwrap().id, "1");
        assert_eq!(repo.get_committed_files().await?.len(), 1);
        assert!(repo.find_file_by_decoding_next().await?.is_none());

        repo.transition("1", &SubscribedFileStatus::Downloading, &SubscribedFileStatus::Decoding, None).await?;
        assert_eq!(repo.find_file_by_decoding_next().await?.unwrap().id, "1");

        let updated = SubscribedFile {
            rank: 1,
            block_count_downloaded: 0,
            block_count_total: 2,
            status: SubscribedFileStatus::Downloading,
            ..file
        };
        let blocks = vec![block(&root_hash, b"a", 1, 0), block(&root_hash, b"b", 1, 1)];
        repo.advance_layer(&updated, &blocks).await?;
        let found = repo.find_file_by_id("1").await?.unwrap();
        assert_eq!(found.rank, 1);
        assert_eq!(found.block_count_total, 2);
        assert!(found.status == SubscribedFileStatus::Downloading);
        assert_eq!(repo.find_blocks_by_root_hash_and_rank(&root_hash, 1).await?.len(), 2);

        let downloaded = SubscribedBlock {
            downloaded: true,
            ..block(&root_hash, b"a", 1, 0)
        };
        repo.mark_downloaded(&root_hash, &downloaded.block_hash).await?;
        let found = repo.find_blocks_by_root_hash_and_block_hash(&root_hash, &hash(b"a")).await?;
        assert_eq!(found.len(), 1);
        assert!(found[0].downloaded);

        repo.delete_file("1").await?;
        assert!(repo.find_file_by_id("1").await?.is_none());
        assert!(repo.find_blocks_by_root_hash_and_rank(&root_hash, 1).await?.is_empty());

        Ok(())
    }

    async fn create_repo(dir: &std::path::Path) -> TestResult<(FileSubscriberRepo, DateTime<Utc>)> {
        let clock = Arc::new(FakeClockUtc::new(DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")?.into()));
        let now = clock.now();
        Ok((FileSubscriberRepo::new(dir, clock).await?, now))
    }

    fn subscribed_file(id: &str, root_hash: &OmniHash, now: DateTime<Utc>) -> SubscribedFile {
        SubscribedFile {
            id: id.to_string(),
            root_hash: root_hash.clone(),
            file_path: "/tmp/a.txt".to_string(),
            rank: 2,
            block_count_downloaded: 1,
            block_count_total: 3,
            attrs: None,
            priority: 5,
            status: SubscribedFileStatus::Downloading,
            failed_reason: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn block(root_hash: &OmniHash, value: &[u8], rank: u32, index: u32) -> SubscribedBlock {
        SubscribedBlock {
            root_hash: root_hash.clone(),
            block_hash: hash(value),
            rank,
            index,
            downloaded: false,
        }
    }

    fn hash(value: &[u8]) -> OmniHash {
        OmniHash::compute_hash(OmniHashAlgorithmType::Sha3_256, value)
    }
}
