# ipv6who

[English](README.md) | 简体中文

一个小型 HTTP 服务，返回所在主机的全局 IPv6 地址，并支持客户端 IP 白名单。

地址通过 rtnetlink 从内核读取（仅支持 Linux）。只返回全局单播地址（`2000::/3`）；
唯一本地地址（`fc00::/7`）、链路本地地址（`fe80::/10`）以及处于 DAD 检测中或检测失败的地址都会被跳过；
隐私扩展产生的临时地址默认不返回。

## 安装

需要带 systemd 的 Linux 和 Rust 1.88 或更新版本（Fedora：`sudo dnf install cargo git`）。

```sh
git clone https://github.com/zhangchaosd/ipv6who.git
cd ipv6who
cargo build --release --locked
sudo install -Dm755 target/release/ipv6who /usr/local/bin/ipv6who
sudo install -Dm644 dist/config.toml /etc/ipv6who/config.toml
sudo install -Dm644 dist/ipv6who.service /etc/systemd/system/ipv6who.service
```

编辑 `/etc/ipv6who/config.toml`，把允许访问的客户端网段加入 `allow`，然后启动：

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now ipv6who
curl -s 'http://[::1]:8066/v1/ipv6'
```

如果启用了 firewalld，需要放行端口：
`sudo firewall-cmd --permanent --add-port=8066/tcp && sudo firewall-cmd --reload`。

### 以非特权用户运行

`sudo` 只用于复制文件。服务通过 `DynamicUser=yes` 运行：由 systemd 临时分配一个没有任何
capability 的系统用户，文件系统只读，沙箱只允许 TCP 和 netlink 套接字。通过 netlink
读取地址不需要特权，因此监听端口必须不小于 1024。由于临时用户需要读取配置文件，该文件对所有
用户可读；如果设置了 `token`，请注意这一点。

### 卸载

```sh
sudo systemctl disable --now ipv6who
sudo rm -r /usr/local/bin/ipv6who /etc/systemd/system/ipv6who.service /etc/ipv6who
sudo systemctl daemon-reload
```

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

`/etc/ipv6who/config.toml`（完整示例见 [dist/config.toml](dist/config.toml)）：

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
在双栈 `[::]` 套接字上，IPv4 客户端按 IPv4 规则匹配。配置文件不存在时，ipv6who 使用只允许本机访问的默认配置。
可以用 `ipv6who --config <路径> --check-config` 检查配置文件，设置 `RUST_LOG=debug` 可记录被拒绝的连接。

## 许可证

MIT，见 [LICENSE](LICENSE)。
