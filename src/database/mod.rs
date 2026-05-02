use std::sync::{Arc, Mutex};

use anyhow::Context;
use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::{config::DdnsProvider, models::Record};

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn connect(sqlite_path: &str, providers: &[DdnsProvider]) -> anyhow::Result<Self> {
        if let Some(parent) = std::path::Path::new(sqlite_path).parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create sqlite parent directory for {sqlite_path}")
            })?;
        }

        let conn = Connection::open(sqlite_path)
            .with_context(|| format!("failed to open sqlite database: {sqlite_path}"))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        db.migrate()?;
        db.seed_records(providers)?;
        Ok(db)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fqdn TEXT NOT NULL UNIQUE,
                record_type TEXT NOT NULL,
                proxied INTEGER NOT NULL DEFAULT 0,
                ttl INTEGER NOT NULL DEFAULT 120
            );

            CREATE TABLE IF NOT EXISTS state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sync_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                synced_at TEXT NOT NULL,
                status TEXT NOT NULL,
                message TEXT NOT NULL,
                ip TEXT
            );

            CREATE TABLE IF NOT EXISTS auth_tokens (
                token TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                expires_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_sync_history_synced_at ON sync_history (synced_at DESC);
            CREATE INDEX IF NOT EXISTS idx_auth_tokens_expires_at ON auth_tokens (expires_at);
            "#,
        )?;
        Ok(())
    }

    fn seed_records(&self, providers: &[DdnsProvider]) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");

        for provider in providers {
            for record in &provider.records {
                conn.execute(
                    "INSERT OR IGNORE INTO records (fqdn, record_type, proxied, ttl) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        record.fqdn,
                        record.record_type.to_string(),
                        if record.proxied { 1 } else { 0 },
                        i64::from(record.ttl)
                    ],
                )?;
            }
        }

        Ok(())
    }

    pub fn list_records(&self) -> anyhow::Result<Vec<Record>> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        let mut stmt = conn
            .prepare("SELECT id, fqdn, record_type, proxied, ttl FROM records ORDER BY id ASC")?;

        let rows = stmt.query_map([], |row| {
            Ok(Record {
                id: row.get(0)?,
                fqdn: row.get(1)?,
                record_type: row.get(2)?,
                proxied: row.get::<_, i64>(3)? != 0,
                ttl: row.get::<_, u32>(4)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn create_record(
        &self,
        fqdn: &str,
        record_type: &str,
        proxied: bool,
        ttl: u32,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO records (fqdn, record_type, proxied, ttl) VALUES (?1, ?2, ?3, ?4)",
            params![
                fqdn,
                record_type,
                if proxied { 1 } else { 0 },
                i64::from(ttl)
            ],
        )?;
        Ok(())
    }

    pub fn update_record(
        &self,
        id: i64,
        fqdn: Option<&str>,
        record_type: Option<&str>,
        proxied: Option<bool>,
        ttl: Option<u32>,
    ) -> anyhow::Result<bool> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");

        let existing = conn
            .query_row(
                "SELECT fqdn, record_type, proxied, ttl FROM records WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? != 0,
                        row.get::<_, u32>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((old_fqdn, old_type, old_proxied, old_ttl)) = existing else {
            return Ok(false);
        };

        conn.execute(
            "UPDATE records SET fqdn = ?1, record_type = ?2, proxied = ?3, ttl = ?4 WHERE id = ?5",
            params![
                fqdn.unwrap_or(&old_fqdn),
                record_type.unwrap_or(&old_type),
                if proxied.unwrap_or(old_proxied) { 1 } else { 0 },
                i64::from(ttl.unwrap_or(old_ttl)),
                id,
            ],
        )?;

        Ok(true)
    }

    pub fn delete_record(&self, id: i64) -> anyhow::Result<bool> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        let changed = conn.execute("DELETE FROM records WHERE id = ?1", params![id])?;
        Ok(changed > 0)
    }

    pub fn set_state(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO state (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_state(&self, key: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        let value = conn
            .query_row(
                "SELECT value FROM state WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(value)
    }

    pub fn add_sync_history(
        &self,
        status: &str,
        message: &str,
        ip: Option<&str>,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "INSERT INTO sync_history (synced_at, status, message, ip) VALUES (?1, ?2, ?3, ?4)",
            params![Utc::now().to_rfc3339(), status, message, ip],
        )?;
        Ok(())
    }

    pub fn latest_sync_history(&self) -> anyhow::Result<Option<(String, String, String)>> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        let row = conn
            .query_row(
                "SELECT synced_at, status, message FROM sync_history ORDER BY id DESC LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn create_access_token(
        &self,
        username: &str,
        ttl_secs: i64,
    ) -> anyhow::Result<(String, String)> {
        let token = format!("meshops_{}", Uuid::new_v4());
        let expires_at = (Utc::now() + Duration::seconds(ttl_secs)).to_rfc3339();
        let conn = self.conn.lock().expect("sqlite mutex poisoned");

        conn.execute(
            "INSERT INTO auth_tokens (token, username, expires_at) VALUES (?1, ?2, ?3)",
            params![token, username, expires_at],
        )?;

        Ok((token, expires_at))
    }

    pub fn validate_access_token(&self, token: &str) -> anyhow::Result<bool> {
        let conn = self.conn.lock().expect("sqlite mutex poisoned");
        conn.execute(
            "DELETE FROM auth_tokens WHERE expires_at <= ?1",
            params![Utc::now().to_rfc3339()],
        )?;

        let present = conn
            .query_row(
                "SELECT token FROM auth_tokens WHERE token = ?1",
                params![token],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        Ok(present.is_some())
    }
}
