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

There is no config file: every setting is a command-line flag in the service
definition (see [Options](#options)). The defaults listen on port 8066 and only
accept clients on this machine, so add the networks that may query the service
to `--allow`, and open TCP port 8066 in the firewall if clients are remote.

On Linux and macOS, download the prebuilt binary from the latest release and
run it as a service under your own user: no root and no Rust toolchain needed.
On Windows, build from source with Rust 1.88 or newer
([rustup](https://rustup.rs)) and install it as a system service;
administrator rights are only needed to copy files and register it, and the
service itself runs as the unprivileged `LocalService` account.

### Linux (systemd user service)

Everything lives in `~/ipv6who` and runs as your own user:

```sh
mkdir -p ~/ipv6who && cd ~/ipv6who
curl -fsSL https://github.com/zhangchaosd/ipv6who/releases/latest/download/ipv6who-linux-$(uname -m).tar.gz | tar -xz --strip-components=1 ipv6who-linux-$(uname -m)/ipv6who
cat > ipv6who.service <<'EOF'
[Unit]
Description=Report this host's global IPv6 address over HTTP
After=network-online.target

[Service]
Type=simple
WorkingDirectory=%h/ipv6who
ExecStart=%h/ipv6who/ipv6who --listen [::]:8066 --allow ::1/128,127.0.0.0/8
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

To change options, edit the `ExecStart=` line in `~/ipv6who/ipv6who.service`,
then `systemctl --user daemon-reload && systemctl --user restart ipv6who`.
Logs: `~/ipv6who/ipv6who.log`. To start at boot without logging in, run
`loginctl enable-linger` (some systems require `sudo loginctl enable-linger $USER`).

Uninstall:

```sh
systemctl --user disable --now ipv6who
rm -rf ~/ipv6who
```

For a hardened system-wide service instead, see
[dist/ipv6who.service](dist/ipv6who.service) (`DynamicUser=yes`).

### macOS (launchd user agent)

The binary goes to `~/.local/bin` and runs as your own user, no sudo needed:

```sh
mkdir -p ~/.local/bin ~/Library/LaunchAgents
curl -fsSL https://github.com/zhangchaosd/ipv6who/releases/latest/download/ipv6who-macos-universal.tar.gz | tar -xz -C ~/.local/bin --strip-components=1 ipv6who-macos-universal/ipv6who
cat > ~/Library/LaunchAgents/io.github.zhangchaosd.ipv6who.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>io.github.zhangchaosd.ipv6who</string>
	<key>ProgramArguments</key>
	<array>
		<string>$HOME/.local/bin/ipv6who</string>
		<string>--listen</string>
		<string>[::]:8066</string>
		<string>--allow</string>
		<string>::1/128,127.0.0.0/8</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
	<key>StandardOutPath</key>
	<string>$HOME/Library/Logs/ipv6who.log</string>
	<key>StandardErrorPath</key>
	<string>$HOME/Library/Logs/ipv6who.log</string>
</dict>
</plist>
EOF
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/io.github.zhangchaosd.ipv6who.plist
curl -s 'http://[::1]:8066/v1/ipv6'
```

To change options, edit `ProgramArguments` in
`~/Library/LaunchAgents/io.github.zhangchaosd.ipv6who.plist`, then reload it with
`launchctl bootout gui/$(id -u)/io.github.zhangchaosd.ipv6who` followed by the
`bootstrap` line above. Logs: `~/Library/Logs/ipv6who.log`. A user agent runs
while you are logged in; for a Mac without automatic login, use the system-wide
daemon in [dist/io.github.zhangchaosd.ipv6who.plist](dist/io.github.zhangchaosd.ipv6who.plist)
(runs as `nobody`, needs sudo).

Uninstall:

```sh
launchctl bootout gui/$(id -u)/io.github.zhangchaosd.ipv6who
rm ~/.local/bin/ipv6who ~/Library/LaunchAgents/io.github.zhangchaosd.ipv6who.plist ~/Library/Logs/ipv6who.log
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
icacls C:\ProgramData\ipv6who\logs /grant "*S-1-5-19:(OI)(CI)M"
New-Service -Name ipv6who -StartupType Automatic -BinaryPathName '"C:\Program Files\ipv6who\ipv6who.exe" --service --listen [::]:8066 --allow ::1/128,127.0.0.0/8 --log-file C:\ProgramData\ipv6who\logs\ipv6who.log'
sc.exe config ipv6who obj= "NT AUTHORITY\LocalService"
Start-Service ipv6who
curl.exe -s "http://[::1]:8066/v1/ipv6"
```

To change options, rewrite the service command line and restart, e.g.:

```powershell
Set-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Services\ipv6who ImagePath '"C:\Program Files\ipv6who\ipv6who.exe" --service --listen [::]:8066 --allow ::1/128,127.0.0.0/8,192.168.1.0/24 --log-file C:\ProgramData\ipv6who\logs\ipv6who.log'
Restart-Service ipv6who
```

Logs: `C:\ProgramData\ipv6who\logs\ipv6who.log`. Runs as the built-in
`LocalService` account (`S-1-5-19`), which may only write to the logs folder.
Firewall: `New-NetFirewallRule -DisplayName ipv6who -Direction Inbound -Protocol TCP -LocalPort 8066 -Action Allow`.

Uninstall:

```powershell
Stop-Service ipv6who
sc.exe delete ipv6who
Remove-Item -Recurse -Force "C:\Program Files\ipv6who", C:\ProgramData\ipv6who
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

## Options

| Flag | Default | Meaning |
|---|---|---|
| `-l, --listen <ADDR>` | `[::]:8066` | Address to listen on; repeat for several. `[::]` also accepts IPv4. |
| `-a, --allow <CIDR>` | `::1/128,127.0.0.0/8` | Client networks allowed to connect; repeat or separate with commas. |
| `--token <TOKEN>` | none | Bearer token required for `/v1/*`; also read from `IPV6WHO_TOKEN`. |
| `--include-temporary` | off | Also report privacy-extension temporary addresses. |
| `--interface <NAME>` | all | Only consider these interfaces (`eth0`, `en0`, `Ethernet`); a trailing `*` matches any suffix. |
| `--exclude-interface <NAME>` | none | Never consider these interfaces; wins over `--interface`. |
| `--log-file <PATH>` | stderr | Append logs to a file. |
| `--dump-addresses` | | Print every IPv6 address the OS reports, unfiltered, and exit. |

Connections from addresses outside `--allow` are closed right after
`accept()`, before any HTTP is parsed; IPv4 clients are matched against IPv4
rules. Set `RUST_LOG=debug` to log rejected connections.

Pass the token through the `IPV6WHO_TOKEN` environment variable rather than
`--token`, because command lines are visible to every local user: add
`Environment=IPV6WHO_TOKEN=...` to the systemd unit (and `chmod 600` it), the
commented `EnvironmentVariables` block in the launchd plist, or on Windows
`Set-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Services\ipv6who Environment -Type MultiString -Value 'IPV6WHO_TOKEN=...'`.

## License

MIT — see [LICENSE](LICENSE).
