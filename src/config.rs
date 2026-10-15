use std::{fs, io, net::SocketAddr, path::Path};

use ipnet::IpNet;
use serde::Deserialize;

pub const DEFAULT_PATH: &str = "/etc/ipv6who/config.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Socket addresses to listen on.
    pub listen: Vec<SocketAddr>,
    /// Client networks allowed to connect. Everything else is dropped at accept time.
    pub allow: Vec<IpNet>,
    /// Optional bearer token required for /v1/* endpoints.
    pub token: Option<String>,
    /// Report privacy-extension temporary addresses as well.
    pub include_temporary: bool,
    pub interfaces: InterfaceFilter,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: vec!["[::]:8066".parse().unwrap()],
            allow: vec!["::1/128".parse().unwrap(), "127.0.0.0/8".parse().unwrap()],
            token: None,
            include_temporary: false,
            interfaces: InterfaceFilter::default(),
        }
    }
}

/// Interface name patterns. A trailing `*` matches any suffix.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct InterfaceFilter {
    /// If non-empty, only these interfaces are considered.
    pub include: Vec<String>,
    /// Interfaces that are never considered. Takes precedence over `include`.
    pub exclude: Vec<String>,
}

impl InterfaceFilter {
    pub fn matches(&self, name: &str) -> bool {
        if self.exclude.iter().any(|p| pattern_matches(p, name)) {
            return false;
        }
        self.include.is_empty() || self.include.iter().any(|p| pattern_matches(p, name))
    }
}

fn pattern_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Parse(toml::de::Error),
    Invalid(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Parse(e) => write!(f, "{e}"),
            Error::Invalid(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {}

impl Config {
    /// Loads the config file. A missing file yields the (loopback-only) defaults.
    pub fn load(path: &Path) -> Result<(Self, bool), Error> {
        match fs::read_to_string(path) {
            Ok(text) => Ok((Self::parse(&text)?, true)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok((Self::default(), false)),
            Err(e) => Err(Error::Io(e)),
        }
    }

    pub fn parse(text: &str) -> Result<Self, Error> {
        let config: Config = toml::from_str(text).map_err(Error::Parse)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), Error> {
        if self.listen.is_empty() {
            return Err(Error::Invalid(
                "`listen` must contain at least one address".into(),
            ));
        }
        if self.token.as_deref() == Some("") {
            return Err(Error::Invalid(
                "`token` must not be empty; omit it to disable".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        let c = Config::parse("").unwrap();
        assert_eq!(c.listen, Config::default().listen);
        assert_eq!(c.allow.len(), 2);
        assert!(!c.include_temporary);
    }

    #[test]
    fn full_file() {
        let c = Config::parse(
            r#"
            listen = ["[::]:9000", "127.0.0.1:9001"]
            allow = ["2001:db8::/32", "10.0.0.0/8"]
            token = "secret"
            include_temporary = true
            [interfaces]
            include = ["eth*"]
            exclude = ["eth9"]
            "#,
        )
        .unwrap();
        assert_eq!(c.listen.len(), 2);
        assert_eq!(c.token.as_deref(), Some("secret"));
        assert!(c.interfaces.matches("eth0"));
        assert!(!c.interfaces.matches("eth9"));
        assert!(!c.interfaces.matches("wlan0"));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Config::parse("allow = [\"not-a-cidr\"]").is_err());
        assert!(Config::parse("listen = []").is_err());
        assert!(Config::parse("token = \"\"").is_err());
        assert!(Config::parse("unknown_key = 1").is_err());
    }
}
