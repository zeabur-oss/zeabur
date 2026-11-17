---
default: minor
---

# Zeabur Desktop on the release page, with updates

Each release now carries Zeabur Desktop for macOS on Apple silicon (`.dmg`), Windows on x64 (an installer and a portable `.exe`) and Linux on amd64 and arm64 (`.deb`, `.rpm` and `.AppImage`), covered by the release's signed `SHA256SUMS` and attestation. The app checks for a new version when it starts and from **Check for updates**, shows what changed, and installs only when you confirm. The macOS app, the Windows installer and the AppImage update themselves; the others open the release page.

Releases are now signed by this repository's own release workflow, `zeabur-oss/zeabur/.github/workflows/release.yml`. To verify a release, accept that signer as well as the shared workflow that signed earlier releases; [Releases](docs/reference/releases.mdx) shows the command.
