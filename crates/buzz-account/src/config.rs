//! Environment-only configuration. Every value is read once at startup and a
//! missing or malformed required value aborts the process with a named error.

use std::{collections::HashMap, net::SocketAddr};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use thiserror::Error;

/// Validated service configuration.
#[derive(Clone)]
pub struct Config {
    /// Address the HTTP server listens on. `ACCOUNT_BIND_ADDR`, default `0.0.0.0:3100`.
    pub bind_addr: SocketAddr,
    /// Postgres URL of the account service's own database. `ACCOUNT_DATABASE_URL`.
    pub database_url: String,
    /// Relay base URL reachable from this service (Docker-internal). `ACCOUNT_RELAY_URL`.
    pub relay_url: url::Url,
    /// Nostr key allow-listed in the relay's `RELAY_OPERATOR_PUBKEYS`. `ACCOUNT_OPERATOR_SECRET_KEY` (64 hex).
    pub operator_keys: nostr::Keys,
    /// 32-byte key that encrypts stored identity keys. `ACCOUNT_MASTER_KEY` (base64).
    pub master_key: [u8; 32],
    /// Resend API key. `RESEND_API_KEY`.
    pub resend_api_key: String,
    /// Resend API base. `RESEND_BASE_URL`, default `https://api.resend.com/`; tests point it at a fake.
    pub resend_base_url: url::Url,
    /// Sender address for login codes. `ACCOUNT_EMAIL_FROM`.
    pub email_from: String,
    /// Domain suffix communities are created under, e.g. `app.pegboard.me`. `ACCOUNT_COMMUNITY_DOMAIN`.
    pub community_domain: String,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("bind_addr", &self.bind_addr)
            .field("relay_url", &self.relay_url.as_str())
            .field("operator_pubkey", &self.operator_keys.public_key())
            .field("email_from", &self.email_from)
            .field("community_domain", &self.community_domain)
            .finish_non_exhaustive()
    }
}

/// Startup configuration failure, naming the offending variable.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid environment variable {0}: {1}")]
    Invalid(&'static str, String),
}

impl Config {
    /// Read configuration from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_map(&std::env::vars().collect())
    }

    /// Read configuration from an explicit map (testable seam for `from_env`).
    pub fn from_map(env: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let required = |name: &'static str| -> Result<&str, ConfigError> {
            env.get(name)
                .map(String::as_str)
                .filter(|v| !v.trim().is_empty())
                .ok_or(ConfigError::Missing(name))
        };
        let invalid = |name: &'static str| {
            move |e: &dyn std::fmt::Display| ConfigError::Invalid(name, e.to_string())
        };

        let bind_addr = env
            .get("ACCOUNT_BIND_ADDR")
            .map(String::as_str)
            .unwrap_or("0.0.0.0:3100")
            .parse()
            .map_err(|e| invalid("ACCOUNT_BIND_ADDR")(&e))?;

        let relay_url = url::Url::parse(required("ACCOUNT_RELAY_URL")?)
            .map_err(|e| invalid("ACCOUNT_RELAY_URL")(&e))?;

        let operator_keys =
            nostr::Keys::parse(required("ACCOUNT_OPERATOR_SECRET_KEY")?).map_err(|_| {
                invalid("ACCOUNT_OPERATOR_SECRET_KEY")(&"expected a 64-hex or nsec secret key")
            })?;

        let resend_base_url = url::Url::parse(
            env.get("RESEND_BASE_URL")
                .map(String::as_str)
                .unwrap_or("https://api.resend.com/"),
        )
        .map_err(|e| invalid("RESEND_BASE_URL")(&e))?;

        let master_key = STANDARD
            .decode(required("ACCOUNT_MASTER_KEY")?)
            .map_err(|e| invalid("ACCOUNT_MASTER_KEY")(&e))?;
        let master_key: [u8; 32] = master_key
            .try_into()
            .map_err(|_| invalid("ACCOUNT_MASTER_KEY")(&"expected exactly 32 bytes (base64)"))?;

        let community_domain = required("ACCOUNT_COMMUNITY_DOMAIN")?
            .trim()
            .trim_matches('.')
            .to_ascii_lowercase();
        if community_domain.is_empty() || community_domain.contains(['/', ':', '*']) {
            return Err(invalid("ACCOUNT_COMMUNITY_DOMAIN")(
                &"expected a bare host such as app.example.com",
            ));
        }

        Ok(Self {
            bind_addr,
            database_url: required("ACCOUNT_DATABASE_URL")?.to_owned(),
            relay_url,
            operator_keys,
            master_key,
            resend_api_key: required("RESEND_API_KEY")?.to_owned(),
            resend_base_url,
            email_from: required("ACCOUNT_EMAIL_FROM")?.to_owned(),
            community_domain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> HashMap<String, String> {
        [
            (
                "ACCOUNT_DATABASE_URL",
                "postgres://buzz:pw@postgres:5432/buzz_account",
            ),
            ("ACCOUNT_RELAY_URL", "http://relay:3000"),
            (
                "ACCOUNT_OPERATOR_SECRET_KEY",
                "1111111111111111111111111111111111111111111111111111111111111111",
            ),
            ("ACCOUNT_MASTER_KEY", &STANDARD.encode([7u8; 32])),
            ("RESEND_API_KEY", "re_test"),
            ("ACCOUNT_EMAIL_FROM", "noreply@example.com"),
            ("ACCOUNT_COMMUNITY_DOMAIN", "App.Example.com."),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
    }

    #[test]
    fn parses_a_complete_environment() {
        let c = Config::from_map(&valid()).expect("valid config");
        assert_eq!(c.bind_addr.port(), 3100);
        assert_eq!(c.master_key, [7u8; 32]);
        assert_eq!(c.community_domain, "app.example.com");
        assert_eq!(c.relay_url.as_str(), "http://relay:3000/");
    }

    #[test]
    fn every_required_variable_is_named_when_missing() {
        for name in [
            "ACCOUNT_DATABASE_URL",
            "ACCOUNT_RELAY_URL",
            "ACCOUNT_OPERATOR_SECRET_KEY",
            "ACCOUNT_MASTER_KEY",
            "RESEND_API_KEY",
            "ACCOUNT_EMAIL_FROM",
            "ACCOUNT_COMMUNITY_DOMAIN",
        ] {
            let mut env = valid();
            env.remove(name);
            assert_eq!(
                Config::from_map(&env).err(),
                Some(ConfigError::Missing(name))
            );
        }
    }

    #[test]
    fn rejects_master_key_of_wrong_length() {
        let mut env = valid();
        env.insert("ACCOUNT_MASTER_KEY".into(), STANDARD.encode([1u8; 16]));
        assert!(matches!(
            Config::from_map(&env),
            Err(ConfigError::Invalid("ACCOUNT_MASTER_KEY", _))
        ));
    }

    #[test]
    fn rejects_malformed_operator_key() {
        let mut env = valid();
        env.insert("ACCOUNT_OPERATOR_SECRET_KEY".into(), "not-a-key".into());
        assert!(matches!(
            Config::from_map(&env),
            Err(ConfigError::Invalid("ACCOUNT_OPERATOR_SECRET_KEY", _))
        ));
    }

    #[test]
    fn debug_output_never_contains_secrets() {
        let c = Config::from_map(&valid()).expect("valid config");
        let rendered = format!("{c:?}");
        assert!(!rendered.contains("re_test"));
        assert!(!rendered.contains(&STANDARD.encode([7u8; 32])));
        assert!(!rendered.contains("1111111111111111"));
    }
}
