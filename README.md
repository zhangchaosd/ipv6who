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

On Linux, download the prebuilt static binary (x86_64 or aarch64) and run it
as a systemd user service: no root and no Rust toolchain needed. On macOS and
Windows, build from source with Rust 1.88 or newer
([rustup](https://rustup.rs)) and install it as a system service; administrator
rights are only needed to copy files and register it, and the service itself
runs as an unprivileged account. After editing the config, restart the service.

The default config only accepts clients on this machine: add the networks
that may query the service to `allow` (see [Configuration](#configuration)),
and open TCP port 8066 in the firewall if clients are remote.

### Linux (systemd user service)

Everything lives in `~/ipv6who` and runs as your own user:

```sh
mkdir -p ~/ipv6who && cd ~/ipv6who
curl -fsSL https://github.com/zhangchaosd/ipv6who/releases/download/v0.2.0/ipv6who-0.2.0-linux-$(uname -m).tar.gz | tar -xz --strip-components=1 ipv6who-0.2.0-linux-$(uname -m)/ipv6who
cat > config.toml <<'EOF'
listen = ["[::]:8066"]
# Client networks allowed to query the service; add your LAN or prefix here.
allow = ["::1/128", "127.0.0.0/8"]
EOF
cat > ipv6who.service <<'EOF'
[Unit]
Description=Report this host's global IPv6 address over HTTP
After=network-online.target

[Service]
Type=simple
WorkingDirectory=%h/ipv6who
ExecStart=%h/ipv6who/ipv6who --config %h/ipv6who/config.toml
Environment=NO_COLOR=1
Restart=on-failure
RestartSec=5
UMask=0077
StandardOutput=append:%h/ipv6who/ipv6who.log
StandardError=append:%h/ipv6who/ipv6who.log

[Install]
WantedBy=default.target
EOF
systemctl --user enable --now ~/ipv6who/ipv6who.service
curl -s 'http://[::1]:8066/v1/ipv6'
```

Edit `~/ipv6who/config.toml`, then `systemctl --user restart ipv6who`.
Logs: `~/ipv6who/ipv6who.log`. To start at boot without logging in, run
`loginctl enable-linger` (some systems require `sudo loginctl enable-linger $USER`).

Uninstall:

```sh
systemctl --user disable --now ipv6who
rm -rf ~/ipv6who
```

For a hardened system-wide service instead, see
[dist/ipv6who.service](dist/ipv6who.service) (`DynamicUser=yes`).

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

On macOS and Windows the config file must be readable by the service
account, so it is readable by all local users; keep that in mind if you set a
`token`.

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
  ],
  "system": {
    "hostname": "server1",
    "os": "linux",
    "name": "Fedora Linux 44 (Cloud Edition)",
    "version": "44",
    "arch": "x86_64"
  }
}
```

- `address` / `interface` / `type` describe the preferred address: not
  deprecated, most stable `type` (`static` > `dynamic` > `temporary`), longest
  preferred lifetime.
- `system` describes the host: `hostname`, `os` (`linux`, `macos`, `windows`), a
  readable `name` such as `Fedora Linux 44 (Cloud Edition)`, `macOS 27.0.1`
  or `Windows 11 Pro 24H2`, `version` and CPU `arch`.
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

## License

MIT — see [LICENSE](LICENSE).
