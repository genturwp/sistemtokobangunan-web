//! HTTP routes.

pub mod pages;

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::db::Db;

/// Shared state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Db,
}

/// Builds the application router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(pages::home))
        .route("/privasi", get(pages::privacy))
        .route("/healthz", get(healthz))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
}

/// Liveness check for the reverse proxy and uptime monitoring.
async fn healthz() -> &'static str {
    "ok"
}
