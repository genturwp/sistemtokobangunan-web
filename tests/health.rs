use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use stb_web::config::Config;
use stb_web::db::Db;
use stb_web::web::{AppState, router};
use tower::ServiceExt;

fn test_state() -> AppState {
    AppState {
        config: Arc::new(Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_path: PathBuf::from(":memory:"),
            base_url: "http://localhost".to_string(),
            whatsapp_number: "6281234567890".to_string(),
        }),
        db: Db::open_in_memory().unwrap(),
    }
}

#[tokio::test]
async fn health_returns_ok() {
    let response = router(test_state())
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"ok");
}
