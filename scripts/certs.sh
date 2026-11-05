#!/usr/bin/env bash
# Ensure a local self-signed code-signing identity "dito-dev" exists in the login
# keychain and is trusted for code signing. One-time setup per machine; answer the
# authorization prompt once.
#
#   scripts/certs.sh
#
# Why: macOS TCC (microphone / accessibility / input monitoring) keys grants off the
# code signature. Unsigned or ad-hoc dev builds get a new signature on every rebuild,
# so granted permissions keep disappearing. A stable local identity fixes that for
# cargo run/test (scripts/dev-runner.sh) and scripts/bundle.sh.
#
# Removal, if ever needed:
#   security find-certificate -c dito-dev -p > /tmp/dito-dev.pem
#   security remove-trusted-cert /tmp/dito-dev.pem          # takes the cert file, not a keychain
#   security delete-certificate -c dito-dev ~/Library/Keychains/login.keychain-db
#   (plus delete the private key in Keychain Access)
set -euo pipefail

identity="${DITO_CODESIGN_IDENTITY:-dito-dev}"
keychain="${HOME}/Library/Keychains/login.keychain-db"

has_valid() { security find-identity -v -p codesigning "$keychain" | grep -q "\"$identity\""; }
has_pair() { security find-identity "$keychain" | grep -q "\"$identity\""; }

if has_valid; then
  echo "identity \"$identity\" already present and trusted for code signing"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if ! has_pair; then
  cat > "$tmp/cert.cnf" <<EOF
[req]
prompt = no
distinguished_name = dn
x509_extensions = v3
[dn]
CN = $identity
O = Dito
[v3]
basicConstraints = critical, CA:FALSE
keyUsage = critical, digitalSignature
extendedKeyUsage = codeSigning
EOF
  openssl req -x509 -sha256 -newkey rsa:2048 -nodes -days 3650 \
    -keyout "$tmp/key.pem" -out "$tmp/cert.pem" -config "$tmp/cert.cnf"
  # Import key and cert separately (paired automatically); LibreSSL-made PKCS#12 with
  # an empty password fails MAC verification in `security import`.
  security import "$tmp/key.pem" -k "$keychain" \
    -T /usr/bin/codesign -T /usr/bin/security
  security import "$tmp/cert.pem" -k "$keychain" \
    -T /usr/bin/codesign -T /usr/bin/security
else
  # Key + cert exist but the cert is not trusted for code signing yet.
  security find-certificate -c "$identity" -p "$keychain" > "$tmp/cert.pem"
fi

# Trust the cert for code signing; macOS asks for authorization once.
security add-trusted-cert -p codeSign -k "$keychain" "$tmp/cert.pem"

has_valid || { echo "error: identity \"$identity\" still not valid for code signing" >&2; exit 1; }
echo "identity \"$identity\" ready: cargo run/test and scripts/bundle.sh now sign with it"
