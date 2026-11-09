//! The owner's command-line tools: `stb-web waitlist-export <file.csv>` and
//! `stb-web backup <dest.db>` (website plan 1, Task 5; RFC-0001).
//!
//! Nothing here prints or logs lead data: callers report only counts and file paths.

use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite::backup::{Backup, StepResult};

use crate::db::Db;

/// Why an owner command failed.
#[derive(Debug, thiserror::Error)]
pub enum AdminError {
    #[error("database not found: {0}")]
    DatabaseNotFound(PathBuf),
    #[error("cannot open the database {path}: {source}")]
    Open {
        path: PathBuf,
        source: crate::db::DbError,
    },
    #[error("{0} already exists; choose a new file name")]
    DestinationExists(PathBuf),
    #[error("cannot write {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// Byte-order mark written first, so Excel reads the file as UTF-8.
const BOM: &str = "\u{feff}";
/// Column names, in output order.
const HEADER: &str = "created_at,name,whatsapp,store_name,email,source,status";

/// Writes the waitlist (`kind = 'waitlist'`) as CSV to `out` and returns the number of rows.
///
/// UTF-8 with a BOM; the header line [`HEADER`]; rows ordered by `created_at`, then `id`; lines
/// end in CRLF (RFC 4180). Every field is quoted with `"` doubled, NULL is an empty field, and a
/// field starting with `=`, `+`, `-`, `@`, a tab or a carriage return gets a leading `'` so a
/// spreadsheet does not run it as a formula (stored WhatsApp numbers start with `+`, so they get
/// it too). One `SELECT`, so the rows are a consistent snapshot. A database error is returned
/// as an [`std::io::Error`].
pub fn export_waitlist_csv(conn: &Connection, out: &mut impl Write) -> std::io::Result<usize> {
    let mut stmt = conn
        .prepare(
            "SELECT created_at, name, whatsapp, store_name, email, source, status FROM lead
             WHERE kind = 'waitlist'
             ORDER BY created_at, id",
        )
        .map_err(std::io::Error::other)?;
    let mut rows = stmt.query([]).map_err(std::io::Error::other)?;

    write!(out, "{BOM}{HEADER}\r\n")?;
    let mut count = 0;
    while let Some(row) = rows.next().map_err(std::io::Error::other)? {
        for column in 0..7 {
            if column > 0 {
                out.write_all(b",")?;
            }
            let value: Option<String> = row.get(column).map_err(std::io::Error::other)?;
            write_field(out, value.as_deref().unwrap_or(""))?;
        }
        out.write_all(b"\r\n")?;
        count += 1;
    }
    Ok(count)
}

/// Writes one quoted CSV field, formula-neutralised (see [`export_waitlist_csv`]).
fn write_field(out: &mut impl Write, value: &str) -> std::io::Result<()> {
    let formula = matches!(
        value.chars().next(),
        Some('=' | '+' | '-' | '@' | '\t' | '\r')
    );
    let prefix = if formula { "'" } else { "" };
    write!(out, "\"{prefix}{}\"", value.replace('"', "\"\""))
}

/// Copies the whole database behind `conn` into the database file at `dest` (created if
/// missing), with SQLite's online backup API. The copy is made in one step, so it is a
/// consistent snapshot even while the server is writing.
pub fn backup_to(conn: &Connection, dest: &Path) -> rusqlite::Result<()> {
    let mut copy = Connection::open(dest)?;
    let backup = Backup::new(conn, &mut copy)?;
    match backup.step(-1)? {
        StepResult::Done => Ok(()),
        // `step(-1)` copies every page, so `More` cannot happen; `Busy` and `Locked` mean
        // another connection held a lock the copy needed.
        _ => Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            Some("the database is busy; try the backup again".to_string()),
        )),
    }
}

/// Opens the owner's database at `path` (applying migrations), which must already exist: a
/// mistyped `STB_DATABASE_PATH` must not create an empty database that is then exported or
/// backed up as if it were the real one. Only `serve` creates the database.
pub fn open_existing_database(path: &Path) -> Result<Db, AdminError> {
    if !path.is_file() {
        return Err(AdminError::DatabaseNotFound(path.to_path_buf()));
    }
    Db::open(path).map_err(|error| AdminError::Open {
        path: path.to_path_buf(),
        source: error,
    })
}

/// `waitlist-export`: writes the waitlist CSV to `file` (replacing it if it exists) and returns
/// the number of rows. On failure the partly written file is removed.
pub fn waitlist_export(db: &Db, file: &Path) -> Result<usize, AdminError> {
    let io_error = |source| AdminError::Io {
        path: file.to_path_buf(),
        source,
    };
    let write = || -> std::io::Result<usize> {
        let mut out = std::io::BufWriter::new(std::fs::File::create(file)?);
        let rows = export_waitlist_csv(&db.lock_blocking(), &mut out)?;
        out.into_inner()
            .map_err(|error| error.into_error())?
            .sync_all()?;
        Ok(rows)
    };
    write().map_err(|error| {
        let _ = std::fs::remove_file(file);
        io_error(error)
    })
}

/// `backup`: copies the database to `dest`, which must not exist yet, so an older backup is
/// never overwritten by a typo. On failure the new file is removed.
pub fn backup(db: &Db, dest: &Path) -> Result<(), AdminError> {
    // Creating the file with `create_new` reserves the name atomically: no other process can
    // slip a file in between the check and the copy.
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
    {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(AdminError::DestinationExists(dest.to_path_buf()));
        }
        Err(source) => {
            return Err(AdminError::Io {
                path: dest.to_path_buf(),
                source,
            });
        }
    }
    backup_to(&db.lock_blocking(), dest).map_err(|error| {
        let _ = std::fs::remove_file(dest);
        AdminError::from(error)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDbFile;
    use rusqlite::params;

    const BOM_AND_HEADER: &str =
        "\u{feff}created_at,name,whatsapp,store_name,email,source,status\r\n";

    /// One `lead` row; `email` and `source` may be NULL.
    struct Row<'a> {
        kind: &'a str,
        name: &'a str,
        whatsapp: &'a str,
        store_name: &'a str,
        email: Option<&'a str>,
        source: Option<&'a str>,
        created_at: &'a str,
    }

    fn row<'a>(name: &'a str, whatsapp: &'a str, created_at: &'a str) -> Row<'a> {
        Row {
            kind: "waitlist",
            name,
            whatsapp,
            store_name: "Toko Maju",
            email: None,
            source: None,
            created_at,
        }
    }

    fn insert(conn: &Connection, r: &Row) {
        conn.execute(
            "INSERT INTO lead (kind, name, whatsapp, store_name, email, consent_at, source, \
             created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?6)",
            params![
                r.kind,
                r.name,
                r.whatsapp,
                r.store_name,
                r.email,
                r.created_at,
                r.source
            ],
        )
        .unwrap();
    }

    fn export(conn: &Connection) -> (usize, String) {
        let mut out = Vec::new();
        let rows = export_waitlist_csv(conn, &mut out).unwrap();
        (rows, String::from_utf8(out).unwrap())
    }

    #[test]
    fn csv_has_header_and_rows() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        insert(
            &conn,
            &row("Budi", "+6281234567890", "2026-11-09T01:00:00Z"),
        );
        insert(
            &conn,
            &row("Sari", "+6281234567891", "2026-11-09T02:00:00Z"),
        );

        let (rows, csv) = export(&conn);

        assert_eq!(rows, 2);
        assert!(csv.starts_with(BOM_AND_HEADER), "got {csv:?}");
        assert_eq!(csv.lines().count(), 3);
    }

    #[test]
    fn csv_quotes_and_neutralises_formulas() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        insert(
            &conn,
            &row(
                "=HYPERLINK(\"x\")",
                "+6281234567890",
                "2026-11-09T01:00:00Z",
            ),
        );

        let (_, csv) = export(&conn);

        assert!(csv.contains(",\"'=HYPERLINK(\"\"x\"\")\","), "got {csv:?}");
    }

    #[test]
    fn csv_row_has_every_field_quoted_and_null_as_empty() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        insert(
            &conn,
            &row("Budi", "+6281234567890", "2026-11-09T01:00:00Z"),
        );

        let (_, csv) = export(&conn);

        // The stored number starts with `+`, so it gets the leading `'` like any other field.
        assert_eq!(
            csv,
            format!(
                "{BOM_AND_HEADER}\"2026-11-09T01:00:00Z\",\"Budi\",\"'+6281234567890\",\
                 \"Toko Maju\",\"\",\"\",\"baru\"\r\n"
            )
        );
    }

    #[test]
    fn csv_neutralises_each_formula_prefix_in_any_field() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        insert(
            &conn,
            &Row {
                kind: "waitlist",
                name: "+SUM(1)",
                whatsapp: "+6281234567890",
                store_name: "-1+1",
                email: Some("@x.co"),
                source: Some("=1"),
                created_at: "2026-11-09T01:00:00Z",
            },
        );

        let (_, csv) = export(&conn);

        assert!(
            csv.ends_with(
                ",\"'+SUM(1)\",\"'+6281234567890\",\"'-1+1\",\"'@x.co\",\"'=1\",\"baru\"\r\n"
            ),
            "got {csv:?}"
        );
    }

    #[test]
    fn csv_neutralises_leading_tab_and_carriage_return() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        insert(
            &conn,
            &Row {
                store_name: "\r=1",
                ..row("\t=1", "+6281234567890", "2026-11-09T01:00:00Z")
            },
        );

        let (_, csv) = export(&conn);

        assert!(
            csv.contains(",\"'\t=1\",\"'+6281234567890\",\"'\r=1\","),
            "got {csv:?}"
        );
    }

    #[test]
    fn open_existing_database_refuses_a_missing_file_and_creates_nothing() {
        let file = TempDbFile::new("admin-open-missing");

        let result = open_existing_database(&file.0);

        assert!(
            matches!(&result, Err(AdminError::DatabaseNotFound(path)) if path == &file.0),
            "got {result:?}"
        );
        assert!(!file.0.exists());
    }

    #[test]
    fn open_existing_database_opens_an_existing_file() {
        let file = TempDbFile::new("admin-open-existing");
        drop(Db::open(&file.0).unwrap());

        let db = open_existing_database(&file.0).unwrap();

        let version: i64 = db
            .lock_blocking()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn csv_keeps_ordinary_text_and_newlines_inside_quotes() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        insert(
            &conn,
            &row(
                "Budi, \"Pak\"\nSantoso",
                "+6281234567890",
                "2026-11-09T01:00:00Z",
            ),
        );

        let (_, csv) = export(&conn);

        assert!(
            csv.contains(",\"Budi, \"\"Pak\"\"\nSantoso\","),
            "got {csv:?}"
        );
    }

    #[test]
    fn csv_has_only_waitlist_rows_ordered_by_created_at_then_id() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        insert(
            &conn,
            &row("Ketiga", "+6281234567893", "2026-11-09T03:00:00Z"),
        );
        insert(
            &conn,
            &row("Pertama", "+6281234567891", "2026-11-09T01:00:00Z"),
        );
        insert(
            &conn,
            &Row {
                kind: "trial",
                ..row("Uji Coba", "+6281234567894", "2026-11-09T00:00:00Z")
            },
        );
        insert(
            &conn,
            &row("Kedua", "+6281234567892", "2026-11-09T01:00:00Z"),
        );

        let (rows, csv) = export(&conn);

        assert_eq!(rows, 3);
        let names: Vec<&str> = csv
            .lines()
            .skip(1)
            .map(|line| line.split(',').nth(1).unwrap())
            .collect();
        assert_eq!(names, ["\"Pertama\"", "\"Kedua\"", "\"Ketiga\""]);
    }

    #[test]
    fn csv_of_empty_waitlist_is_header_only() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();

        let (rows, csv) = export(&conn);

        assert_eq!(rows, 0);
        assert_eq!(csv, BOM_AND_HEADER);
    }

    #[test]
    fn backup_copy_is_readable() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.lock_blocking();
        insert(
            &conn,
            &row("Budi", "+6281234567890", "2026-11-09T01:00:00Z"),
        );
        insert(
            &conn,
            &row("Sari", "+6281234567891", "2026-11-09T02:00:00Z"),
        );
        let dest = TempDbFile::new("admin-backup-readable");

        backup_to(&conn, &dest.0).unwrap();

        let copy = Connection::open(&dest.0).unwrap();
        let rows: i64 = copy
            .query_row("SELECT count(*) FROM lead", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 2);
    }

    #[test]
    fn backup_writes_a_new_file_with_schema_version() {
        let db = Db::open_in_memory().unwrap();
        insert(
            &db.lock_blocking(),
            &row("Budi", "+6281234567890", "2026-11-09T01:00:00Z"),
        );
        let dest = TempDbFile::new("admin-backup-new");

        backup(&db, &dest.0).unwrap();

        let copy = Connection::open(&dest.0).unwrap();
        let (version, rows): (i64, i64) = copy
            .query_row(
                "SELECT (SELECT user_version FROM pragma_user_version), count(*) FROM lead",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((version, rows), (1, 1));
    }

    #[test]
    fn backup_refuses_an_existing_destination() {
        let db = Db::open_in_memory().unwrap();
        let dest = TempDbFile::new("admin-backup-exists");
        std::fs::write(&dest.0, b"older backup").unwrap();

        let result = backup(&db, &dest.0);

        assert!(
            matches!(&result, Err(AdminError::DestinationExists(path)) if path == &dest.0),
            "got {result:?}"
        );
        assert_eq!(std::fs::read(&dest.0).unwrap(), b"older backup");
    }

    #[test]
    fn waitlist_export_writes_the_file_and_counts_rows() {
        let db = Db::open_in_memory().unwrap();
        insert(
            &db.lock_blocking(),
            &row("Budi", "+6281234567890", "2026-11-09T01:00:00Z"),
        );
        let file = TempDbFile::new("admin-export");

        let rows = waitlist_export(&db, &file.0).unwrap();

        assert_eq!(rows, 1);
        let csv = std::fs::read_to_string(&file.0).unwrap();
        assert!(csv.starts_with(BOM_AND_HEADER));
        assert!(csv.contains("\"Budi\""));
    }

    #[test]
    fn waitlist_export_into_a_missing_directory_is_an_io_error() {
        let db = Db::open_in_memory().unwrap();
        let file = std::env::temp_dir()
            .join(format!("stb-web-no-such-dir-{}", std::process::id()))
            .join("w.csv");

        let result = waitlist_export(&db, &file);

        assert!(
            matches!(&result, Err(AdminError::Io { path, .. }) if path == &file),
            "got {result:?}"
        );
    }
}
