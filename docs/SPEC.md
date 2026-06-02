# AI Email — MVP Long-Range Spec

**Status:** Draft v1 · 2026-05-23
**Owner:** zhuxbo
**Goal:** Ship a usable AI-assisted email client for QQ Mail on desktop in ~5 sprints, then port to Android.

This document is the single source of truth for _what_ we're building. `CLAUDE.md` is the constitution (_how_ we work). This spec is updated whenever scope or architecture shifts; every sprint kickoff should re-read § 6.

---

## 0. Why this exists

Email overhead — triage, reading long threads, multilingual senders, drafting replies — eats focus time. Off-the-shelf clients can't be customised to a personal workflow, and most cloud-AI email products send everything you read into a third-party server. This client:

- runs on your own desktop (and later Android),
- stores email in a **local SQLite** database on your own device (zero-config, offline),
- calls **Claude API directly** with prompt caching, and
- **never auto-sends** at MVP — every reply gets human review.

## 1. MVP cut

**In MVP (Sprints 1–5):**

- Desktop client (macOS / Windows / Linux)
- One QQ Mail account, IMAP/SMTP via authorization code
- Inbox list + message detail (header + body, lazy-loaded)
- AI summary (Sonnet 4.6)
- AI classification + priority (Haiku 4.5, batched)
- AI translation to user's UI language
- AI-drafted reply, edited by user, sent on explicit click

**Out of MVP (phase 2+):**

- Android port (Sprint 7)
- Multi-account, IMAP IDLE live sync (Sprint 6)
- Auto-reply / whitelist rules (Sprint 8)
- Other providers (163, Gmail OAuth, Outlook)
- iOS
- Multi-language UI

## 2. Architecture

```
┌───────────────────────────────────────────────────┐
│ React 19 + TypeScript (Vite)                      │
│ 3-pane shell: Accounts │ MessageList │ Detail+AI  │
└───────────────────────┬───────────────────────────┘
                        │ Tauri invoke (typed wrappers in src/lib/tauri.ts)
┌───────────────────────▼───────────────────────────┐
│ Rust core (Tauri 2)                               │
│  imap/      async-imap → fetch headers / bodies   │
│  smtp/      lettre     → send drafts              │
│  ai/        reqwest    → Anthropic + prompt cache │
│  db/        sqlx       → SQLite (embedded)        │
│  keychain/  keyring    → OS-native credential vault│
│  commands/  #[tauri::command] handlers (API)      │
└───────────────────────┬───────────────────────────┘
                        │ TLS
       ┌────────────────┴────────────────┐
       ▼                                 ▼
   QQ IMAP/SMTP                     Anthropic API

   SQLite (embedded, local file in OS app-data dir — no network)
```

### Architectural rules

1. **All I/O in Rust.** The frontend never speaks IMAP / SMTP / Anthropic / DB directly. Tauri commands are the only API surface.
2. **SQLite, not PostgreSQL.** `2026-06-02 由 PostgreSQL 改为 SQLite`（原 2026-05-23 选 PG）。理由：开箱即用——嵌入式本地库，零配置、离线、用户无需自建任何数据库服务；桌面与移动端（Android）用本地嵌入式存储是标准做法，手机 app 也无法直连远程 PG。多设备同步降级为未来可选项（如需，可在 SQLite 之上加一层同步），不再作为选型理由。
3. **No third-party AI wrappers.** Direct `reqwest` calls to Anthropic give full control over prompt caching, model routing, retry.
4. **Credentials never in DB or config.** All auth codes / API keys live in OS keychain via `keyring` crate.
5. **Tauri 2 over Electron.** Smaller bundle, native menus, and Tauri 2 mobile is the planned Android port path.

## 3. Data model (v1)

Migrations live in `src-tauri/migrations/` (SQLite dialect). Initial migration is `0001_initial.sql`.

SQLite mapping vs the original PostgreSQL types: `UUID → BLOB` (16 bytes, generated app-side via `Uuid::new_v4()` — no `gen_random_uuid()`); `TIMESTAMPTZ → TEXT` (RFC3339/UTC, DB default via `strftime`); `TEXT[] → TEXT` (JSON array); `JSONB → TEXT` (JSON value); `BOOL → INTEGER` (0/1). Tables / columns / constraints / semantics are unchanged.

```sql
-- Email accounts. Credentials live in OS keychain keyed by accounts.id —
-- this table holds only non-secret metadata.
CREATE TABLE accounts (
  id              BLOB    PRIMARY KEY,
  email           TEXT    NOT NULL UNIQUE,
  display_name    TEXT,
  provider        TEXT    NOT NULL,    -- 'qq' | '163' | 'gmail' | 'outlook' | 'imap'
  imap_host       TEXT    NOT NULL,
  imap_port       INTEGER NOT NULL DEFAULT 993,
  smtp_host       TEXT    NOT NULL,
  smtp_port       INTEGER NOT NULL DEFAULT 465,
  created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
  last_synced_at  TEXT
);

-- IMAP folders, mirrored per account.
CREATE TABLE mailboxes (
  id              BLOB    PRIMARY KEY,
  account_id      BLOB    NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  name            TEXT    NOT NULL,                  -- 'INBOX' | '已发送' | '收件箱' …
  delimiter       TEXT,
  uid_validity    INTEGER,
  uid_next        INTEGER,
  last_synced_at  TEXT,
  UNIQUE (account_id, name)
);

-- Header-only rows, keyed by IMAP UID. Bodies live in message_bodies.
CREATE TABLE messages (
  id              BLOB    PRIMARY KEY,
  account_id      BLOB    NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  mailbox_id      BLOB    NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
  imap_uid        INTEGER NOT NULL,
  rfc_message_id  TEXT,       -- RFC 822 Message-ID
  thread_id       TEXT,       -- derived from References / In-Reply-To
  subject         TEXT,
  from_addr       TEXT,
  to_addrs        TEXT    NOT NULL DEFAULT '[]',     -- JSON array of strings
  cc_addrs        TEXT    NOT NULL DEFAULT '[]',     -- JSON array of strings
  sent_at         TEXT,
  internal_date   TEXT,
  flags           TEXT    NOT NULL DEFAULT '[]',     -- JSON array, e.g. ["\\Seen","\\Flagged"]
  size_bytes      INTEGER,
  has_attachment  INTEGER NOT NULL DEFAULT 0,        -- 0/1 boolean
  snippet         TEXT,                              -- first ~200 chars, for list view
  priority        INTEGER,                           -- 1=top, 2=med, 3=low; NULL = unset
  body_fetched_at TEXT,
  UNIQUE (account_id, mailbox_id, imap_uid)
);

CREATE INDEX idx_messages_account_sent ON messages (account_id, sent_at DESC);
CREATE INDEX idx_messages_thread       ON messages (thread_id);

-- Bodies are big; lazy-fetch & cache.
CREATE TABLE message_bodies (
  message_id      BLOB PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
  text_plain      TEXT,
  html            TEXT,
  fetched_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);

-- AI outputs. prompt_hash dedupes identical inputs (free cache hit).
CREATE TABLE ai_results (
  id                BLOB    PRIMARY KEY,
  message_id        BLOB    NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  kind              TEXT    NOT NULL,   -- 'summary' | 'translate' | 'classify' | 'draft'
  model             TEXT    NOT NULL,   -- 'claude-haiku-4-5' | 'claude-sonnet-4-6' …
  prompt_hash       TEXT    NOT NULL,   -- sha256(system + user_input)
  output            TEXT    NOT NULL,   -- JSON value
  input_tokens      INTEGER,
  output_tokens     INTEGER,
  cache_read_tokens INTEGER,
  created_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
  UNIQUE (message_id, kind, prompt_hash)
);

CREATE INDEX idx_ai_results_message ON ai_results (message_id);

-- Tags (AI- or user-applied).
CREATE TABLE message_tags (
  message_id  BLOB NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  tag         TEXT NOT NULL,
  source      TEXT NOT NULL,   -- 'ai' | 'user'
  confidence  REAL,            -- 0..1 for AI, NULL for user
  created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
  PRIMARY KEY (message_id, tag)
);

-- Audit log of every outbound SMTP send. Mandatory at MVP.
CREATE TABLE send_log (
  id              BLOB    PRIMARY KEY,
  account_id      BLOB    NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  in_reply_to     BLOB    REFERENCES messages(id),
  to_addrs        TEXT    NOT NULL DEFAULT '[]',     -- JSON array of strings
  subject         TEXT    NOT NULL,
  ai_assisted     INTEGER NOT NULL,                  -- 0/1 boolean
  sent_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
  smtp_response   TEXT
);
```

### AI output shapes (JSON)

| `kind`      | Shape                                                                                                 |
| ----------- | ----------------------------------------------------------------------------------------------------- |
| `summary`   | `{ "tldr": "…", "bullets": ["…"], "language": "zh-CN" }`                                              |
| `translate` | `{ "target": "zh-CN", "subject": "…", "body": "…" }`                                                  |
| `classify`  | `{ "category": "personal\|work\|notification\|promotion\|spam", "priority": 1\|2\|3, "tags": ["…"] }` |
| `draft`     | `{ "subject": "…", "body": "…", "tone": "formal\|friendly" }`                                         |

## 4. Module boundaries (Rust)

### `src-tauri/src/imap/`

- `Client` wraps `async_imap::Client`. One connection per account at MVP (single-account anyway).
- `sync_mailbox(account, mailbox) -> SyncReport` — fetches new UIDs since `uid_next`, writes headers + snippet.
- `fetch_body(message_id) -> MessageBody`.
- Sends IMAP `ID` command after login (required for 163; harmless for QQ).
- TLS-only.

### `src-tauri/src/smtp/`

- `send_draft(account, draft) -> SendReceipt`.
- Always writes a `send_log` row, even on failure.
- Re-auth on 5xx, fail on 4xx.

### `src-tauri/src/ai/`

- `AnthropicClient` — `reqwest` + retry + prompt caching.
- Operations: `summarize`, `classify`, `translate`, `draft_reply`.
- **Prompt caching:** static system prompt + user persona summary cached (5 min TTL). Per-message context not cached.
- Every result persisted to `ai_results` and looked up by `(message_id, kind, prompt_hash)` before any API call.

### `src-tauri/src/db/`

- `Pool` = `sqlx::SqlitePool` over a local DB file in the OS app-data dir; created + migrated on first launch by `db::connect`.
- Repositories: `AccountRepo`, `MessageRepo`, `AiRepo`, `TagRepo`, `SendLogRepo`.
- `sqlx::migrate!()` macro runs migrations on app startup.

### `src-tauri/src/keychain/`

- `store_auth_code(account_id, code)`
- `get_auth_code(account_id) -> Secret<String>`
- `delete_auth_code(account_id)`

### `src-tauri/src/commands/`

The frontend's API surface. Every command is `#[tauri::command] async fn`, returning `Result<T, AppError>`.

```
accounts_list()                            -> Vec<Account>
account_add(form)                          -> Account
account_remove(id)                         -> ()

mailboxes_list(account_id)                 -> Vec<Mailbox>
inbox_sync(account_id)                     -> SyncReport
messages_list(mailbox_id, limit, offset)   -> Vec<MessageHeader>
message_get(id)                            -> MessageDetail
message_body(id)                           -> MessageBody       // auto-fetches if missing

ai_summarize(message_id)                   -> Summary
ai_classify(message_id)                    -> Classification
ai_translate(message_id, target)           -> Translation
ai_draft_reply(message_id, intent)         -> Draft

smtp_send(draft)                           -> SendReceipt        // explicit user action only
```

## 5. TS frontend

- `src/lib/tauri.ts` — the **only** place `invoke` is called. Typed wrappers like `export async function inboxSync(accountId: string): Promise<SyncReport>`.
- `src/lib/store/` — Zustand stores, one per domain (`useAccountStore`, `useMessageStore`, `useAiStore`).
- `src/components/` — kebab-case files; PascalCase components.
- 3-pane layout: `<AccountList />` | `<MessageList />` | `<MessageDetail />` with embedded `<AiPanel />`.
- No router needed at MVP — pane selection is component state.
- Styling: **Tailwind** (decide at Sprint 1.5 kickoff if we want shadcn/ui on top).

## 6. Sprints

> Estimates are calendar days assuming ~2–3 focused hours/day. Treat as rough scaffolding, not commitments.

### ✅ Sprint 0 — Bootstrap (done, commit `d11d96f`)

Quality baseline (Tier 1+3) + Tauri scaffold + embedded SQLite (auto-created on first launch).

### ✅ Sprint 1 — Read-only inbox (done, commits 2dfa20a → e024f92)

- [x] `0001_initial.sql` migration + sqlx-cli setup
- [x] Cargo deps: `sqlx`, `tokio`, `async-imap`, `secrecy`, `keyring`, `anyhow`, `thiserror`, `tracing`, `uuid`, `time` (lettre lands in Sprint 5 with the send path)
- [x] `db::Pool` + run-migrations-on-boot
- [x] Add-account flow: UI form → `account_add` command → keychain + DB
- [x] `inbox_sync` for QQ — TLS, ID command, UID-based incremental
- [x] 3-pane shell + list view of last 50 INBOX rows
- [x] Detail view with body fetched on click

**Done when:** add my QQ account → see latest 50 mails → click one → read body.

### ✅ Sprint 2 — AI summary (done, commits fd440a0 + a43f41a)

### ✅ Sprint 2.5 — Multi-provider AI (NEW, done, commits 8145523 + 38e79de)

Scope drift from user request: AI must support both Anthropic native + OpenAI-compatible
interfaces (the latter covers DeepSeek / 智谱 GLM / Moonshot Kimi / 通义 / 字节豆包 etc.
via base_url override). Pulled the hardcoded Sonnet 4.6 + ANTHROPIC_API_KEY env path out and
replaced with DB-stored model rows + per-role dispatch.

- [x] `ai_models` + `ai_role_defaults` tables (migration 0002)
- [x] `AiClient` enum: `Anthropic` (native `/v1/messages`, cache_control aware) +
      `OpenAI` (`/v1/chat/completions`, covers domestic vendors via base_url)
- [x] `keychain` second service `com.zhuxbo.aiemail.ai` keyed by `ai_models.id`
- [x] 6 Tauri commands: `models_list / model_add / model_remove /
role_defaults_list / role_default_set / role_default_clear`
- [x] `<AiSettingsDialog>` modal with 7 vendor presets (Anthropic / OpenAI / DeepSeek /
      智谱 GLM / Moonshot Kimi / 通义 / 自定义) + role assignment matrix
- [x] `ai_summarize` (and later `ai_classify / ai_translate / ai_draft_reply`) all resolve
      their model via `ai_role_defaults` before each call

**Done when:** user can add a DeepSeek model in the UI, assign it to summary role, and the
next 总结 click uses DeepSeek instead of Sonnet.

- [x] `ai::AnthropicClient` with prompt caching (system prompt cached 5 min ephemeral)
- [x] `ai_summarize` command (Sonnet 4.6)
- [x] `<AiPanel />` in detail view: button → result → token-count footer
- [x] `ai_results` cache lookup before API call (sha256 over system+user prompt)

**Done when:** click "总结" → tldr + bullets in ≤3s; second call instant (cache hit).

### ✅ Sprint 3 — Classification + priority (done, commit c0e72a9)

- [x] `ai_classify` command (Haiku-tier; batch up to 20 messages per call)
- [x] Background bulk-classify on `inbox_sync` (tokio::spawn after persist)
- [x] Tags + priority + category columns in list view
- [x] Filter chips (`personal/work/notification/promotion/spam`) + sort by priority

**Done when:** new mail tagged + prioritised within ~5s of sync; UI re-fetches after 3.5s
to catch the background result.

### ✅ Sprint 4 — Translation (done, commit a3e4179)

- [x] `ai_translate` command (Sonnet-tier, target = zh-CN at MVP)
- [x] "翻译" toggle in detail view (in AiPanel)
- [x] Inline translated subject + body block with usage footer

**Done when:** any English / Japanese mail → readable Chinese with one click; second click
hits ai_results cache (separate entry per target language).

### ✅ Sprint 5 — Draft reply + SMTP send (done, commit 0fb8296)

- [x] `ai_draft_reply` command — context = original message + optional intent string
- [ ] Few-shot: pull 5 recent sent messages as style anchor (DEFERRED to Sprint 6 —
      need a "sent items" repo separate from `send_log` to bootstrap the anchor set)
- [x] Reply composer modal: editable textarea + AI 起草 + 发送
- [x] `smtp_send` via lettre + write to `send_log` (both success and failure paths)

**Done when:** 回复 → AI 起草 → edit → 发送 → mail arrives; send_log row recorded.

**🎯 MVP cut — Sprint 5 complete = shippable v0.1.**

### Sprint 6 — Multi-account + live sync (M · ~3 days)

- [ ] Account switcher in sidebar
- [ ] IMAP IDLE for live updates (single connection per account, exponential backoff)
- [ ] Error toast system
- [ ] Empty / loading states
- [ ] First pass through `/code-review` skill

### Sprint 7 — Android (L · ~5–7 days)

- [ ] `pnpm tauri android init`
- [ ] Touch UI: swipe-to-archive, bottom nav, FAB compose
- [ ] Foreground sync service (Android WorkManager via Tauri plugin)
- [ ] APK signing config
- [ ] FCM push — deferred to phase 3 if NDK story bites

### Sprint 8+ (post-MVP)

- Auto-reply with whitelist + templates (the original 全自动 feature, gated)
- 163 / Gmail OAuth providers
- iOS port
- Cross-device sync (optional sync layer on top of local SQLite)
- Bundle-size + perf budget enforcement

## 7. MVP definition of done

After **Sprint 5**:

- Single QQ account configured via UI; auth code in OS keychain.
- Latest ≥100 emails synced; inbox list renders <500ms from cache.
- Every new message auto-classified + prioritised within 5s of sync.
- On any message, user can: summarise, translate, draft reply.
- Reply composer sends real SMTP; every send recorded in `send_log`.
- All 6 quality gates green (`prettier` / `tsc` / `eslint` / `cargo fmt` / `cargo clippy` / `gitleaks`).
- CI green on every push.
- macOS / Windows / Linux installers built via CI; macOS minimum smoke-tested.

## 8. Performance targets

| Metric                                | Target  | Measurement             |
| ------------------------------------- | ------- | ----------------------- |
| Cold app launch                       | ≤1 s    | local stopwatch         |
| Inbox load (50 cached rows)           | ≤200 ms | client render           |
| Inbox sync (≤5 new messages)          | ≤2 s    | command duration        |
| Summary generation (Sonnet, no cache) | ≤3 s    | command duration        |
| Summary cache hit                     | ≤50 ms  | DB lookup               |
| Classification (20 mails, Haiku)      | ≤5 s    | end-to-end batched call |
| Idle memory (resident)                | ≤200 MB | Activity Monitor        |

## 9. Security posture

- **Credentials:** OS keychain via `keyring` crate. NEVER in DB or config.
- **Anthropic API key:** dev = `.env` (gitignored); prod = OS keychain.
- **TLS-only:** IMAP `:993`, SMTP `:465`, Anthropic HTTPS. No fallback.
- **Local SQLite file:** lives in the OS app-data dir; never network-exposed. No DB password to manage.
- **Audit log:** every SMTP send (`send_log`); every AI call (`ai_results`); every account add/remove (server log via `tracing`).
- **gitleaks:** pre-commit + CI scan; custom rules for QQ 授权码 + `sk-ant-*`.
- **No auto-send at MVP.** Hard rule. Auto-reply waits until Sprint 8 with whitelist + audit.

## 10. Testing strategy

- **Rust unit:** `#[cfg(test)] mod tests` next to source; ≥70 % coverage at Sprint 3 close (`cargo-llvm-cov`).
- **Rust integration:** `src-tauri/tests/` using a temp-file (or in-memory) SQLite DB + a custom in-memory IMAP fixture (NEVER real QQ).
- **TS unit:** `vitest` + React Testing Library, ≥60 % coverage at Sprint 3 close.
- **AI prompt regression:** fixture inputs → semantic-match outputs via embedding similarity. NEVER literal string compare. Runs on every PR touching `ai/`.
- **E2E (post-MVP):** Playwright against the Tauri dev server. Deferred.

## 11. Open questions

| #   | Question                                                                                         | Decide by                                       |
| --- | ------------------------------------------------------------------------------------------------ | ----------------------------------------------- |
| 1   | ~~UI library — plain Tailwind, shadcn/ui, or none?~~                                             | ✅ Tailwind 4, no shadcn (Sprint 1.5)           |
| 2   | ~~State management — Zustand vs Jotai vs Context+useReducer?~~                                   | ✅ Zustand 5 (Sprint 1.5)                       |
| 3   | ~~Migrations tool — `sqlx-cli` (recommended), `refinery`, or hand-rolled?~~                      | ✅ sqlx-cli + `sqlx::migrate!()` (Sprint 1.1)   |
| 4   | ~~AI system-prompt language — English (cheaper tokens) vs Chinese (better Chinese tone match)?~~ | ✅ 中文 (Sprint 2 — 用户读中文邮件，少一道翻译) |
| 5   | Android sync model — foreground service every N min, or only on app open?                        | Sprint 7                                        |

## 12. Risks (with mitigations)

- **Tauri 2 mobile maturity (Android).** Stable on iOS, less battle-tested on Android. → Keep Android as a separate sprint; ship desktop MVP first.
- **Anthropic cost.** Sonnet at scale isn't cheap. → Aggressive prompt caching, Haiku for batch classification, `ai_results` dedup.
- **QQ rate-limits.** Too many concurrent connections get throttled. → Single IDLE connection per account, exponential backoff, no reconnect loops.
- **strict-type-checked ESLint friction.** Already saw the `non-nullable-type-assertion-style` vs `no-non-null-assertion` conflict. → Use explicit null guards (`if (!x) throw …`).
- **Schema drift.** → `sqlx-cli` + CI migration-check + never ALTER without a migration file.

---

## Appendix A — Repo layout (target)

```
ai-email/
├── docs/
│   └── SPEC.md                            (this file)
├── src/
│   ├── components/                        kebab-case files, PascalCase exports
│   ├── lib/
│   │   ├── tauri.ts                       only place invoke() is called
│   │   └── store/                         zustand stores, one per domain
│   └── main.tsx
├── src-tauri/
│   ├── migrations/                        sqlx-cli managed (SQLite)
│   │   └── 0001_initial.sql
│   ├── src/
│   │   ├── imap/
│   │   ├── smtp/
│   │   ├── ai/
│   │   ├── db/
│   │   ├── keychain/
│   │   ├── commands/
│   │   ├── error.rs
│   │   └── lib.rs
│   └── tests/                             integration tests
├── .github/                               CI workflows, dependabot
├── .claude/                               PostToolUse + Stop hooks
├── CLAUDE.md                              constitution
└── ONBOARDING.md                          quick-start
```

## Appendix B — Glossary

- **Authorization code (授权码)** — QQ Mail's app-specific password; what we use to log in via IMAP/SMTP.
- **Sprint** — a focused chunk of work, here roughly 2–5 days each.
- **MVP cut** — the point where the product is shippable, here = Sprint 5 complete.
- **Phase 2** — Sprint 6+7 (multi-account + Android).
- **Phase 3** — Sprint 8+ (auto-reply, more providers, iOS, sync).
