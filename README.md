# ipv6who

English | [简体中文](README.zh-CN.md)

A small HTTP service that reports the global IPv6 address of the host, with an IP allowlist.

Runs on Linux, macOS and Windows. Only global unicast addresses (`2000::/3`)
are reported; unique-local (`fc00::/7`), link-local (`fe80::/10`), tentative
and duplicate addresses are skipped, and privacy-extension temporary
addresses are skipped unless enabled. Addresses come from the OS directly:
rtnetlink on Linux, `getifaddrs` and the in6 ioctls on macOS,
`GetAdaptersAddresses` on Windows.

## Install

Build from source with Rust 1.88 or newer ([rustup](https://rustup.rs)), then
install it as a service. On every OS the service runs as an unprivileged
account; administrator rights are only needed to copy files and register it.
After editing the config, restart the service.

The default config only accepts clients on this machine: add the networks
that may query the service to `allow` (see [Configuration](#configuration)),
and open TCP port 8066 in the firewall if clients are remote.

### Linux (systemd)

```sh
git clone https://github.com/zhangchaosd/ipv6who.git
cd ipv6who
cargo build --release --locked
sudo install -Dm755 target/release/ipv6who /usr/local/bin/ipv6who
sudo install -Dm644 dist/config.toml /etc/ipv6who/config.toml
sudo install -Dm644 dist/ipv6who.service /etc/systemd/system/ipv6who.service
sudo systemctl daemon-reload
sudo systemctl enable --now ipv6who
curl -s 'http://[::1]:8066/v1/ipv6'
```

Config: `/etc/ipv6who/config.toml`. Restart: `sudo systemctl restart ipv6who`.
Logs: `journalctl -u ipv6who`. Runs with `DynamicUser=yes`: a transient user
with no capabilities in a sandbox that only allows TCP and netlink sockets.

Uninstall:

```sh
sudo systemctl disable --now ipv6who
sudo rm -r /usr/local/bin/ipv6who /etc/systemd/system/ipv6who.service /etc/ipv6who
sudo systemctl daemon-reload
```

### macOS (launchd)

Requires the Xcode Command Line Tools (`xcode-select --install`).

```sh
git clone https://github.com/zhangchaosd/ipv6who.git
cd ipv6who
cargo build --release --locked
sudo mkdir -p /usr/local/bin /usr/local/etc/ipv6who
sudo install -m 755 target/release/ipv6who /usr/local/bin/ipv6who
sudo install -m 644 dist/config.toml /usr/local/etc/ipv6who/config.toml
sudo install -m 644 dist/io.github.zhangchaosd.ipv6who.plist /Library/LaunchDaemons/
sudo touch /var/log/ipv6who.log && sudo chown nobody /var/log/ipv6who.log
sudo launchctl bootstrap system /Library/LaunchDaemons/io.github.zhangchaosd.ipv6who.plist
curl -s 'http://[::1]:8066/v1/ipv6'
```

Config: `/usr/local/etc/ipv6who/config.toml`.
Restart: `sudo launchctl kickstart -k system/io.github.zhangchaosd.ipv6who`.
Logs: `/var/log/ipv6who.log` (launchd opens it as the service user, hence the `chown`). Runs as the `nobody` user.

Uninstall:

```sh
sudo launchctl bootout system/io.github.zhangchaosd.ipv6who
sudo rm -r /usr/local/bin/ipv6who /usr/local/etc/ipv6who /Library/LaunchDaemons/io.github.zhangchaosd.ipv6who.plist /var/log/ipv6who.log
```

### Windows (service)

Install Rust with `rustup-init.exe` (let it install the Visual Studio Build
Tools) and Git, then run in PowerShell **as Administrator**:

```powershell
git clone https://github.com/zhangchaosd/ipv6who.git
cd ipv6who
cargo build --release --locked
New-Item -ItemType Directory -Force "C:\Program Files\ipv6who", "C:\ProgramData\ipv6who\logs" | Out-Null
Copy-Item target\release\ipv6who.exe "C:\Program Files\ipv6who\"
Copy-Item dist\config.toml C:\ProgramData\ipv6who\
icacls C:\ProgramData\ipv6who\logs /grant "*S-1-5-19:(OI)(CI)M"
New-Service -Name ipv6who -StartupType Automatic -BinaryPathName '"C:\Program Files\ipv6who\ipv6who.exe" --service --config C:\ProgramData\ipv6who\config.toml --log-file C:\ProgramData\ipv6who\logs\ipv6who.log'
sc.exe config ipv6who obj= "NT AUTHORITY\LocalService"
Start-Service ipv6who
curl.exe -s "http://[::1]:8066/v1/ipv6"
```

Config: `C:\ProgramData\ipv6who\config.toml`. Restart: `Restart-Service ipv6who`.
Logs: `C:\ProgramData\ipv6who\logs\ipv6who.log`. Runs as the built-in
`LocalService` account (`S-1-5-19`), which may only write to the logs folder.
Firewall: `New-NetFirewallRule -DisplayName ipv6who -Direction Inbound -Protocol TCP -LocalPort 8066 -Action Allow`.

Uninstall:

```powershell
Stop-Service ipv6who
sc.exe delete ipv6who
Remove-Item -Recurse -Force "C:\Program Files\ipv6who", C:\ProgramData\ipv6who
```

The config file must be readable by the service account, so it is readable
by all local users on every OS; keep that in mind if you set a `token`.

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

See [dist/config.toml](dist/config.toml) for every option:

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
before any HTTP is parsed; `[::]` also accepts IPv4 clients, which are matched
against IPv4 rules. Interface names are as the OS shows them (`eth0`, `en0`,
`Ethernet`). If the file is missing, ipv6who starts with loopback-only
defaults. Check a file with `ipv6who --config <path> --check-config`, list
what the OS reports with `ipv6who --dump-addresses`, and set `RUST_LOG=debug`
to log rejected connections.

## Packaging

`ipv6who.spec` builds a Fedora RPM with vendored crates: Source0 is the GitHub
tag tarball for the spec's `Version`, Source1 is made by `dist/vendor.sh`.
[`.copr/Makefile`](.copr/Makefile) produces the SRPM from a git checkout, so
COPR can build straight from the repository. In the COPR project, add a
package with source type **SCM**:

| Field | Value |
| --- | --- |
| Clone URL | `https://github.com/zhangchaosd/ipv6who.git` |
| Committish | `main` (or a tag such as `v0.1.0`) |
| Subdirectory | *(empty)* |
| Spec File | `ipv6who.spec` |
| SRPM build method | `make_srpm` |

Enable *Auto-rebuild* on the package, then in COPR open *Settings →
Integrations*, copy the GitHub webhook URL, and add it in the GitHub repository
under *Settings → Webhooks → Add webhook* (content type
`application/json`, events: *Just the push event*). Each push then rebuilds
the package.

The tarball is downloaded from the tag, so to release: bump `Version` in
`Cargo.toml` and `ipv6who.spec`, add a `%changelog` entry, push the tag
`v<version>` first and then the commit to `main`. A push whose spec names an
untagged version fails at the download step. To test locally on Fedora
(installs build tools with dnf, so run as root):

```sh
sudo make -f .copr/Makefile srpm outdir=/tmp/out spec=ipv6who.spec
rpmbuild --rebuild /tmp/out/ipv6who-*.src.rpm
```

## License

MIT — see [LICENSE](LICENSE).
