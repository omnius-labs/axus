use std::{path::Path, str::FromStr as _, sync::Arc};

use chrono::{DateTime, NaiveDateTime, Utc};
use sqlx::{QueryBuilder, sqlite::SqlitePool};

use omnius_core_base::clock::Clock;
use omnius_core_migration::sqlite::{MigrationRequest, SqliteMigrator};
use omnius_core_omnikit::model::OmniHash;

use crate::prelude::*;

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
            .busy_timeout(std::time::Duration::from_secs(10));

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
    depth INTEGER NOT NULL,
    block_count_downloaded INTEGER NOT NULL,
    block_count_total INTEGER NOT NULL,
    attrs TEXT,
    priority INTEGER NOT NULL,
    status TEXT NOT NULL,
    failed_reason TEXT,
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    PRIMARY KEY (root_hash, file_name)
);
CREATE TABLE IF NOT EXISTS blocks (
    root_hash TEXT NOT NULL,
    block_hash TEXT NOT NULL,
    rank INTEGER NOT NULL,
    `index` INTEGER NOT NULL,
    downloaded INTEGER NOT NULL,
    PRIMARY KEY (root_hash, block_hash, rank, `index`)
);
CREATE INDEX IF NOT EXISTS index_root_hash_rank_index_for_blocks ON blocks (root_hash, rank ASC, `index` ASC, is_downloaded);
"#
            .to_string(),
        }];

        SqliteMigrator::migrate(db, requests).await?;

        Ok(())
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
SELECT id, root_hash, file_path, depth, block_count_downloaded, block_count_total, attrs, property, status, failed_reason, created_at, updated_at
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
SELECT id, root_hash, file_path, depth, block_count_downloaded, block_count_total, attrs, property, status, failed_reason, created_at, updated_at
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
SELECT id, root_hash, file_path, depth, block_count_downloaded, block_count_total, attrs, property, status, failed_reason, created_at, updated_at
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

    pub async fn insert_file(&self, file: &SubscribedFile) -> Result<()> {
        let row = SubscribedFileRow::from(file)?;
        sqlx::query(
            r#"
INSERT INTO files (id, root_hash, file_path, depth, block_count_downloaded, block_count_total, attrs, property, status, failed_reason, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?)
"#,
        )
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
        .execute(self.db.as_ref())
        .await?;

        Ok(())
    }

    pub async fn update_file_status(&self, id: &str, status: &SubscribedFileStatus) -> Result<()> {
        sqlx::query(
            r#"
UPDATE files
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
"#,
        )
        .bind(root_hash.to_string())
        .bind(rank)
        .fetch_all(self.db.as_ref())
        .await?;

        let res: Vec<SubscribedBlock> = res.into_iter().filter_map(|r| r.into().ok()).collect();

        Ok(res)
    }

    pub async fn upsert_blocks(&self, blocks: &[SubscribedBlock]) -> Result<()> {
        let mut tx = self.db.begin().await?;

        const CHUNK_SIZE: i64 = 100;

        for chunk in blocks.chunks(CHUNK_SIZE as usize) {
            let mut query_builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
                r#"
INSERT INTO blocks (root_hash, block_hash, rank, `index`, downloaded)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(root_hash, block_hash, rank, `index`) DO UPDATE SET
        downloaded = excluded.downloaded
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
            query_builder.build().execute(&mut *tx).await?;
        }

        tx.commit().await?;

        Ok(())
    }

    pub async fn upsert_file_and_blocks(&self, file: &SubscribedFile, blocks: &[SubscribedBlock]) -> Result<()> {
        let mut tx = self.db.begin().await?;

        let row = SubscribedFileRow::from(file)?;
        sqlx::query(
            r#"
INSERT INTO files (id, root_hash, file_path, rank, block_count_downloaded, block_count_total, attrs, priority, status, failed_reason, created_at, updated_at)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(id) DO UPDATE SET
        root_hash = excluded.root_hash,
        file_path = excluded.file_path,
        rank = excluded.rank,
        block_count_downloaded = excluded.block_count_downloaded,
        block_count_total = excluded.block_count_total,
        attrs = excluded.attrs,
        priority = excluded.priority,
        status = excluded.status,
        failed_reason = excluded.failed_reason,
        updated_at = excluded.updated_at
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
        .bind(row.updated_at)
        .execute(&mut *tx)
        .await?;

        const CHUNK_SIZE: i64 = 100;

        for chunk in blocks.chunks(CHUNK_SIZE as usize) {
            let mut query_builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
                r#"
INSERT INTO blocks (root_hash, block_hash, rank, `index`, downloaded)
    VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(root_hash, block_hash, rank, `index`) DO UPDATE SET
        downloaded = excluded.downloaded
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
            query_builder.build().execute(&mut *tx).await?;
        }

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
            block_count_total: item.block_count_downloaded as i64,
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
