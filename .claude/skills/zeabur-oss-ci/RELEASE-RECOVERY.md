# When a release run fails

Read the repository's release notes for maintainers (zeabur: `docs/internal/releasing.md`), its
`.github/workflows/release.yml`, the shared workflow it calls (zeabur-oss/.github) and the
`scripts/release*.sh` it runs: they are the recovery path. Never push or move a tag, publish a
release, dispatch the workflow or change a package's settings yourself; those are the
maintainers'. Propose the command or the setting instead.

## Keep what was published

Once `publish` succeeded, the images, charts and the draft's assets are signed by the shared
workflow (`https://github.com/zeabur-oss/.github/.github/workflows/release.yml@…`) for
`zeabur-oss/<repo>` at `refs/tags/v<version>`. Do not rebuild, re-sign, overwrite or delete
them to make a check pass (zeabur's `scripts/release.sh publish` refuses an existing release,
image tag or chart version on purpose). Fix the check on `main`, then a maintainer runs
`gh workflow run release.yml --ref main -f version=v<version> -F verify_only=true`, which takes
the scripts from `main` and the Nix shell from the tag. Re-running the failed run reuses its old
workflow file and permissions and picks up no fix.

## Tell the access failures apart

- **`release not found` on `gh release download`** for a draft that exists: the token cannot see
  drafts. GitHub shows a draft only to a token that can push, so `verify` has
  `contents: write`. Check the job's permissions; never publish the draft to get past it.
- **`unauthorized` or `denied` pulling from ghcr.io** in `verify`, or `ErrImagePull` in a smoke
  cluster: the package is private. A public repository does not make its packages public, and
  the first run creates its packages private.
- **`gh attestation verify` finds nothing**: the attestation jobs did not run or failed; read
  that job's log. An image tag or a chart version that exists without attestations is a failed
  publish, not a verification bug.

## Verify a release by hand

```sh
cosign verify <image>@<digest> \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  --certificate-identity-regexp '^https://github\.com/zeabur-oss/\.github/\.github/workflows/release\.yml@' \
  --certificate-github-workflow-repository zeabur-oss/<repo> \
  --certificate-github-workflow-ref refs/tags/<version>
gh attestation verify <file> --repo zeabur-oss/<repo> \
  --signer-workflow zeabur-oss/.github/.github/workflows/release.yml
```
