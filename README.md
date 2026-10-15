# ipv6who

English | [简体中文](README.zh-CN.md)

A small HTTP service that reports the global IPv6 address of the host, with an IP allowlist.

Addresses are read from the kernel over rtnetlink (Linux only). Only global
unicast addresses (`2000::/3`) are reported; unique-local (`fc00::/7`),
link-local (`fe80::/10`), tentative and DAD-failed addresses are skipped, and
privacy-extension temporary addresses are skipped unless enabled.

## Install

Requires Linux with systemd and Rust 1.88 or newer (Fedora: `sudo dnf install cargo git`).

```sh
git clone https://github.com/zhangchaosd/ipv6who.git
cd ipv6who
cargo build --release --locked
sudo install -Dm755 target/release/ipv6who /usr/local/bin/ipv6who
sudo install -Dm644 dist/config.toml /etc/ipv6who/config.toml
sudo install -Dm644 dist/ipv6who.service /etc/systemd/system/ipv6who.service
```

Edit `/etc/ipv6who/config.toml` and add the client networks that may query the
service to `allow`, then start it:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now ipv6who
curl -s 'http://[::1]:8066/v1/ipv6'
```

If firewalld is active, open the port:
`sudo firewall-cmd --permanent --add-port=8066/tcp && sudo firewall-cmd --reload`.

### Runs unprivileged

`sudo` is only used to copy files. The service runs with `DynamicUser=yes`: a
transient system user with no capabilities, a read-only view of the file
system, and a sandbox that only allows TCP and netlink sockets. Reading
addresses over netlink needs no privileges, so the listen port must be 1024 or
higher. Because the transient user must be able to read the config file, it is
world-readable; keep that in mind if you set a `token`.

### Uninstall

```sh
sudo systemctl disable --now ipv6who
sudo rm -r /usr/local/bin/ipv6who /etc/systemd/system/ipv6who.service /etc/ipv6who
sudo systemctl daemon-reload
```

## API

`GET /v1/ipv6`

```json
{
  "address": "2001:db8::10",
  "interface": "eth0",
  "type": "static",
  "addresses": [
    {
      "address": "2001:db8::10",
      "prefix_len": 64,
      "interface": "eth0",
      "type": "static",
      "scope": "global",
      "deprecated": false,
      "stable_privacy": false,
      "preferred_lifetime": null,
      "valid_lifetime": null
    }
  ]
}
```

- `address` / `interface` / `type` describe the preferred address: not
  deprecated, most stable `type` (`static` > `dynamic` > `temporary`), longest
  preferred lifetime.
- Lifetimes are in seconds; `null` means infinite.
- Returns `503` when no matching address exists, `401` when a token is
  configured and missing or wrong.

`GET /healthz` returns `ok` (no token required).

## Configuration

`/etc/ipv6who/config.toml` (see [dist/config.toml](dist/config.toml)):

```toml
listen = ["[::]:8066"]
allow = ["::1/128", "127.0.0.0/8", "2001:db8:1234::/48"]
# token = "change-me"
include_temporary = false

[interfaces]
include = []
exclude = ["lo", "docker*", "veth*"]
```

Connections from addresses outside `allow` are closed right after `accept()`,
before any HTTP is parsed; IPv4 clients on a dual-stack `[::]` socket are
matched against IPv4 rules. If the file is missing, ipv6who starts with
loopback-only defaults. Check a file with `ipv6who --config <path> --check-config`,
and set `RUST_LOG=debug` to log rejected connections.

## License

MIT — see [LICENSE](LICENSE).
