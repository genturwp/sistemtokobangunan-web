//! HTTP routes.

pub mod pages;
pub mod waitlist;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
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
        .route("/waitlist", post(waitlist::submit))
        .route(waitlist::THANKS_PATH, get(waitlist::thanks))
        .route("/healthz", get(healthz))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
}

/// Liveness check for the reverse proxy and uptime monitoring.
async fn healthz() -> &'static str {
    "ok"
}
