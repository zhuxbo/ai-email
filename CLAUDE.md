# CLAUDE.md — AI Email Project Constitution

Source of truth for **how** Claude (and humans) work in this codebase.
For **what** we're building, see [`README.md`](README.md).
Keep this file short. When in doubt, ask the user.

---

## What this is

AI-assisted email client. Tauri 2 desktop (macOS) + Android (arm64-v8a).
Primary email provider: **QQ Mail** (IMAP/SMTP + authorization code); 腾讯企业邮 (exmail) + Gmail also supported via IMAP/SMTP.
AI calls go to **Anthropic API** — Haiku 4.5 for classification, Sonnet 4.6 for summarization / translation / drafting, Opus 4.7 only for complex threads.

## Repo layout

```
ai-email/
├── src/                      # React + TypeScript frontend
├── src-tauri/                # Tauri Rust core
│   └── src/
│       ├── imap/             # IMAP client + parsing
│       ├── smtp/             # SMTP send
│       ├── ai/               # Anthropic client + prompts
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
- **Anthropic API** via `reqwest` (no third-party AI wrappers)
- **pnpm** as package manager (NOT npm or yarn)
- **Mirrors**: `npmmirror.com` for npm, `rsproxy.cn` for cargo

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
- `cargo audit`

**Never bypass with `--no-verify`.** If a check fails, fix the underlying issue.

**完成守卫**：实质改动提交前跑 `/finish-check` —— 在上述自动门之上叠加范围审查、删除审核与独立 reviewer 循环（落盘 `REVIEW_PASS:` 签字才算完成）。主指令见 `.claude/commands/finish-check.md`，反模式与 reviewer 模板见 `skills/review-checklist.md`。

## Claude hooks (this repo's `.claude/settings.json`)

- `PostToolUse` on `*.rs` edits → `rustfmt --check`, non-zero blocks the tool result
- `PostToolUse` on `*.ts|*.tsx|*.js|*.json|*.md` edits → `prettier --check`, non-zero blocks
- `Stop` → if Rust/TS files changed this session, runs `cargo check` / `tsc --noEmit`; non-zero prevents end-of-turn

## Security

- Never commit `.env`, credentials, or anything matching `.gitleaks.toml`
- QQ Mail auth code lives in the OS keychain via the `keyring` crate (macOS Keychain; Android KeyStore via `android-keyring`) — NEVER in plaintext config or DB
- Anthropic API key: dev = `.env` (gitignored), prod = OS keychain
- All HTTP calls (IMAP / SMTP / Anthropic) use TLS — no exceptions

## What NOT to do

- Don't add backwards-compat shims for code we haven't shipped yet
- Don't comment WHAT the code does — only WHY when non-obvious
- Don't add a new dep without checking its weekly download count + last-update date
- Don't silence a lint to make CI pass — fix the root cause
- Don't write to disk outside the Tauri app data directory
- Don't call IMAP / SMTP / AI from the frontend — always go through a Tauri command

## When unsure

Ask the user. Do not guess on architecture, do not silently broaden scope.
