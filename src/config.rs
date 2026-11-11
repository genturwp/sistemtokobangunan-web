//! Configuration read from `STB_*` environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Settings of a running `stb-web` process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Address and port the HTTP server listens on (`STB_BIND`).
    pub bind: SocketAddr,
    /// SQLite database file (`STB_DATABASE_PATH`).
    pub database_path: PathBuf,
    /// Public base URL without a trailing `/` (`STB_BASE_URL`).
    pub base_url: String,
    /// The owner's WhatsApp number for contact links (`STB_WHATSAPP_NUMBER`).
    pub whatsapp_number: String,
    /// The site's public contact email for `mailto:` links (`STB_CONTACT_EMAIL`).
    pub contact_email: String,
}

/// Why the configuration could not be read. Each variant names the variable.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("environment variable {0} is not set")]
    Missing(&'static str),
    #[error("environment variable {0} has an invalid value")]
    Invalid(&'static str),
}

/// Address used when `STB_BIND` is not set.
const DEFAULT_BIND: &str = "127.0.0.1:8080";

impl Config {
    /// Reads the configuration through `get`, which returns the value of a variable or `None`.
    ///
    /// `STB_DATABASE_PATH`, `STB_BASE_URL`, `STB_WHATSAPP_NUMBER` and `STB_CONTACT_EMAIL` are
    /// required; an empty or whitespace-only value counts as missing. `STB_BIND` defaults to
    /// `127.0.0.1:8080`. Trailing `/` are removed from the base URL; nothing left (e.g. `/`) is
    /// invalid. The WhatsApp number must be the digits of a `wa.me` link: `62` then 8 to 13
    /// digits, with no `+`, spaces or dashes; anything else is invalid. The contact email is
    /// trimmed and must then pass [`is_plain_email`]; anything else is invalid.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Config, ConfigError> {
        let required = |name: &'static str| required(&get, name);

        let bind = get("STB_BIND")
            .unwrap_or_else(|| DEFAULT_BIND.to_string())
            .parse()
            .map_err(|_| ConfigError::Invalid("STB_BIND"))?;
        let database_path = database_path_from_vars(&get)?;
        let base_url = required("STB_BASE_URL")?.trim_end_matches('/').to_string();
        if base_url.trim().is_empty() {
            return Err(ConfigError::Invalid("STB_BASE_URL"));
        }
        let whatsapp_number = required("STB_WHATSAPP_NUMBER")?;
        if !is_wa_me_number(&whatsapp_number) {
            return Err(ConfigError::Invalid("STB_WHATSAPP_NUMBER"));
        }
        let contact_email = required("STB_CONTACT_EMAIL")?.trim().to_string();
        if !is_plain_email(&contact_email) {
            return Err(ConfigError::Invalid("STB_CONTACT_EMAIL"));
        }

        Ok(Config {
            bind,
            database_path,
            base_url,
            whatsapp_number,
            contact_email,
        })
    }

    /// Reads the configuration from the process environment.
    pub fn from_env() -> Result<Config, ConfigError> {
        Self::from_vars(|name| std::env::var(name).ok())
    }
}

/// Reads only `STB_DATABASE_PATH` through `get`, with the same rule as [`Config::from_vars`]:
/// unset, empty or whitespace-only is [`ConfigError::Missing`]. Used by the owner's CLI
/// subcommands, which need no other setting.
pub fn database_path_from_vars(
    get: impl Fn(&str) -> Option<String>,
) -> Result<PathBuf, ConfigError> {
    required(&get, "STB_DATABASE_PATH").map(PathBuf::from)
}

/// Reads `STB_DATABASE_PATH` from the process environment (see [`database_path_from_vars`]).
pub fn database_path_from_env() -> Result<PathBuf, ConfigError> {
    database_path_from_vars(|name| std::env::var(name).ok())
}

/// True when `number` is `62` followed by 8 to 13 ASCII digits, as `https://wa.me/<number>`
/// links need it.
fn is_wa_me_number(number: &str) -> bool {
    number.strip_prefix("62").is_some_and(|rest| {
        (8..=13).contains(&rest.len()) && rest.bytes().all(|byte| byte.is_ascii_digit())
    })
}

/// Longest address accepted for `STB_CONTACT_EMAIL`, in characters.
const MAX_EMAIL_CHARS: usize = 254;

/// A deliberately simple check that `email` can go into a `mailto:` link: at most 254
/// characters, no whitespace, exactly one `@` with something on both sides, and a `.` in the
/// domain. It is a guard against typos and placeholders, not a full address parser.
fn is_plain_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    email.chars().count() <= MAX_EMAIL_CHARS
        && !email.chars().any(char::is_whitespace)
        && !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
}

/// The value of a required variable; unset, empty or whitespace-only counts as missing.
fn required(
    get: &impl Fn(&str) -> Option<String>,
    name: &'static str,
) -> Result<String, ConfigError> {
    get(name)
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn from_vars_reads_all_values() {
        let config = Config::from_vars(vars(&[
            ("STB_BIND", "127.0.0.1:8080"),
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]))
        .unwrap();
        assert_eq!(config.bind, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(config.database_path, PathBuf::from("/tmp/x.db"));
        assert_eq!(config.base_url, "https://sistemtokobangunan.com");
        assert_eq!(config.whatsapp_number, "6281234567890");
        assert_eq!(config.contact_email, "sistemtokobangunan@gmail.com");
    }

    #[test]
    fn bind_defaults_to_localhost_8080() {
        let config = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]))
        .unwrap();
        assert_eq!(config.bind, "127.0.0.1:8080".parse().unwrap());
    }

    #[test]
    fn missing_database_path_is_error() {
        let result = Config::from_vars(vars(&[
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_DATABASE_PATH")));
    }

    #[test]
    fn base_url_trailing_slash_removed() {
        let config = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://x.com/"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]))
        .unwrap();
        assert_eq!(config.base_url, "https://x.com");
    }

    #[test]
    fn missing_base_url_is_error() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_BASE_URL")));
    }

    #[test]
    fn missing_whatsapp_number_is_error() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_WHATSAPP_NUMBER")));
    }

    #[test]
    fn unparsable_bind_is_invalid() {
        let result = Config::from_vars(vars(&[
            ("STB_BIND", "not-an-address"),
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]));
        assert_eq!(result, Err(ConfigError::Invalid("STB_BIND")));
    }

    #[test]
    fn empty_database_path_is_missing() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", ""),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_DATABASE_PATH")));
    }

    #[test]
    fn whitespace_only_values_are_missing() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "  "),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_BASE_URL")));

        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", " \t"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_WHATSAPP_NUMBER")));
    }

    #[test]
    fn database_path_from_vars_needs_only_that_variable() {
        let path = database_path_from_vars(vars(&[("STB_DATABASE_PATH", "/tmp/x.db")])).unwrap();
        assert_eq!(path, PathBuf::from("/tmp/x.db"));
    }

    #[test]
    fn database_path_from_vars_missing_empty_or_blank_is_missing() {
        for pairs in [
            &[][..],
            &[("STB_DATABASE_PATH", "")][..],
            &[("STB_DATABASE_PATH", " \t")][..],
        ] {
            assert_eq!(
                database_path_from_vars(vars(pairs)),
                Err(ConfigError::Missing("STB_DATABASE_PATH")),
                "for {pairs:?}"
            );
        }
    }

    fn with_whatsapp(number: &str) -> Result<Config, ConfigError> {
        Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", number),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]))
    }

    #[test]
    fn whatsapp_number_62_then_8_to_13_digits_is_accepted() {
        for number in ["6281234567890", "6281234567", "628123456789012"] {
            assert_eq!(
                with_whatsapp(number).map(|config| config.whatsapp_number),
                Ok(number.to_string()),
                "for {number}"
            );
        }
    }

    #[test]
    fn whatsapp_number_not_wa_me_digits_is_invalid() {
        for number in [
            "+6281234567890",
            "081234567890",
            "REPLACE_WITH_DIGITS_ONLY_NUMBER",
            "62 812 3456 7890",
            "62812-3456-7890",
            " 6281234567890",
            "62812345",
            "6281234567890123",
            "62",
        ] {
            assert_eq!(
                with_whatsapp(number),
                Err(ConfigError::Invalid("STB_WHATSAPP_NUMBER")),
                "for {number:?}"
            );
        }
    }

    fn with_email(email: &str) -> Result<Config, ConfigError> {
        Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", email),
        ]))
    }

    #[test]
    fn missing_contact_email_is_error() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_CONTACT_EMAIL")));
    }

    #[test]
    fn empty_or_blank_contact_email_is_missing() {
        for email in ["", "  ", " \t"] {
            assert_eq!(
                with_email(email),
                Err(ConfigError::Missing("STB_CONTACT_EMAIL")),
                "for {email:?}"
            );
        }
    }

    #[test]
    fn contact_email_is_accepted_and_trimmed() {
        for (email, expected) in [
            (
                "sistemtokobangunan@gmail.com",
                "sistemtokobangunan@gmail.com",
            ),
            ("halo@toko.co.id", "halo@toko.co.id"),
            (
                " sistemtokobangunan@gmail.com\t",
                "sistemtokobangunan@gmail.com",
            ),
        ] {
            assert_eq!(
                with_email(email).map(|config| config.contact_email),
                Ok(expected.to_string()),
                "for {email:?}"
            );
        }
    }

    #[test]
    fn contact_email_up_to_254_characters_is_accepted() {
        let email = format!("{}@gmail.com", "a".repeat(244));
        assert_eq!(email.len(), 254);
        assert_eq!(
            with_email(&email).map(|config| config.contact_email),
            Ok(email)
        );
    }

    #[test]
    fn contact_email_not_an_address_is_invalid() {
        let too_long = format!("{}@gmail.com", "a".repeat(245));
        for email in [
            "REPLACE_WITH_CONTACT_EMAIL",
            "sistemtokobangunan.gmail.com",
            "@gmail.com",
            "sistemtokobangunan@",
            "sistemtokobangunan@gmail",
            "a@b@gmail.com",
            "sistem toko@gmail.com",
            "sistemtokobangunan@gmail .com",
            "sistemtokobangunan@\tgmail.com",
            too_long.as_str(),
        ] {
            assert_eq!(
                with_email(email),
                Err(ConfigError::Invalid("STB_CONTACT_EMAIL")),
                "for {email:?}"
            );
        }
    }

    #[test]
    fn base_url_only_slash_is_invalid() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "/"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
            ("STB_CONTACT_EMAIL", "sistemtokobangunan@gmail.com"),
        ]));
        assert_eq!(result, Err(ConfigError::Invalid("STB_BASE_URL")));
    }
}
