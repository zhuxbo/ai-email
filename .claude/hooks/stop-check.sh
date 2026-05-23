#!/usr/bin/env bash
# Stop hook — runs cargo check / tsc --noEmit IF this session edited Rust / TS files.
# Exit 2 = prevent end-of-turn (Claude must keep working).
# Skips silently when toolchain isn't installed yet (pre-bootstrap).
set -euo pipefail

cd "$(dirname "$0")/../.."

# No git repo yet → nothing to check
git rev-parse --git-dir >/dev/null 2>&1 || exit 0

changed=$(git diff --name-only HEAD 2>/dev/null || true)
untracked=$(git ls-files --others --exclude-standard 2>/dev/null || true)
all="${changed}
${untracked}"

[[ -z "$(printf '%s' "$all" | tr -d '[:space:]')" ]] && exit 0

has_rust=false
has_ts=false

while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  case "$f" in
    *.rs) has_rust=true ;;
    *.ts|*.tsx) has_ts=true ;;
  esac
done <<< "$all"

failed=0

if $has_rust && [[ -f src-tauri/Cargo.toml ]] && command -v cargo >/dev/null 2>&1; then
  if ! (cd src-tauri && cargo check --quiet) 2>&1; then
    echo "[stop-check] cargo check failed — fix before stopping" >&2
    failed=1
  fi
fi

if $has_ts && [[ -f tsconfig.json ]] && [[ -x node_modules/.bin/tsc ]]; then
  if ! node_modules/.bin/tsc --noEmit 2>&1; then
    echo "[stop-check] tsc --noEmit failed — fix before stopping" >&2
    failed=1
  fi
fi

exit $failed
