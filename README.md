# ipv6who

A small HTTP service that reports the global IPv6 address of the host, with an IP allowlist.

Addresses are read from the kernel over rtnetlink (Linux only). Only global
unicast addresses (`2000::/3`) are reported; unique-local (`fc00::/7`),
link-local (`fe80::/10`), tentative and DAD-failed addresses are skipped, and
privacy-extension temporary addresses are skipped unless enabled.

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
before any HTTP is parsed. If the file is missing, ipv6who starts with the
loopback-only defaults shown above. Validate a file with
`ipv6who --config path --check-config`.

## Running

```sh
cargo build --release
./target/release/ipv6who --config dist/config.toml
curl -s 'http://[::1]:8066/v1/ipv6'
```

A hardened systemd unit using `DynamicUser=yes` is in
[dist/ipv6who.service](dist/ipv6who.service). Set `RUST_LOG=debug` to log
rejected connections.

## Packaging

[ipv6who.spec](ipv6who.spec) builds an RPM with vendored dependencies (for
COPR). Create the `Source1` tarball with `dist/vendor.sh`.

## License

MIT — see [LICENSE](LICENSE).
