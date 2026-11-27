# Releasing

How a maintainer publishes a release. What a release contains and how to verify it is in
[Releases](../reference/releases.mdx).

`.github/workflows/release.yml` runs on a `v*` tag. Its jobs run in this repository's Nix dev
shell, after `bun install`: Zeabur OSS's own steps are `scripts/release-*.sh`, and the steps every
zeabur-oss release shares are `bun x zeabur-oss-release <command>` of `@zeabur-oss/release`
(`package.json`, pinned in `bun.lock`), which pushes, signs and verifies images and charts, checks
the `CHANGELOG.md` section, and writes and verifies `SHA256SUMS`. A manual run of the workflow
rehearses: it checks, builds and smokes every platform, and publishes nothing. The draft
release, with its title and notes, is made by the Release pull request workflow when it tags.

Jobs that are not per platform run on Instabox (`instabox-small`). The platform builds run on
GitHub's runners: Linux amd64 and arm64, macOS arm64, and Windows for Zeabur Desktop.

Zeabur Desktop builds in the same workflow, and `publish` waits for it, so nothing is published
until every package is built (see [Zeabur Desktop](#zeabur-desktop)).

| Step | Where | Does |
|---|---|---|
| knope's `prepare-release` | once, in the release pull request | Sets the version in every file of `versioned_files` in `knope.toml`: both `Cargo.toml` files, each workspace crate in both lock files and `zeabur-proxy` in the fuzz crate's, the chart's `version` and `web/package.json`. The chart has no `appVersion`; its templates use the chart's `version`. `openapi.yaml` carries the API's own version, which a release does not change. |
| `check` job | once | The tag is SemVer without build metadata, `CHANGELOG.md` has its section, and a tag is on `main`. |
| `build` job | Linux amd64 and arm64, macOS arm64 | On a tag, the catalog pin resolves (`release-pins.sh`). Builds the platform (`release-build.sh`; on Linux also Zeabur OSS, Proxy and database updater images), then on Linux smokes the archive, the chart and the image on a k3s cluster with Zeabur Identity at `xtask/pins/zeabur-identity` (`release-smoke.sh`) and writes the image's size and the idle pod's working set to the run's summary. These are the release build's numbers; the CI jobs build with the faster `ci` profile. |
| `scripts/release-sign.sh` | macOS arm64, in the `build` job | Signs `zeaburctl` with the Developer ID and notarizes it, in the `apple-signing` environment, on `main` and tags only. Without the certificate it leaves the build as it is and warns. |
| `publish` job | once, tag only, after every build | Refuses when the release is already published. Its token cannot see a draft; the `release` job stops when the release is published. Pushes the images `zeabur`, `proxy` and `database-updater`, then the chart with all three pinned by digest, and signs them; stops when the chart would run any other image; records the debug and tools images the code pins in `digests.txt`, unsigned, and stops when the registry no longer serves either for both platforms (`release-default-images.sh`) and writes `published/subjects.json`, which `attest` attests. |
| `release` job | once, tag only | Copies the assets that `release-assets.sh` lists into `assets/`, adds the signed `SHA256SUMS`, attests them and creates the draft release with them, its title and the `CHANGELOG.md` notes (`zeabur-oss-release draft`). A draft that an earlier run left is replaced. |
| `verify` job, `scripts/release-verify.sh` | every platform | After the workflow has checked `SHA256SUMS` and each asset's attestation, checks that the draft has exactly the expected assets, each image and the chart as a user would, that the chart runs exactly the images in `digests.txt`, and that `digests.txt` names the debug and tools images the code pins (`crates/zeabur-api/src/settings.rs`), then installs the published chart on Linux (`release-smoke.sh`) or runs `zeaburctl` on macOS, where a Developer ID signature must be notarized (`spctl`), and checks Zeabur Desktop's `.dmg` the same way. |

The pins come from the releases of zeabur-oss/catalog and zeabur-oss/identity: the `SHA256SUMS` that
each signed, checked with cosign. Their signer is that repository's own `release.yml` or, for their
older releases, the shared release workflow of zeabur-oss/.github. The catalog files the build
embeds must be the ones its `SHA256SUMS` lists; Zeabur Identity's image is the `index` line of its
`digests.txt`.

Every signature and attestation names this repository's workflow,
`https://github.com/zeabur-oss/zeabur/.github/workflows/release.yml@refs/tags/v<version>`.
`verify` gets the repository and the ref in `RELEASE_SOURCE_REPOSITORY` and `RELEASE_SOURCE_REF`.
Releases before this one were signed by the shared release workflow of zeabur-oss/.github
([Releases](../reference/releases.mdx) shows how to verify both).

## Build files

`scripts/release-build.sh` builds on the platform it runs on, never for another one. The archive
entries carry the commit's time, so the same commit gives the same archives.

| Platform | Files |
|---|---|
| Linux amd64, arm64 | `zeaburctl-<version>-linux-<arch>.tar.gz` and `zeabur-<version>-linux-<arch>.tar.gz`, one static musl binary each with `LICENSE`; `image.tar` (Zeabur OSS's image) , `proxy-image.tar` (Zeabur Proxy's) and `database-updater-image.tar` (the optional CNPG updater) |
| macOS arm64 | `zeaburctl-<version>-darwin-arm64.tar.gz`. The server runs only in a pod, so there is no macOS `zeabur`. |

`scripts/release-smoke.sh` tries these files as they will ship, on the development cluster
(`scripts/dev-cluster.sh`). In `build` it runs the local image; in `verify` the node pulls the
published one. It never touches your kubeconfig.

## Zeabur Desktop

The `desktop-*` jobs of `release.yml` run beside the server's builds, and `publish` needs them;
a failed desktop build stops the release before anything is published. The app's
version is the repository's: knope sets `desktop/Cargo.toml` and its lock file, and Tauri reads
it from there (`desktop/tauri.conf.json` has none). `scripts/check-desktop.sh` fails in CI when it
differs from the root `Cargo.toml`.

| Job | Where | Does |
|---|---|---|
| `desktop-prepare` | Instabox, in the dev shell | Builds `web/dist` and `desktop/web/dist`, copies the catalog, and names the dev shell's nightly and `tauri-cli` (`scripts/ci/desktop-inputs.sh`). |
| `desktop-build` | macOS arm64, Windows amd64, Linux amd64 and arm64 (Ubuntu 24.04, for its older glibc) | Outside the Nix shell, with the runner's compiler, linker and libraries, the same nightly through rustup and `tauri-cli` from crates.io: compiles the app, then packages and signs it in a step of its own (`scripts/package-desktop.sh`). The compile step fails if the binary links anything from `/nix/store` (`otool -L` on macOS, `readelf -d` on Linux); the packages themselves are not scanned for libraries. |
| `desktop-updater` | Instabox, `main` and tags, once `desktop/tauri.conf.json` has an updater public key | Signs the `.app.tar.gz`, the `-setup.exe` and the `.AppImage` with `cargo tauri signer sign` and writes `latest.json` from the `.sig` files, with the `CHANGELOG.md` section as its notes (`scripts/desktop-release.ts`). It is the only job in the `desktop-updater` environment, which alone holds the updater key; a build job has one environment, the platform's signing one, so it cannot sign these files. The signature is not checked against `desktop/tauri.conf.json`'s public key before the release; a key that does not match shows when the first update is refused, so test one update after changing the key. |

| Platform | Files | The updater installs |
|---|---|---|
| macOS arm64 | `zeabur-desktop-v<version>-darwin-arm64.dmg`, `.app.tar.gz` | `.app.tar.gz` |
| Windows amd64 | `zeabur-desktop-v<version>-windows-amd64-setup.exe` (NSIS, per user), `-portable.exe` | `-setup.exe` |
| Linux amd64, arm64 | `zeabur-desktop-v<version>-linux-<arch>.deb`, `.rpm`, `.AppImage` | `.AppImage` |

The app lists the newest 30 releases and the latest release through the GitHub REST API. It reads
the `latest.json` of the newest release newer than itself that carries one: a release build takes
only releases, a pre-release build takes pre-releases too (`desktop/src/update.rs`). The latest
release is read on its own because many pre-releases can push it off the list. The API does not list drafts, so publishing the draft is
what offers an update. A `.deb`, `.rpm` or portable `.exe` finds its
platform in `latest.json` but cannot replace itself; the app offers the release page instead
(`desktop/src/update.rs`). The signed `SHA256SUMS`, its attestation and the checksums cover every
desktop file like the others. The organization makes releases immutable once published, so the draft must hold `latest.json` and `SHA256SUMS` with every other asset before it is published; neither can be added afterwards.

On macOS, the Tauri bundler signs the app with the Developer ID and the hardened runtime and no
entitlements, then notarizes and staples it; `package-desktop.sh` signs, notarizes and staples the
`.dmg`. On Windows, signtool signs the portable `.exe` and, as the bundler's `signCommand`, the app and
the installer, through Microsoft's
Artifact Signing plug-in (the `Microsoft.ArtifactSigning.Client` NuGet package, pinned by version
and SHA-256 in `package-desktop.sh`), after `azure/login` with OIDC. Linux packages are not signed
on their own; the signed `SHA256SUMS` and the attestation cover them.

Rehearsals on other branches build every platform unsigned: the
environments' deployment branches keep the credentials to `main` and `v*` tags, and the workflow
skips the Windows login and the updater job elsewhere.

## Homebrew

The `zeabur-oss/homebrew-tap` repository holds `Formula/zeaburctl.rb`, which installs `zeaburctl`
on macOS arm64 and Linux amd64 and arm64, and `Casks/zeabur-desktop.rb`, which installs Zeabur OSS
Desktop's `.dmg` on macOS arm64. Both point at the newest normal release's files, with the SHA-256
its `SHA256SUMS` lists, so Homebrew installs the same signed files and checks them.

A pre-release goes to the beta channel: `Formula/zeaburctl-beta.rb` and
`Casks/zeabur-desktop@beta.rb`, which never replace the normal files. The cask takes Homebrew's
`@<channel>` token for an
[alternative release channel](https://docs.brew.sh/Cask-Cookbook#casks-for-alternative-release-channels).
The formula cannot be `zeaburctl@beta`: Homebrew turns `@` into `AT` in a formula's class name
only before a digit, so `zeaburctl@beta` has no class it can load. Each beta file conflicts with
its normal one and the other way round, as both install the same `zeaburctl` or `Zeabur.app`. The
beta files stay at the newest pre-release, so after a normal release they can be older than the
normal files.

`scripts/homebrew.ts` writes the files; do not edit them in the tap by hand. Publishing a release
starts `.github/workflows/homebrew.yml` (`release: published`), and the tag picks the channel. It
checks that the release workflow signed the release's `SHA256SUMS` for this tag, installs the
files it wrote on macOS and Linux (and `spctl` must accept the app), and then zeabur-oss-bot
commits them to the tap's `main`. Nothing is committed when a check fails or the tap already has
the version. To redo it for a published release, run **Actions → Homebrew → Run workflow** with
the version.

The workflow needs the `ZEABUR_OSS_BOT_CLIENT_ID` variable and the `ZEABUR_OSS_BOT_PRIVATE_KEY`
secret to reach this repository, and the bot's GitHub App to be installed on the tap with
contents write.

The cask declares `auto_updates true`: the app's own updater stays on, and `brew upgrade` leaves it
to the app unless run with `--greedy`.

## Pre-releases

A tag with a pre-release part (`v0.1.2-beta.1`, `v1.0.0-rc.1`) is a
[SemVer 2.0.0](https://semver.org/spec/v2.0.0.html) pre-release: a GitHub pre-release, never
latest. Any other tag, `v0.*` included, is a normal release. Build metadata (`+...`) is refused.
Release it from `main` like any release, with `prerelease: beta`. The manifests carry the full version (`0.1.2-beta.1`), and
`CHANGELOG.md` has a section of its own for it, `## 0.1.2-beta.1 (2026-11-01)`. A normal release
pins only normal releases of the catalog and Zeabur Identity; a pre-release may pin pre-releases
too. On a commit tagged both `v0.1.2-beta.1` and `v0.1.2`, the pin is `v0.1.2`; of several tags of
one kind, the newest by SemVer §11. Anything that picks the newest version compares by SemVer §11,
not `sort -V`.

## Publish a release

1. Run **Actions → Release pull request → Run workflow** on `main`. Leave `version` empty to take
   the one the change files give (before 1.0, `minor` is a patch bump), or give `v<version>`;
   `prerelease: rc` makes `vX.Y.Z-rc.N`. knope turns the change files in `.changeset/` into the
   `## <version> (<date>)` section of `CHANGELOG.md` and sets `Cargo.toml`; `bump` sets the other
   manifests, `check` confirms them, and zeabur-oss-bot opens `chore(release): v<version>` from
   `release/next`. That section becomes the release notes as it is, so keep each paragraph and
   list item on one line, and edit `CHANGELOG.md` on that branch if it should read differently.
   A change file for a fixed vulnerability says `default: security`, which knope lists under
   `### Security` and releases as a patch. The update check in the interface and in `zeaburctl`
   looks for that heading to tell people their version has known security problems, as
   docs/reference/telemetry.mdx describes.
   The catalog commit in `flake.lock` is what the release embeds: it must carry a published
   release tag, with its signed `SHA256SUMS`. `check` refuses a pin without a release tag, so
   zeabur-oss-bot opens no release pull request until the catalog is released and the pin moved
   to it. The pins in `xtask/pins` are not part of a release: they only choose what the
   development cluster, the cluster tests and the release smoke install.
2. Rehearse: **Actions → Release → Run workflow** on `release/next` with the version, `v<version>`. It
   runs the three builds with their smoke tests and publishes nothing; the pins are
   not checked.
3. Before the first release only, see [Settings the workflow needs](#settings-the-workflow-needs).
4. **Screenshots** starts on its own with the release pull request and commits the docs screenshots
   that changed onto `release/next`, which runs **CI** again. Look at the images in the diff. The
   release doesn't wait for it: if it fails or you merge first, the release goes out with the
   screenshots on main.
5. Once **CI** has passed on the release pull request, merge it. zeabur-oss-bot tags the merge
   commit `v<version>`, which starts the **Release** run.
6. Watch the **Release** run. When it succeeds, the images and the chart are in the registry (their
   tags are public while the release is still a draft) and the draft release has every asset.
   The `verify` jobs have downloaded the draft's assets on fresh runners, checked every
   signature, checksum and attestation without a registry login, installed the published chart
   on a new k3s cluster on Linux amd64 and arm64, whose node pulled the published image, and run
   `zeaburctl` on macOS arm64.
7. Check the draft: the notes, the asset list, the digests. Optionally install it on a cluster of
   your own as [Install on an existing cluster](../install/existing-cluster.mdx) describes.
8. Publish it: on the release page, **Edit → Publish release**, or
   `gh release edit v0.1.1 --repo zeabur-oss/zeabur --draft=false`. Then download it once without signing in and repeat
   the checks above against the public release.

## Milestones

Each repository has one open version milestone, the release that comes next (`v0.2.1`), and `Later`
for work that is not committed to a release. New work that is committed goes in the version
milestone, and so does every pull request that closes such an issue. If the release turns out to be
a minor one, rename the milestone; its issues stay.

After publishing a release:

1. Close its milestone, and move what is still open in it to the next one.
2. Open the milestone of the release after it: the next patch version, unless the plan says
   otherwise.

A release is ready when its milestone has no open issue.

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
- **Tags.** A ruleset that lets zeabur-oss-bot create `v*` tags while nobody may move or delete them.
- **Labels.** `changelog: none`.
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

  Without them, releases go on with `zeaburctl` and Zeabur Desktop signed ad hoc only. Zeabur OSS
  Desktop's macOS build uses the same environment and secrets.
- **Windows signing.** An Azure Artifact Signing account with a certificate profile for public
  trust, and a Microsoft Entra app registration whose service principal has the **Artifact
  Signing Certificate Profile Signer** role on that profile. The app registration has a federated
  credential for GitHub Actions with the subject
  `repo:zeabur-oss/zeabur:environment:windows-signing`. An environment `windows-signing` whose
  deployment branches are `main` and `v*` tags, with these variables (none is secret):
  - `AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`: the app registration and the
    subscription of the signing account.
  - `AZURE_SIGNING_ENDPOINT`: the account's region endpoint, such as
    `https://eus.codesigning.azure.net`.
  - `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_PROFILE`: the account's and the certificate profile's
    names.

  Without `AZURE_CLIENT_ID`, the Windows files are released unsigned.
- **Updater key.** Zeabur Desktop's updates are signed with a minisign key of their own. Made
  once, on a maintainer's machine, with a password:
  `cargo tauri signer generate -w ~/.tauri/zeabur-desktop.key` (in `nix develop .#desktop`).
  Commit the public key (`~/.tauri/zeabur-desktop.key.pub`, one line) as
  `plugins.updater.pubkey` in `desktop/tauri.conf.json`, and create an environment
  `desktop-updater` whose deployment branches are `main` and `v*` tags, with the secrets
  `TAURI_SIGNING_PRIVATE_KEY` (the private key file's text) and
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Keep the key file somewhere safe: an app only accepts
  updates signed with the key it was built with, so losing it means people must download the next
  version by hand. The public key (key id `C2994A9A6B0BA88C`) is committed and the environment holds
  the private key, so every release has a `latest.json`. A build without a public key, a fork's
  for one, has no `latest.json` and the app does not check for updates.
- **Telemetry key.** A repository variable `POSTHOG_KEY`, the PostHog project key of the
  experience program (docs/reference/telemetry.mdx). Tag builds of `build` and `desktop-build` compile it in as
  `ZEABUR_TELEMETRY_KEY`; every other build, local ones included, has none and sends nothing.
- **Workflow permissions** can stay read-only by default: each job of `release.yml` asks for
  what it needs. `desktop-build` has `id-token: write` for `azure/login`. `publish` has `packages: write` and
  `id-token: write` (keyless signing); `attest` has `attestations: write`; `release` has
  `contents: write` (the draft), `id-token: write` and `attestations: write`; `verify` has
  `contents: write`, because GitHub shows a draft release only to a token that can push, and
  `attestations: read`.

## When a release run fails

Never move a published tag and never replace a published asset or image tag:
the `publish` job refuses when the release is already published, or an image tag or the chart
version already exists.

- **Before `publish`** (CI, a build, a smoke test): nothing was published, but the tag stays: the
  ruleset "release tags" refuses to delete or move a `v*` tag. Fix it on `main`, rehearse
  (a dispatch on `main` signs as a tag build does), then release the next patch version. In one
  pull request, restore the `.changeset/` files that the release commit removed and delete its
  `CHANGELOG.md` section, so the next release lists those changes and the failed version never
  appears. Rename the milestones up one version, starting with the highest.
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
export RELEASE_SOURCE_REPOSITORY=zeabur-oss/zeabur RELEASE_SOURCE_REF=refs/tags/v0.1.1
gh release download v0.1.1 --repo zeabur-oss/zeabur --dir downloaded
bun x zeabur-oss-release verify-assets downloaded
scripts/release-verify.sh v0.1.1 downloaded
```

On Linux the last step installs the published chart on the development cluster, so it needs
Docker and port 443.
