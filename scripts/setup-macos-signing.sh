#!/usr/bin/env bash
# 一次性设置本地 macOS 代码签名身份，消除「每次重新构建后钥匙串重复索权弹窗」。
#
# 原理：未签名 app 的钥匙串 ACL 按**二进制内容哈希**识别身份，每次重建都变 → 每次都弹。
# 用固定的自签名证书签名后，ACL 记住的是证书身份 → 只需最后一次点「始终允许」，
# 之后所有同证书签名的新构建都免弹。仅本机生效，不影响 CI（release.yml 仍出未签名包）。
#
# 幂等：已存在同名身份则直接退出。删除方式：
#   钥匙串访问.app → 证书助理创建的「ai-email local signing」删除，或
#   security delete-identity -c "ai-email local signing"
set -euo pipefail

IDENTITY="ai-email local signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning | grep -q "$IDENTITY"; then
  echo "签名身份已存在: $IDENTITY（无需重复创建）"
  exit 0
fi

work="$(mktemp -d "${TMPDIR:-/tmp}/ai-email-sign.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# 自签名证书：10 年有效期，EKU=codeSigning（codesign 要求；系统信任与否不影响签名与钥匙串 ACL 匹配）。
cat > "$work/openssl.cnf" <<'EOF'
[req]
distinguished_name = dn
prompt = no
[dn]
CN = ai-email local signing
O = ai-email local
[v3_codesign]
basicConstraints = critical, CA:FALSE
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
EOF

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$work/key.pem" -out "$work/cert.pem" \
  -days 3650 -config "$work/openssl.cnf" -extensions v3_codesign >/dev/null 2>&1

# 分别导入 PEM 私钥与证书（不用 p12：LibreSSL 导出的 p12 与 macOS security 导入的
# MAC 算法不兼容，会报 MAC verification failed）。-T /usr/bin/codesign 免去每次签名时
# 的密钥使用确认弹窗。
security import "$work/key.pem" -k "$KEYCHAIN" -T /usr/bin/codesign
security import "$work/cert.pem" -k "$KEYCHAIN"

# 自签名证书需显式信任（用户域，codeSign 策略）才会被 find-identity 列为有效签名身份。
security add-trusted-cert -p codeSign -k "$KEYCHAIN" "$work/cert.pem"

if security find-identity -v -p codesigning | grep -q "$IDENTITY"; then
  echo "已创建并导入本地签名身份: $IDENTITY"
  echo "重新运行 pnpm build:macos 即自动用它签名；首次访问钥匙串时点一次「始终允许」即可。"
else
  echo "导入后未找到身份，请用「钥匙串访问」手动检查" >&2
  exit 1
fi
