# CLAUDE.md — AI Email Project Constitution

Source of truth for **how** Claude (and humans) work in this codebase.
For **what** we're building, see [`README.md`](README.md).
Keep this file short. When in doubt, ask the user.

---

## What this is

AI-assisted email client. Tauri 2 desktop (macOS) + Android (arm64-v8a).
Primary email provider: **QQ Mail** (IMAP/SMTP + authorization code); 腾讯企业邮 (exmail) + Gmail also supported via IMAP/SMTP.
AI calls go through configured **Anthropic or OpenAI-compatible providers**. Keep role defaults explicit in app settings.

## Repo layout

```
ai-email/
├── src/                      # React + TypeScript frontend
├── src-tauri/                # Tauri Rust core
│   └── src/
│       ├── imap/             # IMAP client + parsing
│       ├── smtp/             # SMTP send
│       ├── ai/               # Anthropic / OpenAI-compatible clients + prompts
│       ├── db/               # SQLite layer (sqlx + migrations)
│       └── commands/         # #[tauri::command] handlers
├── .github/                  # CI workflows, dependabot, PR template
├── .claude/                  # Claude Code settings + hooks
├── skills/                   # 系统技能文档(开发细节 / 操作手册)
├── CLAUDE.md                 # this file
└── ONBOARDING.md             # quick-start context
```

## 文档组织

- **系统技能文档**(开发细节、操作手册)→ 根目录 `skills/`,入库。
- **过程文档**(plan / 设计稿 / 调试记录)→ `.superpowers/`,git 忽略,**不被任何代码、注释或入库文档引用**。
- `CLAUDE.md` / `README.md` 保持精简;细节进对应 skill。

## Tech decisions (LOCKED — confirm with user before changing)

- **Tauri 2** — no Electron, no separate web server
- **React + TypeScript**, strict mode everywhere
- **Rust** for the core (IMAP / SMTP / DB / AI / Tauri commands)
- **SQLite** via `sqlx` (embedded, bundled libsqlite3 — no server). DB file lives in the OS app-data dir, created + migrated on first launch → zero-config, offline, works on desktop + Android
- **`async-imap`** + **`lettre`** for mail
- **IMAP behavior**: connect timeout covers TCP + TLS + LOGIN + ID (60s); command/body timeouts stay separate. Inline `cid:` images are materialized as `data:` URLs during body parsing; unresolved `cid:` image sources must be neutralized so cached bodies do not refetch forever.
- **HTML email rendering**: render sanitized HTML in Shadow DOM with app-owned base CSS for image/table width constraints; do not rely on remote email CSS for layout safety.
- **Logs**: runtime diagnostics append to `<app-data>/logs/ai-email.log` with startup rotation; IMAP command logs must include the command/phase and elapsed time.
- **Anthropic / OpenAI-compatible APIs** via `reqwest` (no third-party AI wrappers)
- **pnpm** as package manager (NOT npm or yarn)
- **Mirrors**: `npmmirror.com` for npm, `rsproxy.cn` for cargo
- **Release entrypoints**: `pnpm build:macos` and `pnpm build:android` are low-level build entrypoints; `.github/workflows/release.yml` is the only production build/publish entrypoint
- **Android release signing**: optional via local `ANDROID_RELEASE_*` env vars; only the protected `release.yml` release workflow may inject GitHub signing secrets, while ordinary CI always builds unsigned APKs; never commit keystores
- **Release automation**: `release.yml` accepts only annotated stable tags from `main`, separates secret-bearing build from `contents: write` publish, and stores signing material only in the `release` GitHub Environment
- **macOS delivery**: macOS checks GitHub Release, then opens the versioned DMG URL and relies on manual app replacement; it has no updater-signature, certificate, or notarization release gate
- **Release assets**: public stable releases contain exactly `android-latest.json`, `ai-email_<version>_aarch64.dmg`, and `ai-email_<version>_arm64-v8a.apk`; Android APK certificate SHA-256 validation happens before artifact transfer
- **Signing boundaries**: the protected build job accepts only Android JKS credentials and `ANDROID_RELEASE_CERT_SHA256`; publish has no release credentials and only `contents: write`
- **npm security overrides** live in `pnpm-workspace.yaml`, not `package.json`'s deprecated `pnpm` field

## Code conventions

### Rust

- Edition 2021, `max_width = 100`
- Errors: `thiserror` for libraries, `anyhow` for binaries
- Async I/O via `tokio` (single runtime, no mixing)
- Tauri commands return `Result<T, AppError>` — never panic across FFI
- Use `tracing` (NOT `log` or `println!`) for instrumentation
- Credentials wrapped in `secrecy::Secret<String>`; never log them

### TypeScript

- Strict mode + `noUncheckedIndexedAccess` + `exactOptionalPropertyTypes`
- No `any`. Use `unknown` + narrowing.
- Async functions only — no `.then` chains
- Tauri commands wrapped with typed helpers in `src/lib/tauri.ts`; UI never calls `invoke` directly

### Naming

|           | Rust                   | TypeScript             |
| --------- | ---------------------- | ---------------------- |
| Files     | `snake_case.rs`        | `kebab-case.ts`        |
| Types     | `PascalCase`           | `PascalCase`           |
| Functions | `snake_case`           | `camelCase`            |
| Constants | `SCREAMING_SNAKE_CASE` | `SCREAMING_SNAKE_CASE` |

Tauri commands: `snake_case` in Rust → auto-converted to `camelCase` for JS.

## Testing

- Rust unit tests: `#[cfg(test)] mod tests` next to source
- Rust integration tests: `src-tauri/tests/`
- TS unit tests: `*.test.ts` co-located with source
- TS integration tests: `tests/`
- **IMAP integration: use in-memory IMAP server. NEVER hit real QQ Mail in tests.**
- **AI prompts: regression tests via semantic match (LLM-judge or embedding). NEVER literal string compare.**
- Coverage thresholds (Tier 2): Rust core ≥70%, TS ≥60%

## Commits

Conventional Commits, enforced by `commitlint`:

`feat:`, `fix:`, `refactor:`, `perf:`, `test:`, `docs:`, `chore:`, `ci:`, `build:`, `style:`, `revert:`

Body required when the change isn't obvious from the title.

## Quality Gates (STRICT — no exceptions)

Before any commit lands, lefthook runs:

1. `cargo fmt --check`
2. `cargo clippy --no-deps -- -D warnings`
3. `pnpm exec prettier --check`
4. `pnpm exec eslint --max-warnings 0`
5. `pnpm exec tsc --noEmit`
6. `gitleaks protect --staged`

Pre-push adds:

- `cargo test`
- `vitest run`
- `cargo audit --ignore RUSTSEC-2023-0071`

**Never bypass with `--no-verify`.** If a check fails, fix the underlying issue.

`RUSTSEC-2023-0071` is ignored narrowly because `Cargo.lock` includes `rsa` through `sqlx-macros`' optional MySQL backend path; this app disables sqlx default features and only enables SQLite, so the MySQL/RSA path is not compiled or used. Do not add broader audit ignores without documenting the reachable path analysis.

For dependency audits, keep `pnpm audit --registry https://registry.npmjs.org/` clean. If a transitive npm advisory needs an override, add the narrowest selector to `pnpm-workspace.yaml` and refresh `pnpm-lock.yaml`.

**完成守卫**：实质改动提交前跑 `/finish-check` —— 在上述自动门之上叠加范围审查、删除审核与独立 reviewer 循环（落盘 `REVIEW_PASS:` 签字才算完成）。主指令见 `.claude/commands/finish-check.md`，反模式与 reviewer 模板见 `skills/review-checklist.md`。

## Claude hooks (this repo's `.claude/settings.json`)

- `PostToolUse` on `*.rs` edits → `rustfmt --check`, non-zero blocks the tool result
- `PostToolUse` on `*.ts|*.tsx|*.js|*.json|*.md` edits → `prettier --check`, non-zero blocks
- `Stop` → if Rust/TS files changed this session, runs `cargo check` / `tsc --noEmit`; non-zero prevents end-of-turn

## Security

- Never commit `.env`, credentials, or anything matching `.gitleaks.toml`
- QQ Mail auth code lives in the OS keychain via the `keyring` crate (macOS Keychain; Android KeyStore via `android-keyring`) — NEVER in plaintext config or DB
- AI provider API keys: dev = `.env` (gitignored), prod = OS keychain
- IMAP / SMTP / AI provider network calls use TLS — no exceptions

## What NOT to do

- Don't add backwards-compat shims for code we haven't shipped yet
- Don't comment WHAT the code does — only WHY when non-obvious
- Don't add a new dep without checking its weekly download count + last-update date
- Don't silence a lint to make CI pass — fix the root cause; React Hooks compiler diagnostics stay enabled
- Don't make runtime Rust code depend on ignored Android generated files; use synchronized source version metadata
- Don't duplicate release asset naming in workflow shell; use the release asset helper as the single source
- Don't write to disk outside the Tauri app data directory, except user-selected attachment saves from the backend native save dialog
- Don't call IMAP / SMTP / AI from the frontend — always go through a Tauri command

## When unsure

Ask the user. Do not guess on architecture, do not silently broaden scope.
