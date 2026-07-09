# Onboarding

Quick-start for new contributors and fresh Claude sessions.

## TL;DR

AI-assisted email client (desktop + Android) built with **Tauri 2 + React/TS + Rust**.
Primary email provider: **QQ Mail**. AI via configured **Anthropic or OpenAI-compatible providers**.

For the rules of engagement see [`CLAUDE.md`](CLAUDE.md).

## Prerequisites

| Tool          | Install (macOS via Homebrew)                                                                           |
| ------------- | ------------------------------------------------------------------------------------------------------ |
| Node 22.13+   | `brew install node`                                                                                    |
| pnpm 11       | `brew install pnpm`                                                                                    |
| Rust (stable) | `brew install rustup && rustup toolchain install stable --profile default && brew link --force rustup` |
| lefthook      | `brew install lefthook`                                                                                |
| gitleaks      | `brew install gitleaks`                                                                                |
| jq            | `brew install jq` (usually pre-installed)                                                              |

Optional for Android builds: Android Studio + NDK r26b, Java 17 (`brew install --cask android-studio temurin@17`).

For mainland China users, set rustup mirror before installing the toolchain:

```bash
export RUSTUP_DIST_SERVER="https://rsproxy.cn"
export RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"
```

`.cargo/config.toml` and `.npmrc` are already configured to use rsproxy / npmmirror.

## First-time setup

```bash
# 1. Install deps
pnpm install

# 2. Install git hooks
lefthook install

# 3. Set up env
cp .env.example .env
# Edit .env: add your AI provider API key, and (dev only) QQ Mail credentials.
# 数据库为本地 SQLite，首次启动 (pnpm tauri dev) 自动创建于 OS app-data 目录并
# 跑 migrations，无需额外步骤。

# 4. Verify everything is wired
lefthook run pre-commit --all-files
```

## Day-to-day

```bash
# Desktop dev
pnpm tauri dev

# Android dev (requires NDK + emulator/device)
pnpm tauri android dev

# Release builds
pnpm build:macos
pnpm build:android

# Tests
pnpm test:run
cd src-tauri && cargo test --all-features

# Lint
pnpm exec prettier --check .
pnpm exec eslint --max-warnings 0 .
pnpm exec tsc --noEmit
cd src-tauri && cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings

# Security audit
pnpm audit --registry https://registry.npmjs.org/
cd src-tauri && cargo audit --ignore RUSTSEC-2023-0071
```

Android release signing is optional for local smoke builds. To create a signed release, provide `ANDROID_RELEASE_STORE_FILE` (or `ANDROID_RELEASE_KEYSTORE_BASE64` in CI), `ANDROID_RELEASE_STORE_PASSWORD`, `ANDROID_RELEASE_KEY_ALIAS`, and `ANDROID_RELEASE_KEY_PASSWORD` before running `pnpm build:android`.

## QQ Mail setup (dev account)

1. Log in to mail.qq.com → 设置 → 账户
2. Find "POP3/IMAP/SMTP/Exchange/CardDAV/CalDAV服务" → 开启 IMAP/SMTP
3. Generate **授权码** (auth code). This is what the app uses, NOT your QQ password.
4. Paste into `.env` as `DEV_QQ_AUTH_CODE`.

Connection parameters:

- IMAP: `imap.qq.com:993` (TLS)
- SMTP: `smtp.qq.com:465` (TLS)

## Architecture overview

The frontend is a React + TypeScript Tauri webview. UI code lives under `src/`, with Tauri command wrappers centralized in `src/lib/tauri.ts` so components do not call `invoke` directly.

The Rust core lives under `src-tauri/src/`. IMAP sync/parsing, SMTP send, AI calls, SQLite repositories, keychain access, attachment save dialogs, and `#[tauri::command]` handlers stay on the Rust side. The frontend receives app-state events such as database readiness and sync updates through Tauri events.

Data is local-first: SQLite is created and migrated in the OS app-data directory on first launch, while mailbox auth codes and AI API keys are stored in the OS keychain. Production builds target macOS arm64 and Android arm64-v8a.

## Common tasks

- Add or update a Tauri command: implement the Rust handler under `src-tauri/src/commands/`, register it in `src-tauri/src/lib.rs`, then add a typed wrapper in `src/lib/tauri.ts`; keep command-layer validation and capability tests in sync.
- Change mail sync or parsing behavior: update Rust unit/integration tests first, especially around UID validity, flags, HTML/plaintext parsing, and attachment materialization.
- Change AI behavior: update prompt/response parsing tests and keep provider base URLs HTTPS-only.
- Change release packaging: use `pnpm build:macos` and `pnpm build:android`, then keep README, `CLAUDE.md`, and CI in sync.
- Run a local release sanity check: `pnpm test:coverage`, `cd src-tauri && cargo test --all-features`, `pnpm audit --registry https://registry.npmjs.org/`, and both release build scripts.
