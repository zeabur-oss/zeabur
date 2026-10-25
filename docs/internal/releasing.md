# Releasing

How a maintainer publishes a release. What a release contains and how to verify it is in
[Releases](../reference/releases.md).

## Publish a release

1. On a branch, set the version in `Cargo.toml` (`[workspace.package]`, then
   `nix develop -c cargo update --workspace` for `Cargo.lock`) and in
   `deploy/chart/zeabur/Chart.yaml` (`version` and `appVersion`, without the `v`). Rename
   `## [Unreleased]` in `CHANGELOG.md` to `## [<version>] - <date>` and add its link at the end;
   that section becomes the release notes. The catalog commit in `flake.lock` and the
   Zeabur Identity commit in `ZEABUR_IDENTITY_PINNED` are what the release embeds and installs:
   each must carry a published release tag, or the release stops (`scripts/release-pins.sh`).
   Merge the pull request to `main`.
2. Rehearse: **Actions → Release → Run workflow** on `main` with the version, `v<version>`. It
   runs all of CI and the three builds with their smoke tests and publishes nothing.
3. Before the first release only, see [Settings the workflow needs](#settings-the-workflow-needs).
4. Tag the merged commit and push the tag:

   ```sh
   git fetch origin
   git tag -s v0.0.2 -m v0.0.2 <commit on main>
   git push origin v0.0.2
   ```

5. Watch the **Release** run. When it succeeds, the image and the chart are in the registry (their
   tags are public while the release is still a draft) and the draft release has every asset.
   The `verify` jobs have downloaded the draft's assets on fresh runners, checked every
   signature, checksum and attestation without a registry login, installed the published chart
   on a new k3s cluster on Linux amd64 and arm64, whose node pulled the published image, and run
   `zeaburctl` on macOS arm64.
6. Check the draft: the notes, the asset list, the digests. Optionally install it on a cluster of
   your own as [Install on an existing cluster](../get-started/install.md) describes.
7. Publish it: on the release page, **Edit → Publish release**, or
   `gh release edit v0.0.2 --repo zeabur-oss/zeabur --draft=false`. It stays marked as a
   pre-release while Zeabur is before 1.0. Then download it once without signing in and repeat
   the checks above against the public release.

The tag must match `Cargo.toml` and `Chart.yaml`, `CHANGELOG.md` must have its section, and the
commit must be on `main`; otherwise the run stops before building.

## Settings the workflow needs

These are repository and organisation settings, not files; a maintainer sets them once.

- **Package visibility.** A public repository does not make its packages public. The first
  release run creates the packages `zeabur` and `charts/zeabur` on ghcr.io, private by default,
  so its `verify` jobs fail: they pull without a login on purpose. Make both packages public
  (package settings → **Change visibility**) and confirm that the repository has write access
  under **Manage Actions access**, then run verify-only as below.
- **Tags.** A ruleset that lets only maintainers create, move or delete `v*` tags.
- **Immutable releases** (repository settings → **Releases**): once published, a release's tag
  and assets cannot be changed. A draft stays editable until then.
- **Workflow permissions** can stay read-only by default: each job asks for what it needs.
  `publish` has `packages: write`, `id-token: write` (keyless signing) and
  `attestations: write`; `release` has `contents: write` (the draft), `id-token: write` and
  `attestations: write`; `verify` has `contents: write`, because GitHub shows a
  draft release only to a token that can push, and `attestations: read`.

## When a release run fails

Never move a published tag and never replace a published asset or image tag:
`scripts/release-publish.sh` refuses when the release, the image tag or the chart version
already exists.

- **Before `publish`** (CI, a build, a smoke test): nothing was published. Fix it on `main`; if
  the fix changes the release commit, delete the tag (`git push origin :refs/tags/v0.0.2`) and tag
  the new commit.
- **In `verify`**, after `publish` succeeded: keep the draft, the tag and the registry
  artifacts. Fix the cause (a package that is still private, or the verification scripts on
  `main`), then run a new verification with the scripts of `main` against the existing assets:

  ```sh
  gh workflow run release.yml --repo zeabur-oss/zeabur --ref main \
    -f version=v0.0.2 -F verify_only=true
  ```

  It builds and publishes nothing, uses the tag's Nix shell, and keeps the tag's signing
  identity. Re-running the failed run instead would reuse its old workflow file and permissions.
- **In `publish`, `installer` or `release`**: look at what it left (`gh release view`, the packages' versions on
  ghcr.io) before anything else. A draft can be deleted; image and chart versions already
  pushed have to be deleted in the package settings before the same version can be published
  again, and only if nobody can have pulled them yet. Otherwise release the next patch version.
