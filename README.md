# AI Email

AI 辅助的邮件客户端 —— Tauri 2 桌面(macOS)+ Android,本地优先、开箱即用。

## 功能

- 多语言翻译
- 自动分类 / 打标签 / 优先级排序
- 长邮件与线程摘要
- AI 起草回复(人工审核后发送)

## 技术栈

- **Tauri 2** + React + TypeScript(前端)
- **Rust** 核心:`async-imap` / `lettre`(IMAP/SMTP)、`reqwest` → Anthropic API
- **嵌入式 SQLite**(`sqlx`):数据库文件在 OS app 数据目录,首次启动自动创建并迁移 —— 零配置、离线、无需数据库服务
- 凭据(QQ 授权码、AI API key)存 OS keychain,从不入库

## 快速开始

前置:Node ≥ 22.13、pnpm 11、Rust stable。**无需安装/运行任何数据库。**

```bash
pnpm install
pnpm tauri dev      # 开发模式(桌面)
```

首次运行会在 app 数据目录自动建 SQLite 库并跑迁移。在应用内添加 QQ 邮箱(授权码)与 AI 模型(Anthropic key)即可使用。

构建发布包:

```bash
pnpm tauri build                                   # macOS 桌面 .app / .dmg
pnpm tauri android build --apk --target aarch64    # Android arm64 APK
```

## 平台

macOS(arm64)+ Android(arm64-v8a)。

## 文档

- 设计与数据模型:[docs/SPEC.md](docs/SPEC.md)
- 协作规范(给 Claude 与人):[CLAUDE.md](CLAUDE.md)
- 快速上手:[ONBOARDING.md](ONBOARDING.md)
