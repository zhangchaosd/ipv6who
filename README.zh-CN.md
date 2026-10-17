# ipv6who

[English](README.md) | 简体中文

一个小型 HTTP 服务，返回所在主机的全局 IPv6 地址，并支持客户端 IP 白名单。

支持 Linux、macOS 和 Windows。只返回全局单播地址（`2000::/3`）；唯一本地地址（`fc00::/7`）、
链路本地地址（`fe80::/10`）以及处于 DAD 检测中或地址冲突的地址都会被跳过；隐私扩展产生的临时地址默认不返回。
地址直接从操作系统读取：Linux 使用 rtnetlink，macOS 使用 `getifaddrs` 和 in6 ioctl，Windows 使用 `GetAdaptersAddresses`。

## 安装

没有配置文件：所有设置都通过服务定义里的命令行参数传入（见[参数](#参数)）。默认监听 8066 端口，
只接受本机访问；请把允许查询的网段加入 `--allow`，如果客户端在其他机器上，还需要在防火墙中放行 TCP 8066 端口。

所有平台都直接从[最新 release](https://github.com/zhangchaosd/ipv6who/releases/latest) 下载预编译二进制，
不需要 Rust 工具链。Linux 和 macOS 上以当前用户身份作为服务运行，不需要 root；Windows 上注册为系统服务，
只在注册时需要一次管理员权限，服务本身以非特权的 `LocalService` 账户运行。

### Linux（systemd 用户服务）

所有文件都放在 `~/ipv6who` 目录下，以当前用户身份运行：

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

修改参数：编辑 `~/ipv6who/ipv6who.service` 中的 `ExecStart=` 行，然后执行
`systemctl --user daemon-reload && systemctl --user restart ipv6who`。
日志：`~/ipv6who/ipv6who.log`。如需开机后无需登录即自动运行，执行 `loginctl enable-linger`
（部分系统需要 `sudo loginctl enable-linger $USER`）。

卸载：

```sh
systemctl --user disable --now ipv6who
rm -rf ~/ipv6who
```

如需加固的系统级服务，可参考 [dist/ipv6who.service](dist/ipv6who.service)（`DynamicUser=yes`）。

### macOS（launchd 用户代理）

二进制安装到 `~/.local/bin`，以当前用户身份运行，不需要 sudo：

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

修改参数：编辑 `~/Library/LaunchAgents/io.github.zhangchaosd.ipv6who.plist` 中的 `ProgramArguments`，
然后执行 `launchctl bootout gui/$(id -u)/io.github.zhangchaosd.ipv6who`，再执行上面的 `bootstrap` 那一行重新加载。
日志：`~/Library/Logs/ipv6who.log`。用户代理只在你登录后运行；如果这台 Mac 没有开启自动登录，
请使用系统级守护进程 [dist/io.github.zhangchaosd.ipv6who.plist](dist/io.github.zhangchaosd.ipv6who.plist)
（以 `nobody` 运行，需要 sudo）。

卸载：

```sh
launchctl bootout gui/$(id -u)/io.github.zhangchaosd.ipv6who
rm ~/.local/bin/ipv6who ~/Library/LaunchAgents/io.github.zhangchaosd.ipv6who.plist ~/Library/Logs/ipv6who.log
```

### Windows（服务）

二进制安装到 `C:\Program Files\ipv6who`，注册为 Windows 服务，以内置的 `LocalService` 账户运行。
**以管理员身份**打开 PowerShell 执行（需要 Windows 10 1803 或更新版本，以使用系统自带的 `curl.exe` 和 `tar.exe`）：

```powershell
$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }
New-Item -ItemType Directory -Force "C:\Program Files\ipv6who", "C:\ProgramData\ipv6who\logs" | Out-Null
curl.exe -fsSLo "$env:TEMP\ipv6who.zip" "https://github.com/zhangchaosd/ipv6who/releases/latest/download/ipv6who-windows-$arch.zip"
tar.exe -xf "$env:TEMP\ipv6who.zip" -C "C:\Program Files\ipv6who" --strip-components=1 "ipv6who-windows-$arch/ipv6who.exe"
icacls C:\ProgramData\ipv6who\logs /grant "*S-1-5-19:(OI)(CI)M"
New-Service -Name ipv6who -StartupType Automatic -BinaryPathName '"C:\Program Files\ipv6who\ipv6who.exe" --service --listen [::]:8066 --allow ::1/128,127.0.0.0/8 --log-file C:\ProgramData\ipv6who\logs\ipv6who.log'
sc.exe config ipv6who obj= "NT AUTHORITY\LocalService"
Start-Service ipv6who
curl.exe -s "http://[::1]:8066/v1/ipv6"
```

修改参数：改写服务的命令行后重启，例如：

```powershell
Set-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Services\ipv6who ImagePath '"C:\Program Files\ipv6who\ipv6who.exe" --service --listen [::]:8066 --allow ::1/128,127.0.0.0/8,192.168.1.0/24 --log-file C:\ProgramData\ipv6who\logs\ipv6who.log'
Restart-Service ipv6who
```

日志：`C:\ProgramData\ipv6who\logs\ipv6who.log`。服务以内置的 `LocalService` 账户（`S-1-5-19`）运行，
只对日志目录有写权限。
防火墙：`New-NetFirewallRule -DisplayName ipv6who -Direction Inbound -Protocol TCP -LocalPort 8066 -Action Allow`。

卸载：

```powershell
Stop-Service ipv6who
sc.exe delete ipv6who
Remove-Item -Recurse -Force "C:\Program Files\ipv6who", C:\ProgramData\ipv6who
```

### 从源码构建

使用 Rust 1.88 或更新版本（[rustup](https://rustup.rs)）执行 `cargo build --release --locked`，
然后在上面的步骤中用 `target/release/ipv6who` 代替下载的二进制。

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

- `address` / `interface` / `type` 描述首选地址：优先未弃用的地址，其次是更稳定的类型
  （`static` > `dynamic` > `temporary`），再其次是剩余首选生存期更长的地址。
- `system` 描述主机：主机名 `hostname`、`os`（`linux`、`macos`、`windows`）、可读名称 `name`
  （如 `Fedora Linux 44 (Cloud Edition)`、`macOS 27.0.1`、`Windows 11 Pro 24H2`）、`version` 和 CPU 架构 `arch`。
- 生存期单位为秒，`null` 表示永久。
- 没有符合条件的地址时返回 `503`；配置了 token 但请求缺失或错误时返回 `401`。

`GET /healthz` 返回 `ok`（不需要 token）。

## 参数

| 参数 | 默认值 | 说明 |
|---|---|---|
| `-l, --listen <ADDR>` | `[::]:8066` | 监听地址，可重复指定多个。`[::]` 同时接受 IPv4。 |
| `-a, --allow <CIDR>` | `::1/128,127.0.0.0/8` | 允许连接的客户端网段，可重复指定或用逗号分隔。 |
| `--token <TOKEN>` | 无 | 访问 `/v1/*` 需要的 Bearer token；也可通过 `IPV6WHO_TOKEN` 环境变量传入。 |
| `--include-temporary` | 关闭 | 同时返回隐私扩展产生的临时地址。 |
| `--interface <NAME>` | 全部 | 只考虑这些网卡（如 `eth0`、`en0`、`Ethernet`）；末尾的 `*` 匹配任意后缀。 |
| `--exclude-interface <NAME>` | 无 | 排除这些网卡，优先于 `--interface`。 |
| `--log-file <PATH>` | stderr | 把日志追加写入文件。 |
| `--dump-addresses` | | 打印操作系统报告的全部 IPv6 地址（不过滤）后退出。 |

来自 `--allow` 之外地址的连接会在 `accept()` 之后立即关闭，不会解析任何 HTTP 内容；
IPv4 客户端按 IPv4 规则匹配。设置 `RUST_LOG=debug` 可记录被拒绝的连接。

建议通过 `IPV6WHO_TOKEN` 环境变量而不是 `--token` 传入 token，因为命令行对本机所有用户可见：
在 systemd unit 中加入 `Environment=IPV6WHO_TOKEN=...`（并 `chmod 600`），在 launchd plist 中启用注释掉的
`EnvironmentVariables` 段，Windows 上执行
`Set-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Services\ipv6who Environment -Type MultiString -Value 'IPV6WHO_TOKEN=...'`。

## 许可证

MIT，见 [LICENSE](LICENSE)。
