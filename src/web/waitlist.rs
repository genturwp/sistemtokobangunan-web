//! The waitlist form submission (RFC-0001 phase 0): `POST /waitlist` and the thank-you page.

use askama::Template;
use axum::Form;
use axum::extract::State;
use axum::extract::rejection::FormRejection;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use super::AppState;
use super::pages::{HomeTemplate, WaitlistForm, render};
use crate::lead::{self, FieldError, WaitlistInput};

/// Where a successful sign-up is redirected.
pub const THANKS_PATH: &str = "/waitlist/terima-kasih";

/// The page shown when the sign-up could not be stored.
const SERVER_ERROR_PAGE: &str = "<!doctype html>\n<html lang=\"id\">\n<head>\n\
<meta charset=\"utf-8\">\n<title>Terjadi gangguan | SistemTokoBangunan</title>\n</head>\n\
<body>\n<p>Maaf, terjadi gangguan. Silakan coba lagi sebentar lagi.</p>\n</body>\n</html>\n";

/// The waitlist form as posted. An unticked checkbox is absent from the form, hence the
/// `default` on `consent`. Not `#[serde(flatten)]` over [`WaitlistInput`]: form decoding
/// cannot parse a flattened `bool`.
#[derive(Debug, Deserialize)]
pub struct WaitlistPost {
    pub name: String,
    pub whatsapp: String,
    pub store_name: String,
    pub email: Option<String>,
    #[serde(default)]
    pub consent: bool,
    pub source: Option<String>,
    /// Honeypot: hidden from people, filled in by bots.
    pub website: Option<String>,
}

impl WaitlistPost {
    /// True when the hidden honeypot field holds anything other than whitespace.
    fn is_bot(&self) -> bool {
        self.website
            .as_deref()
            .is_some_and(|website| !website.trim().is_empty())
    }
}

impl From<WaitlistPost> for WaitlistInput {
    fn from(post: WaitlistPost) -> Self {
        WaitlistInput {
            name: post.name,
            whatsapp: post.whatsapp,
            store_name: post.store_name,
            email: post.email,
            consent: post.consent,
            source: post.source,
        }
    }
}

/// The thank-you page, shown after a sign-up (new or repeated) and to a bot.
#[derive(Template)]
#[template(path = "waitlist_thanks.html")]
pub struct ThanksTemplate;

/// `POST /waitlist`: validates and stores a sign-up, then redirects (303) to the thank-you
/// page. Invalid input re-renders the home page with 422; a body that cannot be decoded
/// re-renders the empty form with 422; a filled honeypot gets the thank-you page and nothing
/// is stored. A number already on the waitlist looks the same to the visitor as a new one.
pub async fn submit(
    State(state): State<AppState>,
    form: Result<Form<WaitlistPost>, FormRejection>,
) -> Response {
    let post = match form {
        Ok(Form(post)) => post,
        Err(_) => {
            // The rejection text may quote what was posted; log only that decoding failed.
            tracing::warn!("waitlist form could not be decoded");
            return rerender(&state, WaitlistForm::empty(None));
        }
    };

    if post.is_bot() {
        tracing::info!("waitlist honeypot filled; nothing stored");
        return render(&ThanksTemplate);
    }

    let input = WaitlistInput::from(post);
    let valid = match lead::validate(&input) {
        Ok(valid) => valid,
        Err(errors) => return rerender(&state, typed_form(input, errors)),
    };

    let now = match stored_time(OffsetDateTime::now_utc()) {
        Ok(now) => now,
        Err(error) => {
            tracing::error!(%error, "cannot format the current time");
            return server_error();
        }
    };

    match state
        .db
        .call(move |conn| lead::register_waitlist(conn, &valid, &now))
        .await
    {
        Ok(outcome) => {
            tracing::info!(?outcome, "waitlist sign-up");
            Redirect::to(THANKS_PATH).into_response()
        }
        Err(error) => {
            tracing::error!(%error, "cannot store waitlist sign-up");
            server_error()
        }
    }
}

/// `GET /waitlist/terima-kasih`: the thank-you page.
pub async fn thanks() -> Response {
    render(&ThanksTemplate)
}

/// The form as the visitor typed it, with its errors. Only a clean `source` is carried back
/// into the hidden field, as on the first view.
fn typed_form(mut values: WaitlistInput, errors: Vec<FieldError>) -> WaitlistForm {
    values.source = values.source.as_deref().and_then(lead::clean_source);
    WaitlistForm { values, errors }
}

/// The home page with `form`, answered with 422.
fn rerender(state: &AppState, form: WaitlistForm) -> Response {
    let mut response = render(&HomeTemplate {
        base_url: state.config.base_url.clone(),
        whatsapp_number: state.config.whatsapp_number.clone(),
        form,
    });
    if response.status() == StatusCode::OK {
        *response.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
    }
    response
}

/// `at` as stored in `lead.created_at` and `lead.consent_at`: RFC 3339 in whole seconds
/// (`2026-11-10T03:04:05Z`), so every stored time has the same shape and sorts as text.
fn stored_time(at: OffsetDateTime) -> Result<String, time::error::Format> {
    at.truncate_to_second().format(&Rfc3339)
}

fn server_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, Html(SERVER_ERROR_PAGE)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month, PrimitiveDateTime, Time};

    fn utc(nanosecond: u32) -> OffsetDateTime {
        let date = Date::from_calendar_date(2026, Month::November, 10).unwrap();
        let time = Time::from_hms_nano(3, 4, 5, nanosecond).unwrap();
        PrimitiveDateTime::new(date, time).assume_utc()
    }

    #[test]
    fn stored_time_drops_the_fraction() {
        assert_eq!(
            stored_time(utc(987_654_321)).unwrap(),
            "2026-11-10T03:04:05Z"
        );
        assert_eq!(stored_time(utc(0)).unwrap(), "2026-11-10T03:04:05Z");
    }
}
