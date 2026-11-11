#!/usr/bin/env bash
# One-time setup: a local, self-signed code-signing identity for development
# builds. Ad-hoc signatures change with every build, so macOS treats each build
# as a new app and asks for Keychain access again. Signing with one stable
# identity keeps the app's designated requirement constant across builds.
#
# The certificate never leaves this Mac and is not trusted for distribution;
# release signing still needs an Apple Developer ID.
set -euo pipefail
name="Twayblade Development"
security=(/usr/bin/security)
if "${security[@]}" find-identity -p codesigning | grep -q "\"$name\""; then
    printf 'Identity "%s" already exists.\n' "$name"
    exit 0
fi
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cat >"$tmp/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $name
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF
# /usr/bin/openssl is LibreSSL; its PKCS#12 output is what `security` imports.
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
    -keyout "$tmp/key.pem" -out "$tmp/cert.pem" -config "$tmp/cert.cnf" 2>/dev/null
password="$(/usr/bin/uuidgen)"
/usr/bin/openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
    -name "$name" -out "$tmp/identity.p12" -passout "pass:$password"
# -T lets codesign use the key without a prompt on every build.
"${security[@]}" import "$tmp/identity.p12" -k "$HOME/Library/Keychains/login.keychain-db" \
    -P "$password" -T /usr/bin/codesign
printf 'Created "%s". Rebuild with macos/build.sh; approve Keychain access once with "Always Allow".\n' "$name"
