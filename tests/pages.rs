use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
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
            base_url: "https://example.test".to_string(),
            whatsapp_number: "6281234567890".to_string(),
        }),
        db: Db::open_in_memory().unwrap(),
    }
}

/// Sends `GET uri` to the router and returns the status, the content type and the body.
async fn get(uri: &str) -> (StatusCode, String, String) {
    let response = router(test_state())
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        content_type,
        String::from_utf8(body.to_vec()).unwrap(),
    )
}

#[tokio::test]
async fn home_has_headline_and_form() {
    let (status, content_type, body) = get("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/html"), "{content_type}");
    assert!(body.contains("Siapa yang masih kasbon, berapa, sejak kapan. Langsung kelihatan."));
    assert!(body.contains(r#"action="/waitlist""#));
    assert!(body.contains(r#"name="whatsapp""#));
    assert!(body.contains(r#"name="website""#));
}

#[tokio::test]
async fn home_has_seo_tags() {
    let (_, _, body) = get("/").await;
    // The template engine may spell `&` as `&amp;` or `&#38;`; both read the same.
    let text = body.replace("&#38;", "&").replace("&amp;", "&");
    assert!(
        text.contains("<title>Aplikasi Kasir & Kasbon Toko Bangunan | SistemTokoBangunan</title>")
    );
    assert!(text.contains(r#"<meta property="og:url" content="https://example.test/">"#));
    assert!(text.contains(r#"<meta property="og:type" content="website">"#));
    assert!(text.contains(r#"<html lang="id">"#));
    assert!(body.contains(r#"<link rel="canonical" href="https://example.test/">"#));
}

#[tokio::test]
async fn home_keeps_utm_source() {
    let (_, _, body) = get("/?utm_source=instagram").await;
    assert!(
        body.contains(r#"name="source" value="instagram""#),
        "{body}"
    );
}

#[tokio::test]
async fn home_drops_unclean_utm_source() {
    let (status, _, body) = get("/?utm_source=%3Cscript%3E").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"name="source" value="""#), "{body}");
    assert!(!body.contains("<script>"));
}

#[tokio::test]
async fn home_ignores_unexpected_query() {
    let (status, _, body) = get("/?utm_source=a&utm_source=b&fbclid=xyz").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(r#"action="/waitlist""#));
}

#[tokio::test]
async fn home_links_whatsapp_and_privacy() {
    let (_, _, body) = get("/").await;
    assert!(body.contains(r#"href="https://wa.me/6281234567890""#));
    assert!(body.contains("Tanya lewat WhatsApp"));
    assert!(body.contains(r#"href="/privasi""#));
    assert!(body.contains(r#"name="consent" value="true""#));
}

#[tokio::test]
async fn home_has_no_script() {
    let (_, _, body) = get("/").await;
    assert!(!body.contains("<script"));
}

#[tokio::test]
async fn privacy_states_retention() {
    let (status, content_type, body) = get("/privasi").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/html"), "{content_type}");
    assert!(body.contains("Kebijakan Privasi"));
    assert!(body.contains("12 bulan"));
    assert!(body.contains(r#"href="https://wa.me/6281234567890""#));
}

#[tokio::test]
async fn static_css_served() {
    let (status, content_type, _) = get("/static/site.css").await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/css"), "{content_type}");
}
