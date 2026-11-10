use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use stb_web::config::Config;
use stb_web::db::Db;
use stb_web::web::{AppState, router};
use tower::ServiceExt;

const THANKS: &str = "/waitlist/terima-kasih";

fn test_state(db: Db) -> AppState {
    AppState {
        config: Arc::new(Config {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_path: PathBuf::from(":memory:"),
            base_url: "https://example.test".to_string(),
            whatsapp_number: "6281234567890".to_string(),
        }),
        db,
    }
}

/// The response's status, `Location` header (empty if absent) and body.
struct Reply {
    status: StatusCode,
    location: String,
    body: String,
}

async fn send(db: &Db, request: Request<Body>) -> Reply {
    let response = router(test_state(db.clone()))
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let location = response
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        location,
        body: String::from_utf8(body.to_vec()).unwrap(),
    }
}

/// Posts an `application/x-www-form-urlencoded` body (already encoded) to `/waitlist`.
async fn post(db: &Db, form: &str) -> Reply {
    let request = Request::post("/waitlist")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form.to_string()))
        .unwrap();
    send(db, request).await
}

async fn lead_count(db: &Db) -> i64 {
    db.call(|conn| conn.query_row("SELECT count(*) FROM lead", [], |row| row.get(0)))
        .await
        .unwrap()
}

/// A complete, valid form with the given (already encoded) WhatsApp number.
fn valid_form(whatsapp: &str) -> String {
    format!(
        "name=Pak+Darto&whatsapp={whatsapp}&store_name=TB+Maju&email=&consent=true&source=instagram&website="
    )
}

#[tokio::test]
async fn valid_post_redirects_and_stores() {
    let db = Db::open_in_memory().unwrap();
    let reply = post(&db, &valid_form("0812-3456-7890")).await;

    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(reply.location, THANKS);
    let stored: i64 = db
        .call(|conn| {
            conn.query_row(
                "SELECT count(*) FROM lead WHERE whatsapp = '+6281234567890'",
                [],
                |row| row.get(0),
            )
        })
        .await
        .unwrap();
    assert_eq!(stored, 1);
    assert_eq!(lead_count(&db).await, 1);
}

#[tokio::test]
async fn stored_times_are_whole_seconds_utc() {
    let db = Db::open_in_memory().unwrap();
    let reply = post(&db, &valid_form("081234567890")).await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{}", reply.body);

    let (created_at, consent_at): (String, String) = db
        .call(|conn| {
            conn.query_row("SELECT created_at, consent_at FROM lead", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
        })
        .await
        .unwrap();
    for stored in [&created_at, &consent_at] {
        // `2026-11-10T03:04:05Z`: 20 characters, no fraction, UTC.
        assert_eq!(stored.len(), 20, "{stored}");
        assert!(!stored.contains('.'), "{stored}");
        assert!(stored.ends_with('Z'), "{stored}");
    }
}

#[tokio::test]
async fn ticked_consent_true_is_accepted() {
    let db = Db::open_in_memory().unwrap();
    let reply = post(
        &db,
        "name=Bu+Sri&whatsapp=%2B62+812+3456+7890&store_name=TB+Sri&consent=true",
    )
    .await;

    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{}", reply.body);
    assert_eq!(reply.location, THANKS);
    assert_eq!(lead_count(&db).await, 1);
}

#[tokio::test]
async fn post_same_number_twice_shows_thanks_both_times() {
    let db = Db::open_in_memory().unwrap();
    let first = post(&db, &valid_form("0812-3456-7890")).await;
    let second = post(&db, &valid_form("%2B62+812+3456+7890")).await;

    assert_eq!(first.status, StatusCode::SEE_OTHER);
    assert_eq!(first.location, THANKS);
    assert_eq!(second.status, StatusCode::SEE_OTHER);
    assert_eq!(second.location, THANKS);
    assert_eq!(lead_count(&db).await, 1);
}

#[tokio::test]
async fn invalid_number_rerenders_with_values() {
    let db = Db::open_in_memory().unwrap();
    let reply = post(&db, &valid_form("021-5551234")).await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        reply.body.contains("Nomor WhatsApp tidak valid"),
        "{}",
        reply.body
    );
    assert!(
        reply.body.contains(r#"value="021-5551234""#),
        "{}",
        reply.body
    );
    assert!(
        reply.body.contains(r#"value="Pak Darto""#),
        "{}",
        reply.body
    );
    assert!(reply.body.contains(r#"value="TB Maju""#), "{}", reply.body);
    assert!(
        reply
            .body
            .contains(r#"name="consent" value="true" checked"#),
        "{}",
        reply.body
    );
    assert!(
        reply.body.contains(r#"name="source" value="instagram""#),
        "{}",
        reply.body
    );
    assert_eq!(lead_count(&db).await, 0);
}

#[tokio::test]
async fn missing_consent_rerenders_and_stores_nothing() {
    let db = Db::open_in_memory().unwrap();
    let reply = post(
        &db,
        "name=Pak+Darto&whatsapp=0812-3456-7890&store_name=TB+Maju&email=&source=&website=",
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        reply.body.contains("Centang persetujuan untuk melanjutkan"),
        "{}",
        reply.body
    );
    assert!(!reply.body.contains(" checked"), "{}", reply.body);
    assert_eq!(lead_count(&db).await, 0);
}

#[tokio::test]
async fn script_in_name_is_escaped_on_rerender() {
    let db = Db::open_in_memory().unwrap();
    let reply = post(
        &db,
        "name=%3Cscript%3Ex%3C%2Fscript%3E&whatsapp=021-5551234&store_name=TB+Maju&consent=true",
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(!reply.body.contains("<script>x"), "{}", reply.body);
    assert!(
        reply.body.contains("&#60;script&#62;x&#60;/script&#62;"),
        "{}",
        reply.body
    );
    assert_eq!(lead_count(&db).await, 0);
}

#[tokio::test]
async fn honeypot_filled_stores_nothing() {
    let db = Db::open_in_memory().unwrap();
    let reply = post(
        &db,
        "name=Pak+Darto&whatsapp=0812-3456-7890&store_name=TB+Maju&consent=true&website=http%3A%2F%2Fspam",
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK);
    assert!(reply.body.contains("Terima kasih!"), "{}", reply.body);
    assert_eq!(lead_count(&db).await, 0);
}

#[tokio::test]
async fn thanks_page_renders() {
    let db = Db::open_in_memory().unwrap();
    let reply = send(&db, Request::get(THANKS).body(Body::empty()).unwrap()).await;

    assert_eq!(reply.status, StatusCode::OK);
    assert!(
        reply.body.contains("<h1>Terima kasih!</h1>"),
        "{}",
        reply.body
    );
    assert!(
        reply.body.contains(
            "Kami akan mengabari Anda lewat WhatsApp saat SistemTokoBangunan siap dicoba."
        ),
        "{}",
        reply.body
    );
    assert!(reply.body.contains(r#"href="/""#), "{}", reply.body);
    assert!(
        reply
            .body
            .contains(r#"<meta name="robots" content="noindex">"#),
        "{}",
        reply.body
    );
}

#[tokio::test]
async fn undecodable_form_rerenders_empty_form() {
    let db = Db::open_in_memory().unwrap();
    // A browser-default checkbox value: form decoding accepts only `true`/`false` for a bool.
    let reply = post(
        &db,
        "name=Pak+Darto&whatsapp=0812-3456-7890&store_name=TB+Maju&consent=on",
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        reply.body.contains(r#"action="/waitlist""#),
        "{}",
        reply.body
    );
    assert!(!reply.body.contains("field-error"), "{}", reply.body);
    assert!(
        reply.body.contains(r#"name="name" value="""#),
        "{}",
        reply.body
    );
    assert_eq!(lead_count(&db).await, 0);
}

#[tokio::test]
async fn wrong_content_type_rerenders_empty_form() {
    let db = Db::open_in_memory().unwrap();
    let request = Request::post("/waitlist")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(valid_form("0812-3456-7890")))
        .unwrap();
    let reply = send(&db, request).await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        reply.body.contains(r#"action="/waitlist""#),
        "{}",
        reply.body
    );
    assert!(!reply.body.contains("field-error"), "{}", reply.body);
    assert_eq!(lead_count(&db).await, 0);
}

#[tokio::test]
async fn database_failure_shows_error_page() {
    let db = Db::open_in_memory().unwrap();
    db.call(|conn| conn.execute_batch("DROP TABLE lead"))
        .await
        .unwrap();
    let reply = post(&db, &valid_form("0812-3456-7890")).await;

    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        reply
            .body
            .contains("Maaf, terjadi gangguan. Silakan coba lagi sebentar lagi."),
        "{}",
        reply.body
    );
    assert!(!reply.body.contains("Pak Darto"), "{}", reply.body);
}
