# AI Email

AI 辅助的邮件客户端 —— Tauri 2 桌面(macOS)+ Android,本地优先、开箱即用。

## 功能

- 多语言翻译
- 自动分类 / 打标签 / 优先级排序;防误导规则避免服务商反垃圾标记(如 `[SPAM]` / `★垃圾邮件★`)被误判,并把商业服务商的事务性通知(如证书签发 / 账单 / 订单状态)归为通知而非推广;详情区「分类」按钮可手动改类并锁定(AI 不再覆写);prompt 更新后启动时后台一次性重跑存量分类
- 发件人黑白名单:按邮箱地址或域名(含 `*.x.com` 通配子域)维护黑 / 白名单,在设置中心管理;黑名单来信分类时直接归垃圾(跳过 AI 调用),白名单确保不被误判为垃圾
- 长邮件与线程摘要
- AI 双语起草回复:中文意图 → 外文回复 + 回译核对,人工审核后发送
- 摘要 / 翻译 / 写信统一收进右侧 AI 抽屉
- 收件箱列表折叠聚合:同一会话折叠成一行;孤立的同发件人通知 / 推广邮件折叠成一行(显示最新一封 + 数量角标);点同发件人折叠组在详情区以会话流展示(默认全折叠)
- 多账户统一收件箱 + 信箱切换:跨账户聚合视图,或按账户浏览收件箱 / 已发送 / 草稿 / 废纸篓 / 垃圾邮件等信箱;支持未读筛选与一键全部已读
- 自动收信:窗口开启时按设定间隔(默认 5 分钟,设置中心可改 关 / 1 / 5 / 15 / 30)自动收取全部账户收件箱;全局顶栏指示器显示上次同步 / 下次倒计时 / 同步中 / 失败,点击即立即同步
- 邮件操作:删除(移到废纸篓,可找回)、标记已读 / 未读、加星 / 取消;打开邮件自动标记已读;会话流内每封邮件各自展示附件(懒加载,点击后由后端原生保存框另存为)
- HTML 邮件防追踪:正文经清洗去除脚本与内联样式,并在 Shadow DOM 内注入基础邮件样式限制图片 / 表格撑宽;远程图片仅对私人 / 工作邮件默认加载,其余默认拦截(防 tracking pixel 暴露已读与 IP),可一键显示;内联 `cid:` 图片会转成本地 `data:` URL 后再渲染,无法匹配 MIME 图片部件的 `cid:` 图片会移除不可加载的 `src`;过滤卡片中纯 HTML 邮件自动转纯文本显示(不再暴露 HTML 源码)
- 本地诊断日志:启动后写入 app 数据目录的 `logs/ai-email.log`,记录 IMAP 连接 / TLS / 登录 / LIST / SELECT / FETCH / STORE / MOVE 阶段耗时与失败原因,便于排查间歇性超时
- 自动回复中心:规则把需要回复的新邮件筛进「建议回复」队列,一键起草双语回复审阅后发送(不自动发出)
- 支持 QQ 邮箱 / 腾讯企业邮 / Gmail;设置中心统一管理邮箱账户(增 / 删 / 改)、AI 模型与本地正文 / AI 缓存清理

## 技术栈

- **Tauri 2** + React + TypeScript(前端)
- **Rust** 核心:`async-imap` / `lettre`(IMAP/SMTP)、`reqwest` → Anthropic / OpenAI-compatible API
- **嵌入式 SQLite**(`sqlx`):数据库文件在 OS app 数据目录,首次启动自动创建并迁移 —— 零配置、离线、无需数据库服务
- 凭据(邮箱授权码 / 客户端专用密码、AI API key)存 OS keychain,从不入库

## 快速开始

前置:Node ≥ 22.13、pnpm 11、Rust stable。**无需安装/运行任何数据库。**

```bash
pnpm install
pnpm tauri dev      # 开发模式(桌面)
```

首次运行会在 app 数据目录自动建 SQLite 库并跑迁移。在应用内设置中心添加邮箱账户(QQ / 腾讯企业邮 / Gmail,授权码或客户端专用密码)与 AI 模型(Anthropic 或 OpenAI-compatible key)即可使用。

构建发布包:

```bash
pnpm build:macos      # macOS 桌面 .app / .dmg
pnpm build:android    # Android arm64 APK
```

发布前基础验证:

```bash
pnpm exec prettier --check .
pnpm exec eslint --max-warnings 0 .
pnpm exec tsc --noEmit
pnpm test:coverage
pnpm audit --registry https://registry.npmjs.org/
cd src-tauri && cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features && cargo audit --ignore RUSTSEC-2023-0071
```

React Hooks 编译器诊断保持开启；依赖升级引入新诊断时，应修正组件状态生命周期，不在 ESLint 中关闭规则。

Android 更新比较使用 Tauri 默认的 SemVer `versionCode` 公式；版本号保持 `package.json`、`tauri.conf.json` 与 `Cargo.toml` 一致，不依赖未跟踪的 Android 生成文件。

`RUSTSEC-2023-0071` 是窄范围忽略项：`Cargo.lock` 会因 `sqlx-macros` 的可选 MySQL 后端路径列出 `rsa`，但本项目禁用 `sqlx` 默认特性且只启用 SQLite，实际编译/运行路径不使用 MySQL/RSA。

> macOS 发布包是未签名的 DMG。用户从 GitHub Release 手动下载 DMG，退出旧版应用后以新应用替换旧应用；本地 `pnpm build:macos` 同样不需要发布凭据。

Android release 签名可通过环境变量启用；不设置时仍产出 unsigned APK，适合本地冒烟验证：

```bash
export ANDROID_RELEASE_STORE_FILE="/path/to/release.jks"
export ANDROID_RELEASE_STORE_PASSWORD="..."
export ANDROID_RELEASE_KEY_ALIAS="..."
export ANDROID_RELEASE_KEY_PASSWORD="..."
pnpm build:android
```

只有受保护的 `release.yml` 发布工作流可以注入 GitHub signing secrets，其中 `ANDROID_RELEASE_KEYSTORE_BASE64` 会由脚本临时解码并自动设置 `ANDROID_RELEASE_STORE_FILE`。普通 CI 必然构建 unsigned APK。不要把 `.jks` 或 `.keystore` 文件提交到仓库。

正式发布由 [release.yml](.github/workflows/release.yml) 完成。本地 `build:*` 只用于冒烟，不能替代受保护的 CI 发布。

1. 在 GitHub 创建受保护的 `release` Environment，并限制为发布维护者使用。仅配置 Android JKS Secrets：`ANDROID_RELEASE_KEYSTORE_BASE64`、`ANDROID_RELEASE_STORE_PASSWORD`、`ANDROID_RELEASE_KEY_ALIAS`、`ANDROID_RELEASE_KEY_PASSWORD`。
2. 配置 Environment Variable：`ANDROID_RELEASE_CERT_SHA256`，其值为 Android 发布证书的 SHA-256 指纹。
3. 在 `main` 同步 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 的版本后，创建带更新说明的注释 SemVer 标签并推送：

```bash
git tag -a v0.1.0 -m "更新说明"
git push origin v0.1.0
```

- 工作流仅接受 `main` 上的 `v<major>.<minor>.<patch>` 注释标签；Actions 手动运行时选择 `main` 并输入已有标签。
- build Job 只使用 Android JKS 凭据与证书指纹，校验 signed APK，并生成未签名 macOS DMG；无凭据的 publish Job 才创建公开 stable GitHub Release。
- Release 只包含 `android-latest.json`、`ai-email_<version>_aarch64.dmg` 和 `ai-email_<version>_arm64-v8a.apk`；资产名称和校验清单由 `scripts/prepare-release-assets.mjs` 统一生成。发布作业会检出对应标签，以校验并创建不可变公开 Release，供客户端匿名下载。
- 已存在同标签 Release 时工作流会失败且不覆盖资产；检查或处理失败的 draft 后再重新触发。保护 `v*` 标签，禁止移动或删除。
- macOS 用户检查 GitHub Release 后打开对应 DMG URL，手动以新应用替换旧应用；Android Release 只允许 signed APK，unsigned APK 仅用于本地冒烟。

## 平台

macOS(arm64)+ Android(arm64-v8a)。

## 文档

- 协作规范(给 Claude 与人):[CLAUDE.md](CLAUDE.md)
- 快速上手:[ONBOARDING.md](ONBOARDING.md)
- 系统技能文档:[skills/](skills/)
