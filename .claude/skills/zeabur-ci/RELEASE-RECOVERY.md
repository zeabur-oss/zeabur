# When a Zeabur release run fails

Start with `zeabur-oss-ci`'s RELEASE-RECOVERY.md (what was published stays; access failures)
and `docs/internal/releasing.md` ("When a release run fails"). These are Zeabur's own cases.

## A private package in `verify`

The first run creates `zeabur` and `charts/zeabur` private on ghcr.io. `scripts/release.ts
verify` uses an empty registry configuration on purpose, so a login cannot hide a private package.
`scripts/release.ts publish` refuses an existing release, image tag or chart version on purpose.

## An image import that reports success

With Docker's containerd store an image can be a multi-platform index whose other platforms are
absent; `k3d image import` of such an image prints `ctr` "content digest … not found" and exits 0,
and the pod then fails with `ErrImageNeverPull`. `release-smoke.sh` exports only the native
platform (`docker image save --platform`) and checks the node's `crictl images` after the
import. When a smoke test fails here, read the import output and the pod's events, not only the
exit code. This is a transport problem: fix the script, never rebuild the release for it.

## macOS builds

A macOS binary that links a library from `/nix/store` would fail on every Mac without Nix:
`release-build.sh` refuses any library outside `/usr/lib` and `/System/Library`, and points
`libiconv` (Rust's standard library links it) at the system's copy. Read `otool -L` of the binary
in the failed build before changing that list.
