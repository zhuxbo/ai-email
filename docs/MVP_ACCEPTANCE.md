# MVP Acceptance Checklist

> Sprints 1 → 5 + Sprint 2.5 are done. Run through this list to validate every claim
> before declaring MVP shippable.

## 0. Prerequisites

```bash
cp .env.example .env                                   # if not already
pnpm tauri dev                                         # launches the desktop app
```

The database is a local SQLite file, created automatically on first launch in the
OS app-data directory — no DATABASE_URL, no docker, no external service.

You'll also need:

- A QQ Mail account with IMAP/SMTP enabled (邮箱 → 设置 → 账户) and the
  16-character 授权码 ready.
- At least one AI provider key (Anthropic, OpenAI, DeepSeek, 智谱, Moonshot, 通义,
  or any OpenAI-compatible endpoint). You can mix — for example Sonnet for
  summary/translate/draft and DeepSeek for classify.

## 1. Add your QQ account

1. Click **+ 添加** at the top of the Accounts pane.
2. Provider: **QQ 邮箱**. Email + 授权码. Submit.
3. Expected: account row appears, INBOX auto-syncs in the background (latest 50
   messages).

✓ when: at least one mail row visible in the middle pane within ~10 seconds.

## 2. Configure AI models

1. Click **⚙ AI 模型配置** at the bottom of the Accounts pane.
2. **添加模型** section: pick a preset (e.g. Anthropic Claude or DeepSeek), fill
   API key, click **添加**. Repeat for as many vendors as you want.
3. **角色指派**: for each of `摘要 / 分类 / 翻译 / 起草回复` pick the model you
   want from the dropdown.

✓ when: 4 role dropdowns each show a model name; the cards above show "used for: …"
chips matching your selection.

## 3. Read a message + summarize

1. Click a row in the message list.
2. Right pane shows headers + body (text or sandboxed HTML iframe).
3. Click **总结** in the AI panel.
4. Expected: tldr + bullets within ~1–3s; footer shows
   `<model> · 输入 X · 输出 Y · 语言 zh-CN`.
5. Click **重新总结**.
6. Expected: instant; footer shows `缓存命中 · 0 token` (ai_results dedup hit).

## 4. Translate

1. Click **翻译** in the AI panel.
2. Expected: translation block appears below summary; subject + body rendered in
   the target language (default `zh-CN`).
3. Click **隐藏翻译** to dismiss.
4. Click **翻译** again.
5. Expected: instant; footer shows `缓存命中` (separate cache entry per target).

## 5. Classify + filter

1. Pick **同步收件箱**. After ~3.5s the list re-fetches.
2. Expected: each row shows a category pill (`私人 / 工作 / 通知 / 推广 / 垃圾`),
   optional **高 / 低** priority badge, and up to 3 AI tags.
3. Click category chips at the top of the list to filter.
4. Click **↑ 优先** to sort by priority (1 → 3).

## 6. Reply + send

1. Open any message, wait for body to load.
2. Click **回复** in the right pane header.
3. Composer opens with To pre-filled (original sender) and Subject `Re: <…>`.
4. Optionally type an intent ("婉拒"、"约下周一", …) and click **AI 起草**.
5. Expected: body fills with the draft; toggle shows "AI 起草，发送时会标记
   ai_assisted=true".
6. Edit the body / subject / To as needed.
7. Click **发送**. Confirm the popup.
8. Expected: success toast with `send_log <8-char-id>` appears, modal closes.
9. Verify in the SQLite DB:
   ```bash
   sqlite3 "$HOME/Library/Application Support/com.zhuxbo.aiemail/ai-email.db" \
     "SELECT subject, ai_assisted, smtp_response FROM send_log ORDER BY sent_at DESC LIMIT 5;"
   ```
   You should see your row, with `smtp_response` containing the SMTP code.
10. Open your own inbox in another client — the reply should be there.

## 7. Quality gates (run anytime)

All eight gates pass in the project's lefthook config; manual check:

```bash
(cd src-tauri && cargo fmt --check && cargo clippy --no-deps --all-targets -- -D warnings)
pnpm exec prettier --check .
pnpm exec eslint --max-warnings 0 --no-warn-ignored .
pnpm exec tsc --noEmit
gitleaks protect --no-banner --redact -v
(cd src-tauri && cargo test --all-features)            # 19 tests
pnpm exec vitest run --passWithNoTests
pnpm build                                              # vite production bundle
```

## 8. Audit invariants

- **Auth code never leaves the keychain.** Verify:
  `security find-generic-password -s com.zhuxbo.aiemail` shows entries by UUID.
- **API keys never leave the keychain.** Verify:
  `security find-generic-password -s com.zhuxbo.aiemail.ai`.
- **Every send is logged.** Even a failed send (wrong recipient, network drop)
  writes a `send_log` row with `smtp_response` starting with `ERROR:`.
- **AI never auto-fires SMTP.** The composer requires an explicit click + confirm
  per SPEC § 9 hard rule.

## 9. What's not in MVP

These ship in Sprint 6+ — DO NOT treat as bugs:

- Multi-account UI (only one account configured at a time renders cleanly)
- IMAP IDLE (live push); current sync is pull-on-click + post-add auto-pull
- Few-shot draft style anchor (5 recent sent messages) — draft uses original
  message context only at MVP
- Android port
- 163 / Gmail OAuth providers
- Sent-items folder mirror
- Attachment download
