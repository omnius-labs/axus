use std::path::Path;

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chrono::{DateTime, Duration, Utc};
use omnius_core_migration::sqlite::{MigrationRequest, SqliteMigrator};
use rand::RngExt as _;
use sha3::{Digest, Sha3_256};
use sqlx::{Row as _, sqlite::SqlitePool};

use crate::prelude::*;

const SESSION_LIFETIME_DAYS: i64 = 30;
const LAST_SEEN_INTERVAL_MINUTES: i64 = 5;

/// The single owner and the server-side sessions that prove a login.
pub struct OwnerStore {
    db: SqlitePool,
}

impl OwnerStore {
    pub async fn open(dir: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(dir).await?;
        let path = dir.join("sqlite.db");
        let path = path
            .to_str()
            .ok_or_else(|| Error::new(ErrorKind::UnexpectedError).with_message("invalid owner database path"))?;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
            .busy_timeout(std::time::Duration::from_secs(10));
        let db = SqlitePool::connect_with(options).await?;
        Self::migrate(&db).await?;
        Ok(Self { db })
    }

    async fn migrate(db: &SqlitePool) -> Result<()> {
        let requests = vec![MigrationRequest {
            name: "2026-11-07_owner".to_string(),
            queries: r#"
CREATE TABLE IF NOT EXISTS owner (
    id INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT NOT NULL PRIMARY KEY,
    created_at TIMESTAMP NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    last_seen_at TIMESTAMP NOT NULL
);
"#
            .to_string(),
        }];
        SqliteMigrator::migrate(db, requests)
            .await
            .map_err(|error| Error::new(ErrorKind::UnexpectedError).with_message(error.to_string()))?;
        Ok(())
    }

    pub async fn initialized(&self) -> Result<bool> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(1) FROM owner").fetch_one(&self.db).await?;
        Ok(count > 0)
    }

    /// Registers the owner only when none exists, so two concurrent requests cannot create two owners.
    pub async fn setup(&self, name: &str, password: &str) -> Result<bool> {
        let hash = Self::hash_password(password)?;
        let now = Utc::now().naive_utc();
        let changed = sqlx::query(
            r#"
INSERT INTO owner (id, name, password_hash, created_at)
    SELECT 1, ?, ?, ?
    WHERE NOT EXISTS (SELECT 1 FROM owner)
"#,
        )
        .bind(name)
        .bind(hash)
        .bind(now)
        .execute(&self.db)
        .await?
        .rows_affected();
        Ok(changed == 1)
    }

    /// Returns a new opaque session token when the credentials match.
    pub async fn login(&self, name: &str, password: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT name, password_hash FROM owner WHERE id = 1").fetch_optional(&self.db).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let stored_name: String = row.get("name");
        let stored_hash: String = row.get("password_hash");
        let parsed = PasswordHash::new(&stored_hash).map_err(|error| Error::new(ErrorKind::UnexpectedError).with_message(error.to_string()))?;
        let verified = Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok();
        if stored_name != name || !verified {
            return Ok(None);
        }
        let token = hex::encode(rand::random::<[u8; 32]>());
        let now = Utc::now();
        sqlx::query("INSERT INTO sessions (token_hash, created_at, expires_at, last_seen_at) VALUES (?, ?, ?, ?)")
            .bind(Self::hash_token(&token))
            .bind(now.naive_utc())
            .bind((now + Duration::days(SESSION_LIFETIME_DAYS)).naive_utc())
            .bind(now.naive_utc())
            .execute(&self.db)
            .await?;
        Ok(Some(token))
    }

    pub async fn logout(&self, token: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(Self::hash_token(token))
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn authenticate(&self, token: &str) -> Result<bool> {
        let hash = Self::hash_token(token);
        let row = sqlx::query("SELECT expires_at, last_seen_at FROM sessions WHERE token_hash = ?")
            .bind(&hash)
            .fetch_optional(&self.db)
            .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let expires_at: DateTime<Utc> = DateTime::from_naive_utc_and_offset(row.get("expires_at"), Utc);
        let now = Utc::now();
        if expires_at <= now {
            sqlx::query("DELETE FROM sessions WHERE token_hash = ?").bind(&hash).execute(&self.db).await?;
            return Ok(false);
        }
        let last_seen_at: DateTime<Utc> = DateTime::from_naive_utc_and_offset(row.get("last_seen_at"), Utc);
        if now - last_seen_at >= Duration::minutes(LAST_SEEN_INTERVAL_MINUTES) {
            sqlx::query("UPDATE sessions SET last_seen_at = ? WHERE token_hash = ?")
                .bind(now.naive_utc())
                .bind(&hash)
                .execute(&self.db)
                .await?;
        }
        Ok(true)
    }

    fn hash_password(password: &str) -> Result<String> {
        // The salt only has to be unique. Use the crate's rand so the salt does not depend on password-hash's
        // optional rand_core feature.
        let salt = SaltString::encode_b64(&rand::rng().random::<[u8; 16]>()).map_err(|error| Error::new(ErrorKind::UnexpectedError).with_message(error.to_string()))?;
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|error| Error::new(ErrorKind::UnexpectedError).with_message(error.to_string()))?;
        Ok(hash.to_string())
    }

    fn hash_token(token: &str) -> String {
        let mut hasher = Sha3_256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::*;

    #[tokio::test]
    async fn setup_login_and_authenticate() -> TestResult {
        let dir = tempfile::tempdir()?;
        let store = OwnerStore::open(dir.path()).await?;
        assert!(!store.initialized().await?);
        assert!(store.setup("owner", "password123").await?);
        assert!(store.initialized().await?);
        assert!(!store.setup("other", "password123").await?);
        assert!(store.login("owner", "wrong").await?.is_none());
        let token = store.login("owner", "password123").await?.expect("a session");
        assert!(store.authenticate(&token).await?);
        store.logout(&token).await?;
        assert!(!store.authenticate(&token).await?);
        Ok(())
    }
}
