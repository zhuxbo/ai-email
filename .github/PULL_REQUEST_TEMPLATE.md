## Summary

<!-- 1-3 bullets describing what changed and why -->

## Test plan

- [ ] Unit tests added/updated
- [ ] Integration tests pass locally
- [ ] Tested manually on macOS / Windows / Linux (delete as appropriate)
- [ ] Tested on Android (if mobile-affecting)

## Security

- [ ] No credentials, API keys, or auth codes in this diff
- [ ] If touching IMAP / SMTP / AI calls: ran `/security-review`
- [ ] If touching prompts: regression tests still pass

## Quality checklist

- [ ] `cd src-tauri && cargo fmt && cargo clippy --no-deps -- -D warnings`
- [ ] `pnpm exec prettier --check .`
- [ ] `pnpm exec eslint --max-warnings 0 .`
- [ ] `pnpm exec tsc --noEmit`
- [ ] `gitleaks protect --staged`
