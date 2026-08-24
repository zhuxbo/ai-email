#!/usr/bin/env bash
# Build the macOS app bundle and create a CI-safe DMG without Finder AppleScript.
set -euo pipefail

cd "$(dirname "$0")/.."

app_name="ai-email"
version="$(node -p "JSON.parse(require('fs').readFileSync('src-tauri/tauri.conf.json', 'utf8')).version")"

host_arch="$(uname -m)"
case "$host_arch" in
  arm64 | aarch64)
    dmg_arch="aarch64"
    ;;
  *)
    echo "macOS release build only supports arm64 hosts; got $host_arch" >&2
    exit 1
    ;;
esac

pnpm tauri build --bundles app "$@"

app_path="src-tauri/target/release/bundle/macos/${app_name}.app"
out_dir="src-tauri/target/release/bundle/dmg"
dmg_path="${out_dir}/${app_name}_${version}_${dmg_arch}.dmg"

if [ ! -d "$app_path" ]; then
  echo "missing macOS app bundle: $app_path" >&2
  exit 1
fi

# 本地签名（可选）：存在本地自签名身份（scripts/setup-macos-signing.sh 创建）时给 app 签名，
# 稳定签名身份让钥匙串 ACL 记住「同一个应用」，消除每次重建后的重复索权弹窗。
# CI 无该身份则跳过，行为不变（未签名包）。可用 AI_EMAIL_SIGN_IDENTITY 覆盖身份名。
sign_identity="${AI_EMAIL_SIGN_IDENTITY:-ai-email local signing}"
if security find-identity -v -p codesigning | grep -q "$sign_identity"; then
  codesign --force --sign "$sign_identity" "$app_path"
  echo "Signed with local identity: $sign_identity"
else
  echo "Local signing identity not found; skipping codesign (run scripts/setup-macos-signing.sh to enable)"
fi

mkdir -p "$out_dir"
staging="$(mktemp -d "${TMPDIR:-/tmp}/${app_name}-dmg.XXXXXX")"
trap 'rm -rf "$staging"' EXIT

cp -R "$app_path" "$staging/${app_name}.app"
ln -s /Applications "$staging/Applications"

rm -f "$dmg_path"
hdiutil create -volname "$app_name" -srcfolder "$staging" -ov -format UDZO "$dmg_path"

echo "Built macOS app: $app_path"
echo "Built macOS DMG: $dmg_path"
