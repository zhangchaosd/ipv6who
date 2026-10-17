//! Service settings, all given on the command line.

use std::net::SocketAddr;

use clap::Args;
use ipnet::IpNet;

#[derive(Debug, Clone, Args)]
pub struct Config {
    /// Socket address to listen on; repeat for several. "[::]" also accepts IPv4.
    #[arg(
        short,
        long = "listen",
        value_name = "ADDR",
        default_value = "[::]:8066"
    )]
    pub listen: Vec<SocketAddr>,

    /// Client network allowed to connect (CIDR); repeat or separate with commas.
    /// Connections from anywhere else are closed right after accept.
    #[arg(
        short,
        long = "allow",
        value_name = "CIDR",
        value_delimiter = ',',
        default_values = ["::1/128", "127.0.0.0/8"]
    )]
    pub allow: Vec<IpNet>,

    /// Bearer token required for /v1/*. Prefer the environment variable:
    /// command lines are visible to every local user.
    #[arg(long, env = "IPV6WHO_TOKEN", hide_env_values = true, value_parser = non_empty)]
    pub token: Option<String>,

    /// Also report privacy-extension temporary addresses.
    #[arg(long)]
    pub include_temporary: bool,

    #[command(flatten)]
    pub interfaces: InterfaceFilter,
}

/// Interface name patterns. A trailing `*` matches any suffix.
#[derive(Debug, Clone, Default, Args)]
pub struct InterfaceFilter {
    /// Only consider these interfaces; repeat or separate with commas.
    #[arg(long = "interface", value_name = "NAME", value_delimiter = ',')]
    pub include: Vec<String>,

    /// Never consider these interfaces; takes precedence over --interface.
    #[arg(long = "exclude-interface", value_name = "NAME", value_delimiter = ',')]
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

fn non_empty(s: &str) -> Result<String, String> {
    if s.is_empty() {
        Err("must not be empty".into())
    } else {
        Ok(s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        config: Config,
    }

    fn parse(args: &[&str]) -> Result<Config, clap::Error> {
        Cli::try_parse_from(std::iter::once("ipv6who").chain(args.iter().copied()))
            .map(|c| c.config)
    }

    #[test]
    fn defaults() {
        let c = parse(&[]).unwrap();
        assert_eq!(c.listen, ["[::]:8066".parse().unwrap()]);
        assert_eq!(
            c.allow,
            [
                "::1/128".parse::<IpNet>().unwrap(),
                "127.0.0.0/8".parse().unwrap()
            ]
        );
        assert!(!c.include_temporary);
    }

    #[test]
    fn all_flags() {
        let c = parse(&[
            "--listen",
            "[::]:9000",
            "-l",
            "127.0.0.1:9001",
            "--allow",
            "2001:db8::/32,10.0.0.0/8",
            "-a",
            "192.168.0.0/16",
            "--token",
            "secret",
            "--include-temporary",
            "--interface",
            "eth*",
            "--exclude-interface",
            "eth9",
        ])
        .unwrap();
        assert_eq!(c.listen.len(), 2);
        assert_eq!(c.allow.len(), 3, "explicit --allow replaces the defaults");
        assert_eq!(c.token.as_deref(), Some("secret"));
        assert!(c.include_temporary);
        assert!(c.interfaces.matches("eth0"));
        assert!(!c.interfaces.matches("eth9"));
        assert!(!c.interfaces.matches("wlan0"));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&["--allow", "not-a-cidr"]).is_err());
        assert!(parse(&["--listen", "8066"]).is_err());
        assert!(parse(&["--token", ""]).is_err());
        assert!(parse(&["--config", "x.toml"]).is_err());
    }
}
