#!/usr/bin/env bash
# Check the Go adapter against the pinned, unmodified upstream Keel module.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/updater"
export GOMAXPROCS=${GOMAXPROCS:-4}
[[ -z "$(gofmt -l .)" ]] || { echo 'Run gofmt on updater/' >&2; exit 1; }
go mod verify
go vet -mod=readonly ./...
go test -mod=readonly -race ./...
mkdir -p "$root/target"
CGO_ENABLED=0 go build -mod=readonly -trimpath -buildvcs=false -o "$root/target/zeabur-database-updater" .
