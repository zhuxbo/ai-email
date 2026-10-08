# 开发与部署

## 新产品开发入口

| 目录                   | 职责                                   | 工具                                   |
| ---------------------- | -------------------------------------- | -------------------------------------- |
| `server/`              | 邮箱同步、SQLite、HTTP / MCP、邮件操作 | Rust stable                            |
| `android/`             | 原生搜索与简单回复 App                 | JDK 17、Android SDK 36、Gradle wrapper |
| `macos/`               | SwiftUI 搜索与简单回复 App             | macOS 13+、Swift Command Line Tools    |
| `scripts/mcp_stdio.py` | stdio 到远程 MCP 桥接                  | Python 3 标准库                        |
| `deploy/`              | systemd 和 HTTPS 反向代理配置示例      | Linux、Caddy                           |

产品范围和配置字段见 [README.md](README.md)。服务器独立管理邮箱凭据和缓存；客户端只保存服务连接信息，不迁移旧缓存。

```bash
cargo test --manifest-path server/Cargo.toml
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path server/Cargo.toml --check
cargo build --manifest-path server/Cargo.toml
python3 -m unittest discover -s scripts -p 'test_*.py'
(cd android && ./gradlew testDebugUnitTest lintDebug assembleDebug)
swift run --package-path macos MailSearchChecks
bash scripts/build-macos-search.sh
```

设置 `ANDROID_HOME` 或在忽略的 `android/local.properties` 中配置 `sdk.dir`。原生 App 不需要 Rust Android target 或 NDK。可使用 Android Studio 打开 `android/`。

本机调试服务可绑定 `127.0.0.1:8080`。安卓模拟器通过 `10.0.2.2` 访问宿主机；只有 Debug 构建且显式开启开发选项才允许这个地址使用 HTTP。生产和真机使用 HTTPS。stdio 桥接仅在设置 `AI_EMAIL_ALLOW_HTTP=1` 后允许 localhost / loopback HTTP。

## Linux 单机部署

以下命令是部署操作说明，不会由测试自动执行。先在目标 Linux 架构构建 `server/target/release/ai-email-server`，配置域名 DNS，安装 Caddy。不要把 macOS 构建的二进制复制到 Linux。

创建专用用户和目录：

```bash
sudo useradd --system --home /var/lib/ai-email --shell /usr/sbin/nologin ai-email
sudo install -d -m 0755 /opt/ai-email
sudo install -d -m 0750 -o root -g ai-email /etc/ai-email
sudo install -d -m 0700 -o ai-email -g ai-email /var/lib/ai-email
sudo install -m 0755 server/target/release/ai-email-server /opt/ai-email/ai-email-server
sudo install -m 0640 -o root -g ai-email server/config.example.json /etc/ai-email/config.json
```

生成两份独立令牌，避免在日志中打印：

```bash
sudo sh -c 'umask 077; openssl rand -hex 32 > /etc/ai-email/read-token; openssl rand -hex 32 > /etc/ai-email/write-token'
sudo chown root:ai-email /etc/ai-email/read-token /etc/ai-email/write-token
sudo chmod 0640 /etc/ai-email/read-token /etc/ai-email/write-token
```

在 `/etc/ai-email/work-password` 保存邮箱授权码（单行），设为 `root:ai-email`、`0640`。编辑配置中的账户邮箱、服务商主机和同步文件夹。新增账户必须使用不同且稳定的 `id`，最多 10 个；修改账户身份时应使用新 id，不能把原账户 id 直接复用给另一邮箱。

示例 `allowed_hosts` 中的 `mail.example.com` 应替换成实际域名；服务只监听 loopback。`allowed_origins: []` 适合不带 Origin 的原生客户端；若 AI 客户端发送 Origin，填入其完整且明确的来源，不使用通配符。

```bash
sudo install -m 0644 deploy/ai-email.service /etc/systemd/system/ai-email.service
sudo systemctl daemon-reload
sudo systemctl enable --now ai-email
sudo systemctl status ai-email
sudo journalctl -u ai-email -n 50
```

将 `deploy/Caddyfile` 的站点配置合并进服务器 Caddy 配置，替换实际域名，验证配置后重载。不要覆盖服务器上的其他站点。只需对外开放 HTTPS；不要暴露后端 8080 端口。

Android 和 macOS 各自只保存一个访问令牌：只读令牌仅能搜索，可写令牌可简单回复。服务器仍配置两份不同的令牌；AI 需要写操作时使用可写令牌。改变令牌、账户或同步配置后重启 `ai-email`；已移除的账户/文件夹缓存会在启动时清理。

## 数据与故障处理

- SQLite 存在 `database_path`；目录必须存在且服务用户可写。数据库包含邮件正文，按个人敏感数据保护。
- 保留期按 IMAP INTERNALDATE 计算为滚动 365 天。首次接入默认跳过现存邮件；服务器停机期间的新邮件会在恢复后按游标补取，只取保留期内。
- `folders` 是邮箱侧 IMAP 文件夹名称，不是 AI 内容分类名称。先在邮箱侧配置规则把需要的邮件放入对应文件夹，再指定它们。Gmail 多标签邮件可能在不同文件夹分别出现。
- 源邮箱 UIDVALIDITY 变化时，旧引用会失效并重新建立同步起点；默认不回导历史。写操作会核验 UIDVALIDITY，避免误操作另一封邮件。
- 列表展示同步状态；同步出错不代表没有邮件，已有缓存仍可搜索。原始邮件超过 25 MiB 不进入缓存。
- 搜索空关键词返回当前缓存的最新邮件；多词按 AND 匹配，`%`、`_` 按普通字符检索。
- 服务不会自动清空垃圾箱或永久删除远端邮件。移动只在服务商支持 IMAP MOVE 时执行；移动出同步范围后不再显示。
- SMTP 超时或连接中断可能发生在服务商已接受之后。`unknown` 结果不会自动重发；应人工核对邮箱。准备操作超过 365 天变为 `expired`，不能再发送；终态及过期准备会清除正文，只保留编号、请求摘要、Message-ID 和状态用于去重核对。`sent` 只表示 SMTP 接受，不保证收件人最终投递。
- 邮箱侧的已读/星标变化不会通过此版本持续刷新全部缓存；通过 MCP 完成的操作立即更新本地状态。
- 备份数据库时先停止服务，再复制数据库文件，完成后恢复服务；不要在运行中仅复制 `.db` 而漏掉 WAL。备份密钥应另行加密保护。

## 客户端权限与回复接口

客户端连接地址填服务根 URL，不带 `/api` 或 `/mcp`。`GET /api/accounts` 返回 `can_write`，表示当前访问令牌是否允许写操作。不要仅依赖客户端隐藏按钮，服务端始终执行权限校验。

- `POST /api/messages/{id}/reply/prepare`：可写令牌提交 `operation_id` 和 `body_text`，只准备回复；返回发件账户、收件人、主题、正文及状态供用户确认。
- `POST /api/send/{operation_id}/send`：用户确认后执行一次发送。
- `GET /api/send/{operation_id}`：查询 `prepared / sending / sent / failed / unknown / expired` 状态及 Message-ID；不会重新发送。

发送失败、响应丢失或客户端重启后，先查询原编号，不自动生成新编号重发。`unknown` 需要核对邮箱；`sent` 仅代表 SMTP 接受。

## 本机检查与 CI

macOS 使用 `swift run --package-path macos MailSearchChecks` 执行原生可执行检查，不依赖完整 Xcode 的 XCTest 环境。`scripts/build-macos-search.sh` 生成 `macos/build/AI Email.app`，CI 用 tar.gz 保留可执行权限；当前构建不承担正式签名、公证或发布。

macOS 构建按图标内容生成资源文件名，避免替换图标后系统继续使用同名缓存。修改 `macos/AppIcon.icns` 后重新构建并重新打开 App。

Git 检查使用独立安装的 `lefthook` 和 `gitleaks`，不依赖 Node。macOS 可通过 `brew install lefthook gitleaks` 安装，再运行 `lefthook install`。现有 Git hooks 会优先调用 PATH 中的 lefthook；不得用 `--no-verify` 跳过失败检查。提交前检查 Rust 格式、clippy、凭据及标题格式；推送前检查服务端测试与 Python 桥接/冒烟。Android 和 macOS 检查按改动范围执行，CI 分别构建。

网络传输回归测试只监听本机临时端口，受限执行环境可能需要允许 localhost 监听；这不需要真实邮箱凭据。
