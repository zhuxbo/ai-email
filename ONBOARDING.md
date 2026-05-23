# Onboarding

Quick-start for new contributors and fresh Claude sessions.

## TL;DR

AI-assisted email client (desktop + Android) built with **Tauri 2 + React/TS + Rust**.
Primary email provider: **QQ Mail**. AI via **Anthropic API**.

For the rules of engagement see [`CLAUDE.md`](CLAUDE.md).

## Prerequisites

| Tool              | Install (macOS via Homebrew)                                                                           |
| ----------------- | ------------------------------------------------------------------------------------------------------ |
| Node 20+          | `brew install node`                                                                                    |
| pnpm 9+           | `brew install pnpm`                                                                                    |
| Rust (stable)     | `brew install rustup && rustup toolchain install stable --profile default && brew link --force rustup` |
| OrbStack (Docker) | `brew install --cask orbstack`                                                                         |
| lefthook          | `brew install lefthook`                                                                                |
| gitleaks          | `brew install gitleaks`                                                                                |
| jq                | `brew install jq` (usually pre-installed)                                                              |

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

# 3. Start local PostgreSQL 18 (via OrbStack)
docker compose up -d postgres
docker compose ps                  # verify "healthy"

# 4. Set up env
cp .env.example .env
# Edit .env: add your Anthropic API key, and (dev only) QQ Mail credentials.
# DATABASE_URL defaults already match the local compose container.

# 5. Verify everything is wired
lefthook run pre-commit --all-files
```

## Day-to-day

```bash
# Desktop dev
pnpm tauri dev

# Android dev (requires NDK + emulator/device)
pnpm tauri android dev

# Tests
cd src-tauri && cargo test
pnpm exec vitest

# Lint
cd src-tauri && cargo fmt && cargo clippy --no-deps -- -D warnings
pnpm exec prettier --check . && pnpm exec eslint .
```

## QQ Mail setup (dev account)

1. Log in to mail.qq.com → 设置 → 账户
2. Find "POP3/IMAP/SMTP/Exchange/CardDAV/CalDAV服务" → 开启 IMAP/SMTP
3. Generate **授权码** (auth code). This is what the app uses, NOT your QQ password.
4. Paste into `.env` as `DEV_QQ_AUTH_CODE`.

Connection parameters:

- IMAP: `imap.qq.com:993` (TLS)
- SMTP: `smtp.qq.com:465` (TLS)

## Architecture overview

_To be filled in as we build. See `CLAUDE.md` "Repo layout" for the current target structure._

## Common tasks

_To be filled in once Sprint 1 lands._
