#!/usr/bin/env bash
# Regenerates the certificates the TLS tests use: two unrelated CAs (a, b) and a leaf for
# localhost and 127.0.0.1 that CA a signed. Run inside `nix develop`; commit the result.
set -euo pipefail
cd "$(dirname "$0")"
days=36500
# macOS refuses a leaf valid for more than 825 days, so the leaf must be regenerated before it expires.
leaf_days=825

for ca in a b; do
  openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out "ca-$ca.key"
  openssl req -x509 -new -key "ca-$ca.key" -sha256 -days "$days" \
    -subj "/CN=Zeabur test CA ${ca^^}" \
    -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" \
    -out "ca-$ca.pem"
done

openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out leaf.key
openssl req -new -key leaf.key -subj "/CN=localhost" -out leaf.csr
openssl x509 -req -in leaf.csr -CA ca-a.pem -CAkey ca-a.key -CAcreateserial -sha256 -days "$leaf_days" \
  -extfile <(printf '%s\n' \
    "basicConstraints=critical,CA:FALSE" \
    "keyUsage=critical,digitalSignature" \
    "extendedKeyUsage=serverAuth" \
    "subjectAltName=DNS:localhost,IP:127.0.0.1") \
  -out leaf.pem

rm -f leaf.csr ca-a.srl ca-a.key ca-b.key
