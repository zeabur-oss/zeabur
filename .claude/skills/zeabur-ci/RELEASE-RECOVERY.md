# When a release run fails

Read `docs/internal/releasing.md` ("When a release run fails"), `.github/workflows/release.yml` and the
`scripts/release-*.sh` it calls first: they are the recovery path. Never push or move a tag,
publish a release, dispatch the workflow or change a package's settings yourself; those are the
maintainers'. Propose the command or the setting instead.

## Keep what was published

Once `publish` succeeded, the image, the chart and the draft's assets are signed with the tag's
identity (`…/release.yml@refs/tags/v<version>`). Do not rebuild, re-sign, overwrite or delete
them to make a check pass: `scripts/release-publish.sh` refuses an existing release, image tag or
chart version on purpose. Fix the check on `main`, then a maintainer runs
`gh workflow run release.yml --ref main -f version=v<version> -F verify_only=true`, which takes
the scripts from `main` and the Nix shell from the tag. Re-running the failed run reuses its old
workflow file and permissions and picks up no fix.

## Tell the access failures apart

- **`release not found` on `gh release download`** for a draft that exists: the token cannot see
  drafts. GitHub shows a draft only to a token that can push, so `verify` has
  `contents: write`. Check the job's permissions; never publish the draft to get past it.
- **`unauthorized` or `denied` pulling from ghcr.io** in `verify`, or `ErrImagePull` in the
  smoke cluster: the package is private. A public repository does not make its packages public,
  and the first run creates `zeabur` and `charts/zeabur` private. `release-verify.sh` uses an
  empty registry configuration on purpose, so a login cannot hide this.
- **`gh attestation verify` finds nothing**: the attestation steps in `publish` did not run or
  failed; read that job's log. An image tag or a chart version that exists without attestations
  is a failed publish, not a verification bug.

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
