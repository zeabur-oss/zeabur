# When a Zeabur release run fails

Start with `docs/internal/releasing.md` ("When a release run fails"). These are Zeabur's own
cases.

## A pin without a release tag

`prepare` says `zeabur-oss/catalog <commit> has no release tag that a pre-release can
pin: tag it, or move the pin`. After a tag, that version is spent. Release the catalog first, move
`flake.lock` to that release in a pull request, then release the next version. A normal release pins only normal releases, so a beta catalog
stops it too.

## A private package in `verify`

The first run creates `zeabur` and `charts/zeabur` private on ghcr.io. `scripts/release-verify.sh`
uses an empty registry configuration on purpose, so a login cannot hide a private package. The
`publish` job refuses an existing release, image tag or chart version on purpose.

## A local image that does not reach the node

`k3d image import` can print "Successfully imported" before `ctr` has read the tarball, and
`release-smoke.sh` never uses it. The dev cluster has a k3d registry: `xtask dev-cluster up` pushes
each local image to `localhost:<port>` and the node pulls it as `zeabur-dev-registry:5000/<name>`.
When a smoke test fails here, read `docker push`'s output and the pod's events (`ErrImagePull`,
`ImagePullBackOff`), not only the exit code. This is a transport problem: fix the script, never
rebuild the release for it.

## macOS builds

A macOS binary that links a library from `/nix/store` would fail on every Mac without Nix:
`release-build-zeaburctl.sh` refuses any library outside `/usr/lib` and `/System/Library`, and points
`libiconv` (Rust's standard library links it) at the system's copy. Read `otool -L` of the binary
in the failed build before changing that list.
