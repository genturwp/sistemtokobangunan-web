//! HTTP routes.

use std::sync::Arc;

use axum::Router;
use axum::routing::get;

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
        .route("/healthz", get(healthz))
        .with_state(state)
}

/// Liveness check for the reverse proxy and uptime monitoring.
async fn healthz() -> &'static str {
    "ok"
}
