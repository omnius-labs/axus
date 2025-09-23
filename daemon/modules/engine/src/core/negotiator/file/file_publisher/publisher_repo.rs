use std::{path::Path, str::FromStr as _, sync::Arc};

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{QueryBuilder, sqlite::SqlitePool};

use omnius_core_base::clock::Clock;
use omnius_core_migration::sqlite::{MigrationRequest, SqliteMigrator};
use omnius_core_omnikit::model::OmniHash;

use crate::{core::negotiator::file::model::PublishedUncommittedFileStatus, prelude::*};

use super::*;

#[allow(unused)]
pub struct FilePublisherRepo {
    db: Arc<SqlitePool>,
    clock: Arc<dyn Clock<Utc> + Send + Sync>,
}

#[allow(unused)]
impl FilePublisherRepo {
    pub async fn new<P: AsRef<Path>>(state_dir: P, clock: Arc<dyn Clock<Utc> + Send + Sync>) -> Result<Self> {
        let path = state_dir.as_ref().join("sqlite.db");
        let path = path
            .to_str()
            .ok_or_else(|| Error::new(ErrorKind::UnexpectedError).with_message("Invalid path"))?;

        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(10));

        let db = Arc::new(SqlitePool::connect_with(options).await?);
        Self::migrate(&db).await?;

        Ok(Self { db, clock })
    }

    async fn migrate(db: &SqlitePool) -> Result<()> {
        let requests = vec![MigrationRequest {
            name: "2024-06-23_init".to_string(),
            queries: r#"
-- committed
CREATE TABLE IF NOT EXISTS committed_files (
    root_hash TEXT NOT NULL,
    file_name TEXT NOT NULL,
    block_size INTEGER NOT NULL,
    attrs TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    PRIMARY KEY (root_hash, file_name)
);
CREATE TABLE IF NOT EXISTS committed_blocks (
    root_hash TEXT NOT NULL,
    block_hash TEXT NOT NULL,
    rank INTEGER NOT NULL,
    `index` INTEGER NOT NULL,
    PRIMARY KEY (root_hash, block_hash, rank, `index`)
);
CREATE INDEX IF NOT EXISTS index_root_hash_rank_index_for_committed_blocks ON committed_blocks (root_hash, rank ASC, `index` ASC);

-- uncommitted
CREATE TABLE IF NOT EXISTS uncommitted_files (
    id TEXT NOT NULL PRIMARY KEY,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    block_size INTEGER NOT NULL,
    attrs TEXT,
    priority INTEGER NOT NULL,
    status TEXT NOT NULL,
    failed_reason TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS unique_index_file_path_file_name_for_uncommitted_files ON uncommitted_files (file_path, file_name);
CREATE TABLE IF NOT EXISTS uncommitted_blocks (
    file_id TEXT NOT NULL,
    block_hash TEXT NOT NULL,
    rank INTEGER NOT NULL,
    `index` INTEGER NOT NULL,
    PRIMARY KEY (root_hash, block_hash, rank, `index`)
);
CREATE INDEX IF NOT EXISTS index_root_hash_rank_index_for_committed_blocks ON committed_blocks (root_hash, rank ASC, `index` ASC);
"#
            .to_string(),
        }];

        SqliteMigrator::migrate(db, requests).await?;

        Ok(())
    }

    pub async fn contains_committed_file(&self, root_hash: &OmniHash) -> Result<bool> {
        let (res,): (i64,) = sqlx::query_as(
            r#"
SELECT COUNT(1)
    FROM committed_files
    WHERE root_hash = ?
    LIMIT 1
"#,
        )
        .bind(root_hash.to_string())
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(res > 0)
    }

    pub async fn get_committed_files(&self) -> Result<Vec<PublishedCommittedFile>> {
        let res: Vec<PublishedCommittedFileRow> = sqlx::query_as(
            r#"
SELECT root_hash, file_name, block_size, property, created_at, updated_at
    FROM committed_files
"#,
        )
        .fetch_all(self.db.as_ref())
        .await?;

        let res: Vec<PublishedCommittedFile> = res.into_iter().filter_map(|r| r.into().ok()).collect();
        Ok(res)
    }

    pub async fn find_committed_file_by_root_hash(&self, root_hash: &OmniHash) -> Result<Option<PublishedCommittedFile>> {
        let res: Option<PublishedCommittedFileRow> = sqlx::query_as(
            r#"
SELECT root_hash, file_name, block_size, property, created_at, updated_at
    FROM committed_files
    WHERE root_hash = ?
"#,
        )
        .bind(root_hash.to_string())
        .fetch_optional(self.db.as_ref())
        .await?;

        res.map(|r| r.into()).transpose()
    }

    pub async fn insert_committed_file(&self, item: &PublishedCommittedFile) -> Result<()> {
        let row = PublishedCommittedFileRow::from(item)?;
        sqlx::query(
            r#"
INSERT INTO committed_files (root_hash, file_name, block_size, attrs, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?)
"#,
        )
        .bind(row.root_hash)
        .bind(row.file_name)
        .bind(row.block_size)
        .bind(row.attrs)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(self.db.as_ref())
        .await?;

        Ok(())
    }

    pub async fn contains_committed_block(&self, root_hash: &OmniHash, block_hash: &OmniHash) -> Result<bool> {
        let (res,): (i64,) = sqlx::query_as(
            r#"
SELECT COUNT(1)
    FROM committed_blocks
    WHERE root_hash = ? AND block_hash = ?
    LIMIT 1
"#,
        )
        .bind(root_hash.to_string())
        .bind(block_hash.to_string())
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(res > 0)
    }

    pub async fn commit_file_with_blocks(&self, file: &PublishedCommittedFile, blocks: &[PublishedCommittedBlock], uncommitted_file_id: &str) -> Result<()> {
        let mut tx = self.db.begin().await?;

        let row = PublishedCommittedFileRow::from(file)?;
        sqlx::query(
            r#"
INSERT INTO committed_files (root_hash, file_name, block_size, attrs, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?)
"#,
        )
        .bind(row.root_hash)
        .bind(row.file_name)
        .bind(row.block_size)
        .bind(row.attrs)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&mut *tx)
        .await?;

        const CHUNK_SIZE: i64 = 100;

        for chunk in blocks.chunks(CHUNK_SIZE as usize) {
            let mut query_builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
                r#"
INSERT OR IGNORE INTO committed_blocks (root_hash, block_hash, rank, `index`)
"#,
            );

            let rows: Vec<PublishedCommittedBlockRow> = chunk.iter().filter_map(|item| PublishedCommittedBlockRow::from(item).ok()).collect();

            query_builder.push_values(rows, |mut b, row| {
                b.push_bind(row.root_hash);
                b.push_bind(row.block_hash);
                b.push_bind(row.rank);
                b.push_bind(row.index);
            });
            query_builder.build().execute(&mut *tx).await?;
        }

        sqlx::query(
            r#"
DELETE FROM uncommitted_files
    WHERE id = ?
"#,
        )
        .bind(uncommitted_file_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
DELETE FROM uncommitted_blocks
    WHERE file_id = ?
"#,
        )
        .bind(uncommitted_file_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn commit_file_without_blocks(&self, file: &PublishedCommittedFile, uncommitted_file_id: &str) -> Result<()> {
        let mut tx = self.db.begin().await?;

        let row = PublishedCommittedFileRow::from(file)?;
        sqlx::query(
            r#"
INSERT INTO committed_files (root_hash, file_name, block_size, attrs, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?)
"#,
        )
        .bind(row.root_hash)
        .bind(row.file_name)
        .bind(row.block_size)
        .bind(row.attrs)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
DELETE FROM uncommitted_files
    WHERE id = ?
"#,
        )
        .bind(uncommitted_file_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
DELETE FROM uncommitted_blocks
    WHERE file_id = ?
"#,
        )
        .bind(uncommitted_file_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn contains_uncommitted_file(&self, id: &str) -> Result<bool> {
        let (res,): (i64,) = sqlx::query_as(
            r#"
SELECT COUNT(1)
    FROM uncommitted_files
    WHERE id = ?
    LIMIT 1
"#,
        )
        .bind(id)
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(res > 0)
    }

    pub async fn get_uncommitted_files(&self) -> Result<Vec<PublishedUncommittedFile>> {
        let res: Vec<PublishedUncommittedFileRow> = sqlx::query_as(
            r#"
SELECT id, file_name, block_size, property, created_at, updated_at
    FROM uncommitted_files
"#,
        )
        .fetch_all(self.db.as_ref())
        .await?;

        let res: Vec<PublishedUncommittedFile> = res.into_iter().filter_map(|r| r.into().ok()).collect();
        Ok(res)
    }

    pub async fn find_uncommitted_file_by_encoding_next(&self) -> Result<Option<PublishedUncommittedFile>> {
        let res: Option<PublishedUncommittedFileRow> = sqlx::query_as(
            r#"
SELECT id, file_path, file_name, block_size, attrs, priority, created_at, updated_at
    FROM uncommitted_files
    ORDER BY priority ASC, created_at ASC
    LIMIT 1
"#,
        )
        .fetch_optional(self.db.as_ref())
        .await?;

        res.map(|r| r.into()).transpose()
    }

    pub async fn find_uncommitted_file_by_id(&self, id: &str) -> Result<Option<PublishedUncommittedFile>> {
        let res: Option<PublishedUncommittedFileRow> = sqlx::query_as(
            r#"
SELECT id, file_name, block_size, property, created_at, updated_at
    FROM uncommitted_files
    WHERE id = ?
"#,
        )
        .bind(id)
        .fetch_optional(self.db.as_ref())
        .await?;

        res.map(|r| r.into()).transpose()
    }

    pub async fn insert_uncommitted_file(&self, item: &PublishedUncommittedFile) -> Result<()> {
        let row = PublishedUncommittedFileRow::from(item)?;
        sqlx::query(
            r#"
INSERT INTO uncommitted_files (id, file_path, file_name, block_size, attrs, priority, status, failed_reason, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?)
"#,
        )
        .bind(row.id)
        .bind(row.file_path)
        .bind(row.file_name)
        .bind(row.block_size)
        .bind(row.attrs)
        .bind(row.priority)
        .bind(row.status)
        .bind(row.failed_reason)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(self.db.as_ref())
        .await?;

        Ok(())
    }

    pub async fn update_uncommitted_file_status(&self, id: &str, status: &PublishedUncommittedFileStatus) -> Result<()> {
        sqlx::query(
            r#"
UPDATE uncommitted_files
    SET status = ?
    WHERE id = ?
"#,
        )
        .bind(status)
        .bind(id)
        .execute(self.db.as_ref())
        .await?;

        Ok(())
    }

    pub async fn delete_uncommitted_file(&self, id: &str) -> Result<()> {
        let mut tx = self.db.begin().await?;

        sqlx::query(
            r#"
DELETE FROM uncommitted_files
    WHERE id = ?
"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
DELETE FROM uncommitted_blocks
    WHERE file_id = ?
"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn contains_uncommitted_block(&self, file_id: &str, block_hash: &OmniHash) -> Result<bool> {
        let (res,): (i64,) = sqlx::query_as(
            r#"
SELECT COUNT(1)
    FROM uncommitted_blocks
    WHERE file_id = ? AND block_hash = ?
    LIMIT 1
"#,
        )
        .bind(file_id)
        .bind(block_hash.to_string())
        .fetch_one(self.db.as_ref())
        .await?;

        Ok(res > 0)
    }

    pub async fn find_uncommitted_blocks_by_file_id(&self, file_id: &str) -> Result<Vec<PublishedUncommittedBlock>> {
        let res: Vec<PublishedUncommittedBlockRow> = sqlx::query_as(
            r#"
SELECT file_id, block_hash, rank, `index`
    FROM uncommitted_blocks
    WHERE file_id = ?
"#,
        )
        .bind(file_id)
        .fetch_all(self.db.as_ref())
        .await?;

        let res: Vec<PublishedUncommittedBlock> = res.into_iter().filter_map(|r| r.into().ok()).collect();
        Ok(res)
    }

    pub async fn insert_or_ignore_uncommitted_block(&self, item: &PublishedUncommittedBlock) -> Result<()> {
        let row = PublishedUncommittedBlockRow::from(item)?;
        sqlx::query(
            r#"
INSERT OR IGNORE INTO uncommitted_blocks (file_id, block_hash, rank, `index`)
    VALUES (?, ?, ?, ?)
"#,
        )
        .bind(row.file_id.to_string())
        .bind(row.block_hash.to_string())
        .bind(row.rank)
        .bind(row.index)
        .execute(self.db.as_ref())
        .await?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct PublishedCommittedFileRow {
    root_hash: String,
    file_name: String,
    block_size: u32,
    attrs: Option<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
}

impl PublishedCommittedFileRow {
    pub fn into(self) -> Result<PublishedCommittedFile> {
        Ok(PublishedCommittedFile {
            root_hash: OmniHash::from_str(self.root_hash.as_str()).unwrap(),
            file_name: self.file_name,
            block_size: self.block_size,
            attrs: self.attrs,
            created_at: DateTime::from_naive_utc_and_offset(self.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(self.updated_at, Utc),
        })
    }

    #[allow(unused)]
    pub fn from(item: &PublishedCommittedFile) -> Result<Self> {
        Ok(Self {
            root_hash: item.root_hash.to_string(),
            file_name: item.file_name.to_string(),
            block_size: item.block_size,
            attrs: item.attrs.as_ref().map(|n| n.to_string()),
            created_at: item.created_at.naive_utc(),
            updated_at: item.updated_at.naive_utc(),
        })
    }
}

#[derive(sqlx::FromRow)]
pub struct PublishedCommittedBlockRow {
    pub root_hash: String,
    pub block_hash: String,
    pub rank: u32,
    pub index: u32,
}

impl PublishedCommittedBlockRow {
    #[allow(unused)]
    pub fn into(self) -> Result<PublishedCommittedBlock> {
        Ok(PublishedCommittedBlock {
            root_hash: OmniHash::from_str(self.root_hash.as_str()).unwrap(),
            block_hash: OmniHash::from_str(self.block_hash.as_str()).unwrap(),
            rank: self.rank,
            index: self.index,
        })
    }

    #[allow(unused)]
    pub fn from(item: &PublishedCommittedBlock) -> Result<Self> {
        Ok(Self {
            root_hash: item.root_hash.to_string(),
            block_hash: item.block_hash.to_string(),
            rank: item.rank,
            index: item.index,
        })
    }
}

#[derive(sqlx::FromRow)]
struct PublishedUncommittedFileRow {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub block_size: u32,
    pub attrs: Option<String>,
    pub priority: i64,
    pub status: PublishedUncommittedFileStatus,
    pub failed_reason: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl PublishedUncommittedFileRow {
    pub fn into(self) -> Result<PublishedUncommittedFile> {
        Ok(PublishedUncommittedFile {
            id: self.id,
            file_path: self.file_path,
            file_name: self.file_name,
            block_size: self.block_size,
            attrs: self.attrs,
            priority: self.priority,
            status: self.status,
            failed_reason: self.failed_reason,
            created_at: DateTime::from_naive_utc_and_offset(self.created_at, Utc),
            updated_at: DateTime::from_naive_utc_and_offset(self.updated_at, Utc),
        })
    }

    #[allow(unused)]
    pub fn from(item: &PublishedUncommittedFile) -> Result<Self> {
        Ok(Self {
            id: item.id.to_string(),
            file_path: item.file_path.to_string(),
            file_name: item.file_name.to_string(),
            block_size: item.block_size,
            attrs: item.attrs.as_ref().map(|n| n.to_string()),
            priority: item.priority,
            status: item.status.clone(),
            failed_reason: item.failed_reason.as_ref().map(|n| n.to_string()),
            created_at: item.created_at.naive_utc(),
            updated_at: item.updated_at.naive_utc(),
        })
    }
}

#[derive(sqlx::FromRow)]
pub struct PublishedUncommittedBlockRow {
    pub file_id: String,
    pub block_hash: String,
    pub rank: u32,
    pub index: u32,
}

impl PublishedUncommittedBlockRow {
    pub fn into(self) -> Result<PublishedUncommittedBlock> {
        Ok(PublishedUncommittedBlock {
            file_id: self.file_id,
            block_hash: OmniHash::from_str(self.block_hash.as_str()).unwrap(),
            rank: self.rank,
            index: self.index,
        })
    }

    #[allow(unused)]
    pub fn from(item: &PublishedUncommittedBlock) -> Result<Self> {
        Ok(Self {
            file_id: item.file_id.to_string(),
            block_hash: item.block_hash.to_string(),
            rank: item.rank,
            index: item.index,
        })
    }
}
