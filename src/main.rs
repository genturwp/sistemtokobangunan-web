//! `stb-web` binary. Usage: `stb-web serve | waitlist-export <file.csv> | backup <dest.db>`.

use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use stb_web::admin_cli;
use stb_web::config::{self, Config};
use stb_web::db::Db;
use stb_web::web::{AppState, router};
use tracing_subscriber::EnvFilter;

const USAGE: &str = "usage: stb-web serve
       stb-web waitlist-export <file.csv>
       stb-web backup <dest.db>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["serve"] => {
            init_tracing(false);
            serve()
        }
        ["waitlist-export", file] => {
            init_tracing(true);
            waitlist_export(Path::new(file))
        }
        ["backup", dest] => {
            init_tracing(true);
            backup(Path::new(dest))
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Logs go to stdout for the server, and to stderr for the owner's commands, whose stdout
/// carries only their one-line result.
fn init_tracing(to_stderr: bool) {
    let builder = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()));
    if to_stderr {
        builder.with_writer(std::io::stderr).init();
    } else {
        builder.with_writer(std::io::stdout).init();
    }
}

/// Opens the database named by `STB_DATABASE_PATH` (applying migrations) for an owner command.
fn open_database_from_env() -> Result<Db, String> {
    let path = config::database_path_from_env()
        .map_err(|error| format!("cannot read configuration: {error}"))?;
    Db::open(&path).map_err(|error| format!("cannot open the database {}: {error}", path.display()))
}

/// `waitlist-export <file.csv>`: prints the row count and file name, never lead data.
fn waitlist_export(file: &Path) -> ExitCode {
    let result = open_database_from_env()
        .and_then(|db| admin_cli::waitlist_export(&db, file).map_err(|error| error.to_string()));
    match result {
        Ok(rows) => {
            println!("exported {rows} rows to {}", file.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `backup <dest.db>`: refuses an existing destination; prints the file name on success.
fn backup(dest: &Path) -> ExitCode {
    let result = open_database_from_env()
        .and_then(|db| admin_cli::backup(&db, dest).map_err(|error| error.to_string()));
    match result {
        Ok(()) => {
            println!("backup written to {}", dest.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve() -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(%error, "cannot read configuration");
            return ExitCode::FAILURE;
        }
    };

    let db = match Db::open(&config.database_path) {
        Ok(db) => db,
        Err(error) => {
            tracing::error!(%error, path = %config.database_path.display(), "cannot open the database");
            return ExitCode::FAILURE;
        }
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "cannot start the async runtime");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run(config, db)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "server stopped with an error");
            ExitCode::FAILURE
        }
    }
}

async fn run(config: Config, db: Db) -> std::io::Result<()> {
    let bind = config.bind;
    let state = AppState {
        config: Arc::new(config),
        db,
    };
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, "listening");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
}

/// Resolves on Ctrl+C, or on SIGTERM on unix (what systemd sends to stop a service).
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "cannot listen for Ctrl+C");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut sigterm) => {
                sigterm.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutting down");
}
