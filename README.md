# AI Email

AI 辅助的邮件客户端 —— Tauri 2 桌面(macOS)+ Android,本地优先、开箱即用。

## 功能

- 多语言翻译
- 自动分类 / 打标签 / 优先级排序
- 长邮件与线程摘要
- AI 双语起草回复:中文意图 → 外文回复 + 回译核对,人工审核后发送
- 摘要 / 翻译 / 写信统一收进右侧 AI 抽屉
- 多账户统一收件箱 + 信箱切换:跨账户聚合视图,或按账户浏览收件箱 / 已发送 / 草稿 / 废纸篓 / 垃圾邮件等信箱;支持未读筛选与一键全部已读
- 邮件操作:删除(移到废纸篓,可找回)、标记已读 / 未读、加星 / 取消;打开邮件自动标记已读
- 自动回复中心:规则把需要回复的新邮件筛进「建议回复」队列,一键起草双语回复审阅后发送(不自动发出)
- 支持 QQ 邮箱 / 腾讯企业邮 / Gmail;设置中心统一管理邮箱账户(增 / 删 / 改)与 AI 模型

## 技术栈

- **Tauri 2** + React + TypeScript(前端)
- **Rust** 核心:`async-imap` / `lettre`(IMAP/SMTP)、`reqwest` → Anthropic API
- **嵌入式 SQLite**(`sqlx`):数据库文件在 OS app 数据目录,首次启动自动创建并迁移 —— 零配置、离线、无需数据库服务
- 凭据(邮箱授权码 / 客户端专用密码、AI API key)存 OS keychain,从不入库

## 快速开始

前置:Node ≥ 22.13、pnpm 11、Rust stable。**无需安装/运行任何数据库。**

```bash
pnpm install
pnpm tauri dev      # 开发模式(桌面)
```

首次运行会在 app 数据目录自动建 SQLite 库并跑迁移。在应用内设置中心添加邮箱账户(QQ / 腾讯企业邮 / Gmail,授权码或客户端专用密码)与 AI 模型(Anthropic key)即可使用。

构建发布包:

```bash
pnpm tauri build                                   # macOS 桌面 .app / .dmg
pnpm build:android    # Android arm64 APK
```

## 平台

macOS(arm64)+ Android(arm64-v8a)。

## 文档

- 协作规范(给 Claude 与人):[CLAUDE.md](CLAUDE.md)
- 快速上手:[ONBOARDING.md](ONBOARDING.md)
- 系统技能文档:[skills/](skills/)
