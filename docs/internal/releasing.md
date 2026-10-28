# Releasing

How a maintainer publishes a release. What a release contains and how to verify it is in
[Releases](../reference/releases.mdx).

`.github/workflows/release.yml` calls the shared release workflow of
[zeabur-oss/.github](https://github.com/zeabur-oss/.github), pinned by commit, which runs every
stage as `scripts/release.sh <stage>` in this repository's Nix dev shell:

| Stage | Where | Does |
|---|---|---|
| `check` | once | The tag is the version of every crate, `Cargo.lock`, the chart (`version`, `appVersion`), `web/package.json`, the installer's `package.json` and `openapi.yaml`, and `CHANGELOG.md` has its section. |
| `build` | Linux amd64 and arm64, macOS arm64 | On a tag, the catalog and Zeabur Identity pins resolve (`release-pins.sh`). Builds the platform (`release-build.sh`); on Linux, smokes the archive, the chart and the image on a k3s cluster with Zeabur Identity at `ZEABUR_IDENTITY_PINNED` (`release-smoke.sh`). A rehearsal also builds a development installer on Linux, which it does not keep. |
| `publish` | once, tag only | Pushes the image and the chart by digest and signs them (`zeabur-oss-release push-image`, `push-chart`); writes `published/subjects.json`, which the shared workflow attests, and `published/outputs.env`, the installer's pins of both. |
| `finish` | every platform, tag only | Builds `zeabur-installer` on Linux, pinned to the catalog, Zeabur Identity and what `publish` pushed. macOS has no installer and leaves only a note, which no asset includes. |
| `assets` | once, tag only | Makes the assets, the signed `SHA256SUMS` (`zeabur-oss-release checksums`) and the notes from `CHANGELOG.md` with a link to the documentation (`zeabur-oss-release notes`); the shared workflow attests them and creates the draft. |
| `verify` | every platform | Downloads the draft and checks it as a user would (`zeabur-oss-release verify-assets`, `verify-image`, `verify-chart`), then installs the published chart on Linux (`release-smoke.sh`) or runs `zeaburctl` on macOS. |

Every signature and attestation names the shared workflow,
`https://github.com/zeabur-oss/.github/.github/workflows/release.yml@…`, not this repository's
file; the certificate's repository (`zeabur-oss/zeabur`) and ref (`refs/tags/v<version>`) say
which release it is. `verify` gets these in `RELEASE_CERT_IDENTITY_REGEXP`,
`RELEASE_SIGNER_WORKFLOW`, `RELEASE_SOURCE_REPOSITORY` and `RELEASE_SOURCE_REF`.

## Publish a release

1. On a branch, set the version in `Cargo.toml` (`[workspace.package]`, then
   `nix develop -c cargo update --workspace` for `Cargo.lock`), in
   `deploy/chart/zeabur/Chart.yaml` (`version` and `appVersion`), `web/package.json`,
   `crates/zeabur-installer/web/package.json` and `crates/zeabur-api/openapi.yaml` (`info.version`;
   the API test regenerates it), all without the `v`; `nix develop -c scripts/release.sh check
   v<version>` checks them. Rename
   `## [Unreleased]` in `CHANGELOG.md` to `## [<version>] - <date>` and add its link at the end;
   that section becomes the release notes. The catalog commit in `flake.lock` and the
   Zeabur Identity release tag in `ZEABUR_IDENTITY_PINNED` are what the release embeds and
   installs: the catalog commit must carry a published release tag and the Identity tag must
   exist, each release with its signed checksum list, or the release stops
   (`scripts/release-pins.sh`). Renovate moves the Identity tag only to newer release tags.
   Merge the pull request to `main`.
2. Rehearse: **Actions → Release → Run workflow** on `main` with the version, `v<version>`. It
   runs the three builds with their smoke tests and publishes nothing; the pins are
   not checked and no pinned installer is built.
3. Before the first release only, see [Settings the workflow needs](#settings-the-workflow-needs).
4. Check that **CI** passed on `main` for the merged commit, then tag it and push the tag:

   ```sh
   git fetch git@github.com:zeabur-oss/zeabur.git main
   git tag -s v0.1.1 -m v0.1.1 <commit on main>
   git push git@github.com:zeabur-oss/zeabur.git v0.1.1
   ```

5. Watch the **Release** run. When it succeeds, the image and the chart are in the registry (their
   tags are public while the release is still a draft) and the draft release has every asset.
   The `verify` jobs have downloaded the draft's assets on fresh runners, checked every
   signature, checksum and attestation without a registry login, installed the published chart
   on a new k3s cluster on Linux amd64 and arm64, whose node pulled the published image, and run
   `zeaburctl` on macOS arm64.
6. Check the draft: the notes, the asset list, the digests. Optionally install it on a cluster of
   your own as [Install on an existing cluster](../get-started/install.mdx) describes.
7. Publish it: on the release page, **Edit → Publish release**, or
   `gh release edit v0.1.1 --repo zeabur-oss/zeabur --draft=false`. It stays marked as a
   pre-release while Zeabur is before 1.0. Then download it once without signing in and repeat
   the checks above against the public release.

The tag must match the manifests that `check` reads, `CHANGELOG.md` must have its section, and
the commit must be on `main`; otherwise the run stops before building.

Version 0.1.0 was never published: zeabur-oss withdrew it in every repository, and releases start
at 0.1.1.

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
- **Workflow permissions** can stay read-only by default: the caller grants the shared
  workflow `contents: write`, `packages: write`, `id-token: write` and `attestations: write`,
  and each of its jobs asks for what it needs. `publish` has `packages: write` and
  `id-token: write` (keyless signing); `attest` has `attestations: write`; `release` has
  `contents: write` (the draft), `id-token: write` and `attestations: write`; `verify` has
  `contents: write`, because GitHub shows a draft release only to a token that can push, and
  `attestations: read`.

## When a release run fails

Never move a published tag and never replace a published asset or image tag:
`scripts/release.sh publish` refuses when the release, the image tag or the chart version
already exists.

- **Before `publish`** (CI, a build, a smoke test): nothing was published. Fix it on `main`; if
  the fix changes the release commit, delete the tag (`git push git@github.com:zeabur-oss/zeabur.git :refs/tags/v0.1.1`) and tag
  the new commit.
- **In `verify`**, after `publish` succeeded: keep the draft, the tag and the registry
  artifacts. Fix the cause (a package that is still private, or the verification scripts on
  `main`), then run a new verification with the scripts of `main` against the existing assets:

  ```sh
  gh workflow run release.yml --repo zeabur-oss/zeabur --ref main \
    -f version=v0.1.1 -F verify_only=true
  ```

  It builds and publishes nothing, uses the tag's Nix shell and `main`'s `scripts/`, and checks
  the tag's signing identity. Re-running the failed run instead would reuse its old workflow file and permissions.
- **In `publish`, `attest`, `finish` or `release`**: look at what it left (`gh release view`, the packages' versions on
  ghcr.io) before anything else. A draft can be deleted; image and chart versions already
  pushed have to be deleted in the package settings before the same version can be published
  again, and only if nobody can have pulled them yet. Otherwise release the next patch version.
