# ipv6who

[English](README.md) | 简体中文

一个小型 HTTP 服务，返回所在主机的全局 IPv6 地址，并支持客户端 IP 白名单。

支持 Linux、macOS 和 Windows。只返回全局单播地址（`2000::/3`）；唯一本地地址（`fc00::/7`）、
链路本地地址（`fe80::/10`）以及处于 DAD 检测中或地址冲突的地址都会被跳过；隐私扩展产生的临时地址默认不返回。
地址直接从操作系统读取：Linux 使用 rtnetlink，macOS 使用 `getifaddrs` 和 in6 ioctl，Windows 使用 `GetAdaptersAddresses`。

## 安装

使用 Rust 1.88 或更新版本（[rustup](https://rustup.rs)）从源码构建，然后安装为系统服务。
在所有系统上服务都以非特权账户运行，管理员权限只用于复制文件和注册服务。修改配置后需要重启服务。

默认配置只接受本机访问：把允许查询的网段加入 `allow`（见[配置](#配置)），
如果客户端在其他机器上，还需要在防火墙中放行 TCP 8066 端口。

### Linux（systemd）

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

配置：`/etc/ipv6who/config.toml`。重启：`sudo systemctl restart ipv6who`。
日志：`journalctl -u ipv6who`。服务通过 `DynamicUser=yes` 运行：由 systemd 临时分配一个没有任何
capability 的用户，沙箱只允许 TCP 和 netlink 套接字。

卸载：

```sh
sudo systemctl disable --now ipv6who
sudo rm -r /usr/local/bin/ipv6who /etc/systemd/system/ipv6who.service /etc/ipv6who
sudo systemctl daemon-reload
```

### macOS（launchd）

需要先安装 Xcode 命令行工具（`xcode-select --install`）。

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

配置：`/usr/local/etc/ipv6who/config.toml`。
重启：`sudo launchctl kickstart -k system/io.github.zhangchaosd.ipv6who`。
日志：`/var/log/ipv6who.log`（launchd 以服务用户身份打开该文件，所以需要 `chown`）。服务以 `nobody` 用户运行。

卸载：

```sh
sudo launchctl bootout system/io.github.zhangchaosd.ipv6who
sudo rm -r /usr/local/bin/ipv6who /usr/local/etc/ipv6who /Library/LaunchDaemons/io.github.zhangchaosd.ipv6who.plist /var/log/ipv6who.log
```

### Windows（服务）

用 `rustup-init.exe` 安装 Rust（按提示安装 Visual Studio Build Tools），并安装 Git，
然后**以管理员身份**打开 PowerShell 执行：

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

配置：`C:\ProgramData\ipv6who\config.toml`。重启：`Restart-Service ipv6who`。
日志：`C:\ProgramData\ipv6who\logs\ipv6who.log`。服务以内置的 `LocalService` 账户（`S-1-5-19`）运行，
只对日志目录有写权限。
防火墙：`New-NetFirewallRule -DisplayName ipv6who -Direction Inbound -Protocol TCP -LocalPort 8066 -Action Allow`。

卸载：

```powershell
Stop-Service ipv6who
sc.exe delete ipv6who
Remove-Item -Recurse -Force "C:\Program Files\ipv6who", C:\ProgramData\ipv6who
```

由于服务账户需要读取配置文件，在所有系统上该文件对本机所有用户可读；如果设置了 `token`，请注意这一点。

## 接口

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

- `address` / `interface` / `type` 描述首选地址：优先未弃用的地址，其次是更稳定的类型
  （`static` > `dynamic` > `temporary`），再其次是剩余首选生存期更长的地址。
- 生存期单位为秒，`null` 表示永久。
- 没有符合条件的地址时返回 `503`；配置了 token 但请求缺失或错误时返回 `401`。

`GET /healthz` 返回 `ok`（不需要 token）。

## 配置

全部选项见 [dist/config.toml](dist/config.toml)：

```toml
listen = ["[::]:8066"]
allow = ["::1/128", "127.0.0.0/8", "2001:db8:1234::/48"]
# token = "change-me"
include_temporary = false

[interfaces]
include = []
exclude = ["lo", "docker*", "veth*"]
```

来自 `allow` 之外地址的连接会在 `accept()` 之后立即关闭，不会解析任何 HTTP 内容；
`[::]` 同时接受 IPv4 客户端，按 IPv4 规则匹配。网卡名称以操作系统显示的为准（如 `eth0`、`en0`、`Ethernet`）。
配置文件不存在时，ipv6who 使用只允许本机访问的默认配置。可以用 `ipv6who --config <路径> --check-config`
检查配置文件，用 `ipv6who --dump-addresses` 查看操作系统报告的全部地址，设置 `RUST_LOG=debug` 可记录被拒绝的连接。

## 许可证

MIT，见 [LICENSE](LICENSE)。
