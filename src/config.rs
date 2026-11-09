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
    /// `STB_DATABASE_PATH`, `STB_BASE_URL` and `STB_WHATSAPP_NUMBER` are required; an empty or
    /// whitespace-only value counts as missing. `STB_BIND` defaults to `127.0.0.1:8080`.
    /// Trailing `/` are removed from the base URL; nothing left (e.g. `/`) is invalid.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Result<Config, ConfigError> {
        let required = |name: &'static str| {
            get(name)
                .filter(|value| !value.trim().is_empty())
                .ok_or(ConfigError::Missing(name))
        };

        let bind = get("STB_BIND")
            .unwrap_or_else(|| DEFAULT_BIND.to_string())
            .parse()
            .map_err(|_| ConfigError::Invalid("STB_BIND"))?;
        let database_path = PathBuf::from(required("STB_DATABASE_PATH")?);
        let base_url = required("STB_BASE_URL")?.trim_end_matches('/').to_string();
        if base_url.trim().is_empty() {
            return Err(ConfigError::Invalid("STB_BASE_URL"));
        }
        let whatsapp_number = required("STB_WHATSAPP_NUMBER")?;

        Ok(Config {
            bind,
            database_path,
            base_url,
            whatsapp_number,
        })
    }

    /// Reads the configuration from the process environment.
    pub fn from_env() -> Result<Config, ConfigError> {
        Self::from_vars(|name| std::env::var(name).ok())
    }
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
        ]))
        .unwrap();
        assert_eq!(config.bind, "127.0.0.1:8080".parse().unwrap());
        assert_eq!(config.database_path, PathBuf::from("/tmp/x.db"));
        assert_eq!(config.base_url, "https://sistemtokobangunan.com");
        assert_eq!(config.whatsapp_number, "6281234567890");
    }

    #[test]
    fn bind_defaults_to_localhost_8080() {
        let config = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
        ]))
        .unwrap();
        assert_eq!(config.bind, "127.0.0.1:8080".parse().unwrap());
    }

    #[test]
    fn missing_database_path_is_error() {
        let result = Config::from_vars(vars(&[
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_DATABASE_PATH")));
    }

    #[test]
    fn base_url_trailing_slash_removed() {
        let config = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://x.com/"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
        ]))
        .unwrap();
        assert_eq!(config.base_url, "https://x.com");
    }

    #[test]
    fn missing_base_url_is_error() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_BASE_URL")));
    }

    #[test]
    fn missing_whatsapp_number_is_error() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
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
        ]));
        assert_eq!(result, Err(ConfigError::Invalid("STB_BIND")));
    }

    #[test]
    fn empty_database_path_is_missing() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", ""),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_DATABASE_PATH")));
    }

    #[test]
    fn whitespace_only_values_are_missing() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "  "),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_BASE_URL")));

        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "https://sistemtokobangunan.com"),
            ("STB_WHATSAPP_NUMBER", " \t"),
        ]));
        assert_eq!(result, Err(ConfigError::Missing("STB_WHATSAPP_NUMBER")));
    }

    #[test]
    fn base_url_only_slash_is_invalid() {
        let result = Config::from_vars(vars(&[
            ("STB_DATABASE_PATH", "/tmp/x.db"),
            ("STB_BASE_URL", "/"),
            ("STB_WHATSAPP_NUMBER", "6281234567890"),
        ]));
        assert_eq!(result, Err(ConfigError::Invalid("STB_BASE_URL")));
    }
}
