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
    dmg_arch="$host_arch"
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

mkdir -p "$out_dir"
staging="$(mktemp -d "${TMPDIR:-/tmp}/${app_name}-dmg.XXXXXX")"
trap 'rm -rf "$staging"' EXIT

cp -R "$app_path" "$staging/${app_name}.app"
ln -s /Applications "$staging/Applications"

rm -f "$dmg_path"
hdiutil create -volname "$app_name" -srcfolder "$staging" -ov -format UDZO "$dmg_path"

echo "Built macOS app: $app_path"
echo "Built macOS DMG: $dmg_path"
