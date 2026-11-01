# Releasing

How a maintainer publishes a release. What a release contains and how to verify it is in
[Releases](../reference/releases.mdx).

`.github/workflows/release.yml` calls the shared release workflow of
[zeabur-oss/.github](https://github.com/zeabur-oss/.github), pinned by commit, which runs every
stage as `bun scripts/release.ts <stage>` in this repository's Nix dev shell, after
`bun install`. `@zeabur-oss/release` (`package.json`, pinned in `bun.lock`) pushes, signs and
verifies; the shared workflow checks the `CHANGELOG.md` section, signs `SHA256SUMS`, writes the
notes and verifies the assets.

| Stage | Where | Does |
|---|---|---|
| `check` | once | The tag is SemVer without build metadata, and the version of every crate, `Cargo.lock`, the chart (`version`, `appVersion`), `web/package.json`, the installer's `package.json` and `openapi.yaml`. Every `ghcr.io/zeabur-oss` image the chart runs is one the release publishes. |
| `build` | Linux amd64 and arm64, macOS arm64 | On a tag, the catalog and Zeabur Identity pins resolve. Builds the platform (`release-build.sh`; on Linux also Zeabur's and Zeabur Proxy's images), then on Linux smokes the archive, the chart and the image on a k3s cluster with Zeabur Identity at `ZEABUR_IDENTITY_PINNED` (`release-smoke.sh`). A rehearsal also builds a development installer on Linux, which it does not keep. |
| `sign` | macOS arm64, after `build` | Signs `zeaburctl` with the Developer ID and notarizes it, in the `apple-signing` environment, on `main` and tags only. Without the certificate it leaves the build as it is and warns. |
| `publish` | once, tag only | Pushes the images `zeabur` and `proxy`, then the chart with both pinned by digest, and signs them; stops when the chart would run any other image; writes `published/subjects.json`, which the shared workflow attests, and `published/outputs.env`, the installer's pins of Zeabur's image and the chart. |
| `finish` | Linux amd64 and arm64, tag only | Builds `zeabur-installer`, pinned to the catalog, Zeabur Identity and what `publish` pushed. |
| `assets` | once, tag only | Copies the assets into the release; the shared workflow adds the signed `SHA256SUMS` and the notes from `CHANGELOG.md` with a link to the documentation, attests them and creates the draft. |
| `verify` | every platform | After the shared workflow has checked `SHA256SUMS` and each asset's attestation, checks that the draft has exactly the expected assets, each image and the chart as a user would, and that the chart runs exactly the images in `digests.txt`, then installs the published chart on Linux (`release-smoke.sh`) or runs `zeaburctl` on macOS, where a Developer ID signature must be notarized (`spctl`). |

The pins come from the releases of zeabur-oss/catalog and zeabur-oss/identity: the `SHA256SUMS`
that the shared workflow signed for each, checked with cosign. The catalog files the build embeds
must be the ones its `SHA256SUMS` lists; Zeabur Identity's image is the `index` line of its
`digests.txt`.

Every signature and attestation names the shared workflow,
`https://github.com/zeabur-oss/.github/.github/workflows/release.yml@…`, not this repository's
file; the certificate's repository (`zeabur-oss/zeabur`) and ref (`refs/tags/v<version>`) say
which release it is. `verify` gets these in `RELEASE_CERT_IDENTITY_REGEXP`,
`RELEASE_SIGNER_WORKFLOW`, `RELEASE_SOURCE_REPOSITORY` and `RELEASE_SOURCE_REF`.

## Pre-releases

A tag with a pre-release part (`v0.1.2-beta.1`, `v1.0.0-rc.1`) is a
[SemVer 2.0.0](https://semver.org/spec/v2.0.0.html) pre-release: a GitHub pre-release, never
latest. Any other tag, `v0.*` included, is a normal release. Build metadata (`+...`) is refused.
Tag it on `main` like any release. The manifests carry the full version (`0.1.2-beta.1`), and
`CHANGELOG.md` has a section of its own for it, `## 0.1.2-beta.1 (2026-11-01)`. A normal release
pins only normal releases of the catalog and Zeabur Identity; a pre-release may pin pre-releases
too. On a commit tagged both `v0.1.2-beta.1` and `v0.1.2`, the pin is `v0.1.2`; of several tags of
one kind, the newest by SemVer §11. Anything that picks the newest version compares by SemVer §11,
not `sort -V`.

## Publish a release

1. On a branch, set the version in `Cargo.toml` (`[workspace.package]`, then
   `nix develop -c cargo update --workspace` for `Cargo.lock`), in
   `deploy/chart/zeabur/Chart.yaml` (`version` and `appVersion`), `web/package.json`,
   `crates/zeabur-installer/web/package.json` and `crates/zeabur-api/openapi.yaml` (`info.version`;
   the API test regenerates it), all without the `v`; `nix develop -c bun scripts/release.ts check
   v<version>` checks them. Rename `## Unreleased` in `CHANGELOG.md` to
   `## <version> (<date>)`; that section becomes the release notes as it is, so keep each
   paragraph and list item on one line. A release that fixes a vulnerability lists it under a heading
   written exactly `### Security`. The update check in the interface and in `zeaburctl` looks for
   that heading to tell people their version has known security problems, as
   docs/reference/telemetry.mdx describes. The catalog commit in `flake.lock` and the
   Zeabur Identity release tag in `ZEABUR_IDENTITY_PINNED` are what the release embeds and
   installs: the catalog commit must carry a published release tag and the Identity tag must
   exist, each release with its signed `SHA256SUMS`, or the release stops. Renovate moves the
   Identity tag only to newer release tags.
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

5. Watch the **Release** run. When it succeeds, the images and the chart are in the registry (their
   tags are public while the release is still a draft) and the draft release has every asset.
   The `verify` jobs have downloaded the draft's assets on fresh runners, checked every
   signature, checksum and attestation without a registry login, installed the published chart
   on a new k3s cluster on Linux amd64 and arm64, whose node pulled the published image, and run
   `zeaburctl` on macOS arm64.
6. Check the draft: the notes, the asset list, the digests. Optionally install it on a cluster of
   your own as [Install on an existing cluster](../get-started/install.mdx) describes.
7. Publish it: on the release page, **Edit → Publish release**, or
   `gh release edit v0.1.1 --repo zeabur-oss/zeabur --draft=false`. Then download it once without signing in and repeat
   the checks above against the public release.

The tag must match the manifests that `check` reads, `CHANGELOG.md` must have its section, and
the commit must be on `main`; otherwise the run stops before building.

Version 0.1.0 was never published: zeabur-oss withdrew it in every repository, and releases start
at 0.1.1.

## Settings the workflow needs

These are repository and organization settings, not files; a maintainer sets them once.

- **Package visibility.** A public repository does not make its packages public. The first
  release run that pushes a package (`zeabur`, `proxy`, `charts/zeabur`) creates it on ghcr.io,
  private by default, so its `verify` jobs fail: they pull without a login on purpose. Make the package public
  (package settings → **Change visibility**) and confirm that the repository has write access
  under **Manage Actions access**, then verify again as below.
- **Tags.** A ruleset that lets only maintainers create, move or delete `v*` tags.
- **Immutable releases** (repository settings → **Releases**): once published, a release's tag
  and assets cannot be changed. A draft stays editable until then.
- **macOS signing.** An environment `apple-signing` whose deployment branches are `main` and
  `v*` tags, with these secrets (zeabur#87):
  - `APPLE_CERTIFICATE_P12`: the Developer ID Application certificate with its private key,
    exported from Keychain Access as `.p12`, then `base64 -i certificate.p12`.
  - `APPLE_CERTIFICATE_PASSWORD`: the password it was exported with.
  - `APPLE_API_KEY_P8`, `APPLE_API_KEY_ID`, `APPLE_API_ISSUER_ID`: an App Store Connect API key
    with the Developer role (**Users and Access → Integrations**): the `.p8` file's text, its key
    ID and the issuer ID.

  Without them, releases go on with `zeaburctl` signed ad hoc only.
- **Telemetry key.** A repository variable `POSTHOG_KEY`, the PostHog project key of the
  experience program (docs/reference/telemetry.mdx). Tag builds compile it in as
  `ZEABUR_TELEMETRY_KEY`; every other build, local ones included, has none and sends nothing.
- **Workflow permissions** can stay read-only by default: the caller grants the shared
  workflow `contents: write`, `packages: write`, `id-token: write` and `attestations: write`,
  and each of its jobs asks for what it needs. `publish` has `packages: write` and
  `id-token: write` (keyless signing); `attest` has `attestations: write`; `release` has
  `contents: write` (the draft), `id-token: write` and `attestations: write`; `verify` has
  `contents: write`, because GitHub shows a draft release only to a token that can push, and
  `attestations: read`.

## When a release run fails

Never move a published tag and never replace a published asset or image tag:
`scripts/release.ts publish` refuses when the release, an image tag or the chart version
already exists.

- **Before `publish`** (CI, a build, a smoke test): nothing was published. Fix it on `main`; if
  the fix changes the release commit, delete the tag (`git push git@github.com:zeabur-oss/zeabur.git :refs/tags/v0.1.1`) and tag
  the new commit.
- **In `verify`**, after `publish` succeeded: keep the draft, the tag and the registry
  artifacts. Fix the cause (a package that is still private, or the check itself on `main`),
  then verify again as below.

- **In `publish`, `attest`, `finish` or `release`**: look at what it left (`gh release view`, the packages' versions on
  ghcr.io) before anything else. A draft can be deleted; image and chart versions already
  pushed have to be deleted in the package settings before the same version can be published
  again, and only if nobody can have pulled them yet. Otherwise release the next patch version.

### Fix the check and verify again

From a checkout with the fixed check, in `nix develop`, after `bun install`, on each platform
whose `verify` failed (Linux amd64 or arm64, or macOS arm64):

```sh
export RELEASE_CERT_IDENTITY_REGEXP='^https://github\.com/zeabur-oss/\.github/\.github/workflows/release\.yml@' \
  RELEASE_SIGNER_WORKFLOW=zeabur-oss/.github/.github/workflows/release.yml \
  RELEASE_SOURCE_REPOSITORY=zeabur-oss/zeabur RELEASE_SOURCE_REF=refs/tags/v0.1.1
gh release download v0.1.1 --repo zeabur-oss/zeabur --dir downloaded
node_modules/.bin/zeabur-oss-release verify-assets downloaded
bun scripts/release.ts verify v0.1.1 linux arm64 downloaded
```

On Linux the last step installs the published chart on the development cluster, so it needs
Docker and port 443.
