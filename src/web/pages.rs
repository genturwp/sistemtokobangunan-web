//! Public pages: the pre-launch home page (Beranda, layout B2-A) and the privacy page
//! (RFC-0001, *Website functions* 1).

use askama::Template;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use super::AppState;
use crate::lead::{self, FieldError, WaitlistInput};

/// The waitlist form as shown on the home page: what the visitor typed and what was wrong with
/// it. Empty on first view; Task 4 re-renders it with the posted values and errors.
#[derive(Debug, Clone, Default)]
pub struct WaitlistForm {
    pub values: WaitlistInput,
    pub errors: Vec<FieldError>,
}

impl WaitlistForm {
    /// A blank form carrying the visit's cleaned `utm_source`, if any.
    pub fn empty(source: Option<String>) -> Self {
        Self {
            values: WaitlistInput {
                source,
                ..WaitlistInput::default()
            },
            errors: Vec::new(),
        }
    }

    /// The message to show next to `field`, if that field has an error.
    pub fn error_for(&self, field: &str) -> Option<&'static str> {
        self.errors
            .iter()
            .find(|error| error.field == field)
            .map(|error| error.message)
    }
}

/// The home page with the waitlist form.
#[derive(Template)]
#[template(path = "home.html")]
pub struct HomeTemplate {
    /// Public base URL without a trailing `/`.
    pub base_url: String,
    /// The owner's WhatsApp number for the `wa.me` link.
    pub whatsapp_number: String,
    /// The site's contact email for the `mailto:` link.
    pub contact_email: String,
    pub form: WaitlistForm,
}

/// The privacy page.
#[derive(Template)]
#[template(path = "privacy.html")]
pub struct PrivacyTemplate {
    pub base_url: String,
    pub whatsapp_number: String,
    pub contact_email: String,
    pub retention_months: u32,
}

/// The page for an address that does not exist.
#[derive(Template)]
#[template(path = "not_found.html")]
pub struct NotFoundTemplate;

/// Query parameters the home page reads. Anything else in the query is ignored.
#[derive(Debug, Default, Deserialize)]
pub struct HomeQuery {
    pub utm_source: Option<String>,
}

/// `GET /`: the pre-launch home page. A clean `utm_source` is carried into the form's hidden
/// `source` field; a malformed query is ignored rather than refused.
pub async fn home(
    State(state): State<AppState>,
    query: Result<Query<HomeQuery>, QueryRejection>,
) -> Response {
    let source = query
        .ok()
        .and_then(|Query(query)| query.utm_source)
        .and_then(|raw| lead::clean_source(&raw));
    render(&HomeTemplate {
        base_url: state.config.base_url.clone(),
        whatsapp_number: state.config.whatsapp_number.clone(),
        contact_email: state.config.contact_email.clone(),
        form: WaitlistForm::empty(source),
    })
}

/// `GET /privasi`: what is collected from the waitlist, why, and for how long.
pub async fn privacy(State(state): State<AppState>) -> Response {
    render(&PrivacyTemplate {
        base_url: state.config.base_url.clone(),
        whatsapp_number: state.config.whatsapp_number.clone(),
        contact_email: state.config.contact_email.clone(),
        retention_months: lead::LEAD_RETENTION_MONTHS,
    })
}

/// Fallback for every unknown path: the not-found page, answered with 404.
pub async fn not_found() -> Response {
    let mut response = render(&NotFoundTemplate);
    if response.status() == StatusCode::OK {
        *response.status_mut() = StatusCode::NOT_FOUND;
    }
    response
}

/// Renders a template as an HTML response, or a bare 500 if rendering fails.
pub fn render(template: &impl Template) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!(%error, "template rendering failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_form_keeps_only_source() {
        let form = WaitlistForm::empty(Some("instagram".into()));
        assert_eq!(form.values.source.as_deref(), Some("instagram"));
        assert_eq!(form.values.name, "");
        assert!(!form.values.consent);
        assert!(form.errors.is_empty());
        assert_eq!(WaitlistForm::empty(None).values.source, None);
    }

    #[test]
    fn error_for_finds_the_field_message() {
        let form = WaitlistForm {
            values: WaitlistInput::default(),
            errors: vec![
                FieldError {
                    field: "whatsapp",
                    message: "Nomor WhatsApp tidak valid",
                },
                FieldError {
                    field: "consent",
                    message: "Centang persetujuan untuk melanjutkan",
                },
            ],
        };
        assert_eq!(
            form.error_for("whatsapp"),
            Some("Nomor WhatsApp tidak valid")
        );
        assert_eq!(
            form.error_for("consent"),
            Some("Centang persetujuan untuk melanjutkan")
        );
        assert_eq!(form.error_for("name"), None);
    }

    fn home(form: WaitlistForm) -> String {
        HomeTemplate {
            base_url: "https://example.test".into(),
            whatsapp_number: "6281234567890".into(),
            contact_email: "kontak@example.test".into(),
            form,
        }
        .render()
        .unwrap()
    }

    #[test]
    fn home_rerenders_values_escaped_with_errors() {
        let html = home(WaitlistForm {
            values: WaitlistInput {
                name: "<b>Budi</b>".into(),
                whatsapp: "021-555".into(),
                store_name: "TB \"Maju\" & Co".into(),
                email: Some("budi@example.co.id".into()),
                consent: true,
                source: Some("instagram".into()),
            },
            errors: vec![FieldError {
                field: "whatsapp",
                message: "Nomor WhatsApp tidak valid",
            }],
        });
        assert!(
            html.contains("&#60;b&#62;Budi&#60;/b&#62;")
                || html.contains("&lt;b&gt;Budi&lt;/b&gt;"),
            "{html}"
        );
        assert!(!html.contains("<b>Budi</b>"));
        assert!(html.contains(r#"value="021-555""#));
        assert!(!html.contains(r#"value="TB "Maju""#));
        assert!(html.contains(r#"value="budi@example.co.id""#));
        assert!(html.contains("Nomor WhatsApp tidak valid"));
        assert!(html.contains(r#"value="true" checked"#), "{html}");
        assert!(html.contains(r#"name="source" value="instagram""#));
    }

    #[test]
    fn empty_home_shows_no_errors_and_no_consent() {
        let html = home(WaitlistForm::empty(None));
        for message in [
            "Nama wajib diisi",
            "Nomor WhatsApp tidak valid",
            "Centang persetujuan untuk melanjutkan",
        ] {
            assert!(!html.contains(message), "{message}");
        }
        assert!(!html.contains("checked"));
        assert!(html.contains(r#"name="source" value="""#));
    }
}
