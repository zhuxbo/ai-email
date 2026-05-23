#!/usr/bin/env bash
# PostToolUse hook — fast per-file format check after Edit/Write/MultiEdit.
# Exit 2 = block (Claude sees stderr); exit 0 = continue.
# Tools that aren't installed yet are skipped silently so this works pre-bootstrap.
set -euo pipefail

cd "$(dirname "$0")/../.."

input=$(cat)
file_path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty' 2>/dev/null || true)
[[ -z "$file_path" ]] && exit 0

# Resolve to absolute path if relative
case "$file_path" in
  /*) abs_path="$file_path" ;;
  *)  abs_path="$PWD/$file_path" ;;
esac

[[ -f "$abs_path" ]] || exit 0

# Only enforce checks on files inside this project — memory/notes elsewhere are out of scope.
case "$abs_path" in
  "$PWD"/*) ;;
  *) exit 0 ;;
esac

case "$file_path" in
  *.rs)
    if command -v rustfmt >/dev/null 2>&1; then
      if ! rustfmt --edition 2021 --check "$abs_path" >/dev/null 2>&1; then
        echo "[post-edit] rustfmt: $file_path needs formatting. Run: rustfmt --edition 2021 $file_path" >&2
        exit 2
      fi
    fi
    ;;
  *.ts|*.tsx|*.js|*.jsx|*.json|*.md|*.yml|*.yaml|*.css|*.html)
    if [[ -x node_modules/.bin/prettier ]]; then
      if ! node_modules/.bin/prettier --check "$abs_path" >/dev/null 2>&1; then
        echo "[post-edit] prettier: $file_path needs formatting. Run: pnpm exec prettier --write $file_path" >&2
        exit 2
      fi
    fi
    ;;
esac

exit 0
