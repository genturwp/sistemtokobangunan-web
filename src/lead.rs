//! Leads (*Prospek*): waitlist sign-ups (RFC-0001 phase 0).

use rusqlite::{Connection, params};
use serde::Deserialize;

/// Months a lead that never buys is kept after sign-up (RFC-0001, open question 20).
pub const LEAD_RETENTION_MONTHS: u32 = 12;

/// The waitlist form as posted. Missing fields take their default, so an unticked consent
/// checkbox (which the browser does not send) is `false`. The checkbox must post the value
/// `true`: form decoding accepts only `true` and `false` for a `bool`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WaitlistInput {
    pub name: String,
    pub whatsapp: String,
    pub store_name: String,
    pub email: Option<String>,
    pub consent: bool,
    pub source: Option<String>,
}

/// A waitlist sign-up that passed [`validate`]: trimmed, number normalised to `+628…`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidWaitlist {
    pub name: String,
    pub whatsapp: String,
    pub store_name: String,
    pub email: Option<String>,
    pub consent: bool,
    pub source: Option<String>,
}

/// A validation failure on one form field, with the message shown to the visitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: &'static str,
}

/// What [`register_waitlist`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterOutcome {
    Created,
    AlreadyRegistered,
}

/// Longest `source` tag kept.
const SOURCE_MAX_CHARS: usize = 50;
/// Longest name and store name, in characters.
const NAME_MAX_CHARS: usize = 100;
/// Longest e-mail address, in characters.
const EMAIL_MAX_CHARS: usize = 254;

/// Keeps a `utm_source`-style tag, trimmed and ASCII-lowercased, only if it is then 1 to 50
/// characters of `[a-z0-9_-]`.
pub fn clean_source(raw: &str) -> Option<String> {
    let tag = raw.trim().to_ascii_lowercase();
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-');
    (!tag.is_empty() && tag.len() <= SOURCE_MAX_CHARS && tag.chars().all(allowed)).then_some(tag)
}

/// Validates and normalises the form. Every field is trimmed first. Returns every error, in
/// field order (name, whatsapp, store_name, email, consent). An invalid `source` is dropped
/// silently. Text is kept as typed: escaping is the job of whatever displays it.
pub fn validate(input: &WaitlistInput) -> Result<ValidWaitlist, Vec<FieldError>> {
    let mut errors = Vec::new();
    let mut fail = |field: &'static str, message: &'static str| {
        errors.push(FieldError { field, message });
    };

    let name = input.name.trim();
    match name.chars().count() {
        0 => fail("name", "Nama wajib diisi"),
        n if n > NAME_MAX_CHARS => fail("name", "Nama terlalu panjang"),
        _ => {}
    }

    let whatsapp = crate::phone::normalize_id_mobile(input.whatsapp.trim());
    if whatsapp.is_err() {
        fail("whatsapp", "Nomor WhatsApp tidak valid");
    }

    let store_name = input.store_name.trim();
    match store_name.chars().count() {
        0 => fail("store_name", "Nama toko wajib diisi"),
        n if n > NAME_MAX_CHARS => fail("store_name", "Nama toko terlalu panjang"),
        _ => {}
    }

    let email = input
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty());
    if email.is_some_and(|email| !is_plausible_email(email)) {
        fail("email", "Format email tidak valid");
    }

    if !input.consent {
        fail("consent", "Centang persetujuan untuk melanjutkan");
    }

    match whatsapp {
        Ok(whatsapp) if errors.is_empty() => Ok(ValidWaitlist {
            name: name.to_string(),
            whatsapp,
            store_name: store_name.to_string(),
            email: email.map(str::to_string),
            consent: true,
            source: input.source.as_deref().and_then(clean_source),
        }),
        _ => Err(errors),
    }
}

/// At most 254 characters, exactly one `@` with something before it, no whitespace, and a `.`
/// after the `@`.
fn is_plausible_email(email: &str) -> bool {
    email.chars().count() <= EMAIL_MAX_CHARS
        && !email.chars().any(char::is_whitespace)
        && email.matches('@').count() == 1
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'))
}

/// Stores a waitlist sign-up once per WhatsApp number. A number already on the waitlist is left
/// as it was (first sign-up wins) and reported as [`RegisterOutcome::AlreadyRegistered`].
/// `now_rfc3339` is recorded as both the consent time and the creation time.
pub fn register_waitlist(
    conn: &mut Connection,
    v: &ValidWaitlist,
    now_rfc3339: &str,
) -> rusqlite::Result<RegisterOutcome> {
    let inserted = conn.execute(
        "INSERT INTO lead (kind, name, whatsapp, store_name, email, consent_at, source, created_at)
         VALUES ('waitlist', ?1, ?2, ?3, ?4, ?5, ?6, ?5)
         ON CONFLICT (kind, whatsapp) DO NOTHING",
        params![
            v.name,
            v.whatsapp,
            v.store_name,
            v.email,
            now_rfc3339,
            v.source
        ],
    )?;
    Ok(if inserted == 1 {
        RegisterOutcome::Created
    } else {
        RegisterOutcome::AlreadyRegistered
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    const NOW: &str = "2026-11-09T03:04:05Z";

    fn good_input() -> WaitlistInput {
        WaitlistInput {
            name: "  Budi Santoso ".into(),
            whatsapp: "0812-3456-7890".into(),
            store_name: " TB Maju Jaya ".into(),
            email: None,
            consent: true,
            source: None,
        }
    }

    fn messages(errors: &[FieldError]) -> Vec<&'static str> {
        errors.iter().map(|e| e.message).collect()
    }

    #[test]
    fn valid_input_normalises() {
        let valid = validate(&good_input()).unwrap();
        assert_eq!(
            valid,
            ValidWaitlist {
                name: "Budi Santoso".into(),
                whatsapp: "+6281234567890".into(),
                store_name: "TB Maju Jaya".into(),
                email: None,
                consent: true,
                source: None,
            }
        );

        let blank_email = WaitlistInput {
            email: Some("   ".into()),
            ..good_input()
        };
        assert_eq!(validate(&blank_email).unwrap().email, None);

        let email = WaitlistInput {
            email: Some(" budi@example.co.id ".into()),
            ..good_input()
        };
        assert_eq!(
            validate(&email).unwrap().email,
            Some("budi@example.co.id".into())
        );
    }

    #[test]
    fn all_errors_returned_in_order() {
        let input = WaitlistInput {
            name: "   ".into(),
            whatsapp: "021-5551234".into(),
            consent: false,
            ..good_input()
        };
        let errors = validate(&input).unwrap_err();
        assert_eq!(
            messages(&errors),
            [
                "Nama wajib diisi",
                "Nomor WhatsApp tidak valid",
                "Centang persetujuan untuk melanjutkan",
            ]
        );
        assert_eq!(
            errors.iter().map(|e| e.field).collect::<Vec<_>>(),
            ["name", "whatsapp", "consent"]
        );
    }

    #[test]
    fn every_field_error_message() {
        let input = WaitlistInput {
            name: "x".repeat(101),
            whatsapp: String::new(),
            store_name: String::new(),
            email: Some("budi@example".into()),
            consent: false,
            source: None,
        };
        assert_eq!(
            messages(&validate(&input).unwrap_err()),
            [
                "Nama terlalu panjang",
                "Nomor WhatsApp tidak valid",
                "Nama toko wajib diisi",
                "Format email tidak valid",
                "Centang persetujuan untuk melanjutkan",
            ]
        );

        let long_store = WaitlistInput {
            store_name: "x".repeat(101),
            ..good_input()
        };
        assert_eq!(
            messages(&validate(&long_store).unwrap_err()),
            ["Nama toko terlalu panjang"]
        );
    }

    #[test]
    fn lengths_count_characters_not_bytes() {
        let input = WaitlistInput {
            name: "é".repeat(100),
            store_name: "ü".repeat(100),
            ..good_input()
        };
        assert!(validate(&input).is_ok());
    }

    #[test]
    fn email_rule() {
        let check = |email: &str| {
            validate(&WaitlistInput {
                email: Some(email.into()),
                ..good_input()
            })
            .map(|v| v.email)
        };
        assert_eq!(check("a@b.co"), Ok(Some("a@b.co".into())));
        let local = "a".repeat(254 - "@b.co".len());
        assert!(check(&format!("{local}@b.co")).is_ok());
        for bad in [
            "budi.example.com",
            "budi@@example.com",
            "bu@di@example.com",
            "budi @example.com",
            "budi@example",
            "budi.santoso@example",
            &format!("{local}a@b.co"),
        ] {
            let errors = check(bad).unwrap_err();
            assert_eq!(messages(&errors), ["Format email tidak valid"], "{bad:?}");
        }
    }

    #[test]
    fn email_without_local_part_rejected() {
        let input = WaitlistInput {
            email: Some("@example.com".into()),
            ..good_input()
        };
        assert_eq!(
            messages(&validate(&input).unwrap_err()),
            ["Format email tidak valid"]
        );
    }

    #[test]
    fn clean_source_lowercases() {
        assert_eq!(clean_source("Google"), Some("google".into()));
        assert_eq!(clean_source(" FB_Ads-2 "), Some("fb_ads-2".into()));
        assert_eq!(clean_source("Ünïcode"), None);
        let upper = WaitlistInput {
            source: Some("Instagram".into()),
            ..good_input()
        };
        assert_eq!(validate(&upper).unwrap().source, Some("instagram".into()));
    }

    #[test]
    fn script_text_kept_as_typed() {
        let input = WaitlistInput {
            name: "<script>alert(1)</script>".into(),
            store_name: "<b>Toko</b> & Co".into(),
            ..good_input()
        };
        let valid = validate(&input).unwrap();
        assert_eq!(valid.name, "<script>alert(1)</script>");
        assert_eq!(valid.store_name, "<b>Toko</b> & Co");
    }

    #[test]
    fn source_kept_only_when_clean() {
        assert_eq!(clean_source("google"), Some("google".into()));
        assert_eq!(clean_source(" fb_ads-2 "), Some("fb_ads-2".into()));
        assert_eq!(clean_source(&"a".repeat(50)), Some("a".repeat(50)));
        assert_eq!(clean_source(&"a".repeat(51)), None);
        assert_eq!(clean_source(""), None);
        assert_eq!(clean_source("a b"), None);
        assert_eq!(clean_source("<script>"), None);

        let dirty = WaitlistInput {
            source: Some("evil<script>".into()),
            ..good_input()
        };
        assert_eq!(validate(&dirty).unwrap().source, None);
        let clean = WaitlistInput {
            source: Some("instagram".into()),
            ..good_input()
        };
        assert_eq!(validate(&clean).unwrap().source, Some("instagram".into()));
    }

    #[tokio::test]
    async fn form_without_consent_or_email_deserializes() {
        use axum::extract::{Form, FromRequest};
        use axum::http::{Request, header};

        let request = Request::post("/")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(axum::body::Body::from(
                "name=Budi&whatsapp=0812-3456-7890&store_name=TB+Maju",
            ))
            .unwrap();
        let Form(input) = Form::<WaitlistInput>::from_request(request, &())
            .await
            .unwrap();
        assert_eq!(input.name, "Budi");
        assert!(!input.consent);
        assert_eq!(input.email, None);
        assert_eq!(input.source, None);
    }

    fn valid() -> ValidWaitlist {
        validate(&good_input()).unwrap()
    }

    #[tokio::test]
    async fn register_creates_row() {
        let db = Db::open_in_memory().unwrap();
        let outcome = db
            .call(|conn| register_waitlist(conn, &valid(), NOW))
            .await
            .unwrap();
        assert_eq!(outcome, RegisterOutcome::Created);

        let row: (String, String, String, String, String, String) = db
            .call(|conn| {
                conn.query_row(
                    "SELECT kind, status, name, whatsapp, consent_at, created_at FROM lead",
                    [],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                        ))
                    },
                )
            })
            .await
            .unwrap();
        assert_eq!(
            row,
            (
                "waitlist".into(),
                "baru".into(),
                "Budi Santoso".into(),
                "+6281234567890".into(),
                NOW.into(),
                NOW.into(),
            )
        );
    }

    #[tokio::test]
    async fn register_twice_keeps_one_row() {
        let db = Db::open_in_memory().unwrap();
        let first = db
            .call(|conn| register_waitlist(conn, &valid(), NOW))
            .await
            .unwrap();
        // Same number typed another way, different name: still the same lead.
        let again = validate(&WaitlistInput {
            name: "Orang Lain".into(),
            whatsapp: "+62 812 3456 7890".into(),
            ..good_input()
        })
        .unwrap();
        let second = db
            .call(move |conn| register_waitlist(conn, &again, "2026-11-10T00:00:00Z"))
            .await
            .unwrap();
        assert_eq!(first, RegisterOutcome::Created);
        assert_eq!(second, RegisterOutcome::AlreadyRegistered);

        let (count, name, created_at): (i64, String, String) = db
            .call(|conn| {
                conn.query_row(
                    "SELECT count(*), min(name), min(created_at) FROM lead",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
            })
            .await
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(name, "Budi Santoso");
        assert_eq!(created_at, NOW);
    }

    #[tokio::test]
    async fn register_stores_email_and_source() {
        let db = Db::open_in_memory().unwrap();
        let v = validate(&WaitlistInput {
            email: Some("budi@example.co.id".into()),
            source: Some("google".into()),
            ..good_input()
        })
        .unwrap();
        db.call(move |conn| register_waitlist(conn, &v, NOW))
            .await
            .unwrap();
        let row: (Option<String>, Option<String>, Option<String>) = db
            .call(|conn| {
                conn.query_row("SELECT email, source, affiliate_code FROM lead", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
            })
            .await
            .unwrap();
        assert_eq!(
            row,
            (
                Some("budi@example.co.id".into()),
                Some("google".into()),
                None
            )
        );
    }
}
