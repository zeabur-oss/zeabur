// The Homebrew formula for zeaburctl and the cask for Zeabur Desktop, which live in the
// zeabur-oss/homebrew-tap repository (docs/internal/releasing.md#homebrew).
// .github/workflows/homebrew.yml runs this when a release is published.
//
//   bun scripts/homebrew.ts <version> <sums-dir> <out-dir>
//
// <sums-dir> holds the release's SHA256SUMS and SHA256SUMS.sigstore.json. The command checks that
// the release workflow (this repository's, or the shared one before v0.3.0) signed SHA256SUMS for
// this repository at the tag, then writes
// <out-dir>/Formula/zeaburctl.rb and, when the release has the macOS app,
// <out-dir>/Casks/zeabur-desktop.rb.
import { Failure, sums } from "@zeabur-oss/release";
import { $ } from "bun";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { parseVersion, type Version } from "./versions";

const repository = "zeabur-oss/zeabur";
const homepage = "https://oss.zeabur.com/docs/zeabur/";

const download = (version: Version, file: string) =>
  `https://github.com/${repository}/releases/download/${version.tag}/${file}`;

function sha(checksums: Map<string, string>, file: string): string {
  const sum = checksums.get(file);
  if (!sum) throw new Failure(`SHA256SUMS does not list ${file}`);
  return sum;
}

export function formula(
  version: Version,
  checksums: Map<string, string>,
): string {
  const build = (platform: string, indent: string) => {
    const file = `zeaburctl-${version.tag}-${platform}.tar.gz`;
    return [
      `${indent}url "${download(version, file)}"`,
      `${indent}sha256 "${sha(checksums, file)}"`,
    ].join("\n");
  };
  return `# frozen_string_literal: true

# Written by scripts/homebrew.ts in zeabur-oss/zeabur for each release; do not edit.
class Zeaburctl < Formula
  desc "Open Zeabur for any cluster in your kubeconfig"
  homepage "${homepage}"
  license "Apache-2.0"

  on_macos do
    depends_on arch: :arm64

    on_arm do
${build("darwin-arm64", "      ")}
    end
  end

  on_linux do
    on_intel do
${build("linux-amd64", "      ")}
    end

    on_arm do
${build("linux-arm64", "      ")}
    end
  end

  def install
    bin.install "zeaburctl"
  end

  test do
    assert_equal "zeaburctl #{version}", shell_output("#{bin}/zeaburctl --version").strip
  end
end
`;
}

// undefined when the release has no macOS app.
export function cask(
  version: Version,
  checksums: Map<string, string>,
): string | undefined {
  const file = `zeabur-desktop-${version.tag}-darwin-arm64.dmg`;
  if (!checksums.has(file)) return undefined;
  const id = "com.zeabur.oss.desktop";
  return `# Written by scripts/homebrew.ts in zeabur-oss/zeabur for each release; do not edit.
cask "zeabur-desktop" do
  version "${version.plain}"
  sha256 "${sha(checksums, file)}"

  url "${download(version, file).replaceAll(version.plain, "#{version}")}"
  name "Zeabur Desktop"
  desc "Open Zeabur for the clusters in your kubeconfig"
  homepage "${homepage}"

  # The app updates itself, so brew upgrade leaves it to the app unless asked with --greedy.
  auto_updates true
  depends_on arch: :arm64
  depends_on :macos

  app "Zeabur.app"

  zap trash: [
    "~/Library/Application Support/${id}",
    "~/Library/Caches/${id}",
    "~/Library/WebKit/${id}",
  ]
end
`;
}

export async function write(
  version: Version,
  checksums: Map<string, string>,
  out: string,
) {
  await mkdir(join(out, "Formula"), { recursive: true });
  await Bun.write(
    join(out, "Formula/zeaburctl.rb"),
    formula(version, checksums),
  );
  const desktop = cask(version, checksums);
  if (!desktop) return;
  await mkdir(join(out, "Casks"), { recursive: true });
  await Bun.write(join(out, "Casks/zeabur-desktop.rb"), desktop);
}

if (import.meta.main) {
  const [tag, dir, out] = process.argv.slice(2);
  const version = parseVersion(tag ?? "");
  if (!version || !dir || !out) {
    console.error(
      "usage: bun scripts/homebrew.ts <version> <sums-dir> <out-dir>",
    );
    process.exit(2);
  }
  if (version.prerelease) {
    console.error(`${tag} is a pre-release, which Homebrew does not offer`);
    process.exit(1);
  }
  const list = join(dir, "SHA256SUMS");
  await $`cosign verify-blob --bundle ${join(dir, "SHA256SUMS.sigstore.json")} \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    --certificate-identity-regexp ${"^https://github\\.com/zeabur-oss/(zeabur|\\.github)/\\.github/workflows/release\\.yml@"} \
    --certificate-github-workflow-repository ${repository} \
    --certificate-github-workflow-ref refs/tags/${version.tag} ${list}`;
  await write(version, sums(await Bun.file(list).text()), out);
}
