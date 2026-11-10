//! Runs the real `stb-web waitlist-export` and `stb-web backup` processes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use stb_web::db::Db;

/// A file in the temp directory, removed (with SQLite companions) before use and on drop.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str) -> TempFile {
        let file = TempFile(
            std::env::temp_dir().join(format!("stb-web-cli-{name}-{}", std::process::id())),
        );
        file.remove();
        file
    }

    fn remove(&self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut file = self.0.as_os_str().to_owned();
            file.push(suffix);
            let _ = std::fs::remove_file(file);
        }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Runs `stb-web` with only `STB_DATABASE_PATH` set (when given).
fn run(database: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_stb-web"));
    command
        .args(args)
        .env_remove("STB_DATABASE_PATH")
        .env_remove("STB_BASE_URL")
        .env_remove("STB_WHATSAPP_NUMBER")
        .env("RUST_LOG", "info");
    if let Some(database) = database {
        command.env("STB_DATABASE_PATH", database);
    }
    command.output().expect("run stb-web")
}

/// A database with one waitlist lead whose personal data must never reach the console.
fn seeded_database(name: &str) -> TempFile {
    let file = TempFile::new(name);
    let db = Db::open(&file.0).unwrap();
    db.lock_blocking()
        .execute(
            "INSERT INTO lead (kind, name, whatsapp, store_name, email, consent_at, created_at) \
             VALUES ('waitlist', 'Budi Rahasia', '+6281299998888', 'Toko Rahasia', \
             'budi@rahasia.id', '2026-11-09T01:00:00Z', '2026-11-09T01:00:00Z')",
            [],
        )
        .unwrap();
    file
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn assert_no_lead_data(output: &Output) {
    let console = text(&output.stdout) + &text(&output.stderr);
    for secret in [
        "Budi Rahasia",
        "6281299998888",
        "Toko Rahasia",
        "budi@rahasia.id",
    ] {
        assert!(
            !console.contains(secret),
            "console shows {secret:?}: {console}"
        );
    }
}

#[test]
fn waitlist_export_writes_csv_and_prints_one_line() {
    let database = seeded_database("export-db");
    let csv = TempFile::new("export.csv");

    let output = run(
        Some(&database.0),
        &["waitlist-export", csv.0.to_str().unwrap()],
    );

    assert!(output.status.success(), "got {output:?}");
    assert_eq!(
        text(&output.stdout),
        format!("exported 1 rows to {}\n", csv.0.display())
    );
    assert_no_lead_data(&output);
    let written = std::fs::read_to_string(&csv.0).unwrap();
    assert!(written.contains("\"Budi Rahasia\""));
}

#[test]
fn backup_writes_copy_and_prints_one_line() {
    let database = seeded_database("backup-db");
    let dest = TempFile::new("backup-dest.db");

    let output = run(Some(&database.0), &["backup", dest.0.to_str().unwrap()]);

    assert!(output.status.success(), "got {output:?}");
    assert_eq!(
        text(&output.stdout),
        format!("backup written to {}\n", dest.0.display())
    );
    assert_no_lead_data(&output);
    let copy = rusqlite::Connection::open(&dest.0).unwrap();
    let rows: i64 = copy
        .query_row("SELECT count(*) FROM lead", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 1);
}

#[test]
fn backup_over_an_existing_file_fails_and_keeps_it() {
    let database = seeded_database("backup-exists-db");
    let dest = TempFile::new("backup-exists-dest.db");
    std::fs::write(&dest.0, b"older backup").unwrap();

    let output = run(Some(&database.0), &["backup", dest.0.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(1), "got {output:?}");
    assert!(text(&output.stdout).is_empty(), "got {output:?}");
    assert!(
        text(&output.stderr).contains("already exists"),
        "got {output:?}"
    );
    assert_eq!(std::fs::read(&dest.0).unwrap(), b"older backup");
}

#[test]
fn owner_commands_refuse_a_missing_database_and_create_nothing() {
    let database = TempFile::new("absent-db");
    let target = TempFile::new("absent-target");

    for command in ["waitlist-export", "backup"] {
        let output = run(Some(&database.0), &[command, target.0.to_str().unwrap()]);

        assert_eq!(output.status.code(), Some(1), "{command}: {output:?}");
        assert!(text(&output.stdout).is_empty(), "{command}: {output:?}");
        assert!(
            text(&output.stderr).contains(&format!("database not found: {}", database.0.display())),
            "{command}: {output:?}"
        );
        assert!(!database.0.exists(), "{command} created the database");
        assert!(
            !target.0.exists(),
            "{command} created {}",
            target.0.display()
        );
    }
}

#[test]
fn missing_database_path_fails_with_exit_1() {
    let dest = TempFile::new("no-config-dest.db");

    let output = run(None, &["backup", dest.0.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(1), "got {output:?}");
    assert!(
        text(&output.stderr).contains("STB_DATABASE_PATH"),
        "got {output:?}"
    );
    assert!(!dest.0.exists());
}

#[test]
fn wrong_arguments_print_usage_with_exit_2() {
    for args in [
        &["backup"][..],
        &["waitlist-export"][..],
        &["backup", "a.db", "b.db"][..],
        &["waitlist-export", "a.csv", "b.csv"][..],
        &["export"][..],
    ] {
        let output = run(None, args);

        assert_eq!(output.status.code(), Some(2), "for {args:?}: {output:?}");
        let stderr = text(&output.stderr);
        assert!(
            stderr.contains("waitlist-export <file.csv>") && stderr.contains("backup <dest.db>"),
            "for {args:?}: {stderr}"
        );
    }
}
