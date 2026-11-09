//! The SQLite database: one connection behind a mutex, used from blocking threads (ADR-0009).

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

/// Why a database operation failed.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("database schema version {found} is newer than this binary knows ({known})")]
    UnknownVersion { found: i64, known: i64 },
}

/// Shared handle to the database.
#[derive(Clone, Debug)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

/// Schema migrations, in order: (version, SQL). Version `n` is applied when
/// `PRAGMA user_version` is below `n`. Append only; never edit a released migration.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/0001_lead.sql"))];

/// How long a statement waits for a lock held by another connection.
const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

impl Db {
    /// Opens (or creates) the database file and applies pending migrations.
    pub fn open(path: &Path) -> Result<Db, DbError> {
        Db::init(Connection::open(path)?)
    }

    /// Opens a private in-memory database with all migrations applied (tests).
    pub fn open_in_memory() -> Result<Db, DbError> {
        Db::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Db, DbError> {
        // `journal_mode` answers with the mode in force ("memory" for an in-memory database).
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        migrate(&mut conn)?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs `f` with the connection on a blocking thread.
    pub async fn call<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        let result = tokio::task::spawn_blocking(move || f(&mut lock(&conn))).await?;
        Ok(result?)
    }

    /// Locks the connection on the calling thread, for synchronous callers such as the owner's
    /// CLI subcommands. Async code uses [`Db::call`] instead: never hold the guard across an
    /// `.await`, and never call this on a runtime worker thread.
    pub fn lock_blocking(&self) -> MutexGuard<'_, Connection> {
        lock(&self.conn)
    }
}

/// Locks the connection, ignoring poisoning: a panic in an earlier holder poisons the mutex, but
/// the connection itself is still usable (a `rusqlite::Transaction` rolls back when dropped
/// during unwinding).
fn lock(conn: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    conn.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Applies every migration newer than `PRAGMA user_version`, and sets the new version, in one
/// transaction: either all of them land or none.
fn migrate(conn: &mut Connection) -> Result<(), DbError> {
    let known = MIGRATIONS.last().map_or(0, |(version, _)| *version);
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let found: i64 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if found > known {
        return Err(DbError::UnknownVersion { found, known });
    }
    for (version, sql) in MIGRATIONS {
        if *version > found {
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
            tracing::info!(version, "applied database migration");
        }
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDbFile;

    #[tokio::test]
    async fn migrations_set_user_version_1() {
        let db = Db::open_in_memory().unwrap();
        let version: i64 = db
            .call(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get(0)))
            .await
            .unwrap();
        assert_eq!(version, 1);
        let tables: i64 = db
            .call(|conn| {
                conn.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = 'lead'",
                    [],
                    |row| row.get(0),
                )
            })
            .await
            .unwrap();
        assert_eq!(tables, 1);
    }

    #[tokio::test]
    async fn open_file_sets_pragmas() {
        let file = TempDbFile::new("pragmas");
        let path = file.0.clone();
        let db = Db::open(&path).unwrap();
        let (journal_mode, foreign_keys, busy_timeout) = db
            .call(|conn| {
                let journal_mode: String =
                    conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
                let foreign_keys: i64 =
                    conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
                let busy_timeout: i64 =
                    conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
                Ok((journal_mode, foreign_keys, busy_timeout))
            })
            .await
            .unwrap();

        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
        assert_eq!(busy_timeout, 5000);
    }

    #[tokio::test]
    async fn reopening_keeps_data_and_version() {
        let file = TempDbFile::new("reopen");
        let path = file.0.clone();
        {
            let db = Db::open(&path).unwrap();
            db.call(|conn| {
                conn.execute(
                    "INSERT INTO lead (kind, name, whatsapp, store_name, consent_at, created_at) \
                     VALUES ('waitlist', 'n', '+6281234567890', 's', 't', 't')",
                    [],
                )
            })
            .await
            .unwrap();
        }
        let db = Db::open(&path).unwrap();
        let (version, rows): (i64, i64) = db
            .call(|conn| {
                let version = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
                let rows = conn.query_row("SELECT count(*) FROM lead", [], |row| row.get(0))?;
                Ok((version, rows))
            })
            .await
            .unwrap();

        assert_eq!(version, 1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn lock_blocking_gives_the_migrated_connection_without_a_runtime() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn newer_schema_than_binary_is_refused() {
        let file = TempDbFile::new("newer");
        let path = file.0.clone();
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", 99).unwrap();
        }
        let result = Db::open(&path);

        assert!(
            matches!(
                result,
                Err(DbError::UnknownVersion {
                    found: 99,
                    known: 1
                })
            ),
            "got {result:?}"
        );
    }
}
