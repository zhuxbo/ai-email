#!/usr/bin/env bash
# 构建 Android arm64-v8a release APK。
# 优先使用已设的 ANDROID_HOME / NDK_HOME / JAVA_HOME（CI 会设，故不受影响）；
# 本地缺失时回退到 Homebrew 默认位置，并自动选用版本号最高的 NDK。
# 默认 `--apk --target aarch64`，可透传其它参数覆盖（如 `--aab`）。
set -euo pipefail

export ANDROID_HOME="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"

# NDK：尊重 NDK_HOME / ANDROID_NDK_ROOT，否则取 SDK 下版本号最高的 ndk/*
ndk="${NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [ -z "$ndk" ]; then
  ndk="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1 || true)"
fi
if [ -z "$ndk" ] || [ ! -d "$ndk" ]; then
  echo "✗ 找不到 Android NDK。请设 NDK_HOME，或在 $ANDROID_HOME/ndk/ 下安装。" >&2
  exit 1
fi
export NDK_HOME="$ndk"
export ANDROID_NDK_ROOT="$ndk"

# JDK 17：尊重已设 JAVA_HOME，否则用 Homebrew openjdk@17
if [ -z "${JAVA_HOME:-}" ]; then
  for c in /opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home \
    /opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home; do
    [ -x "$c/bin/java" ] && {
      export JAVA_HOME="$c"
      break
    }
  done
fi
if [ -z "${JAVA_HOME:-}" ] || [ ! -x "$JAVA_HOME/bin/java" ]; then
  echo "✗ 找不到 JDK 17。请设 JAVA_HOME。" >&2
  exit 1
fi

echo "→ ANDROID_HOME=$ANDROID_HOME"
echo "→ NDK_HOME=$NDK_HOME"
echo "→ JAVA_HOME=$JAVA_HOME"

[ "$#" -eq 0 ] && set -- --apk --target aarch64
exec pnpm tauri android build "$@"
