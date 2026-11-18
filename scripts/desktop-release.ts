// Zeabur Desktop's release files and its update manifest, latest.json, which the app's updater
// reads from the newest release (.github/workflows/release.yml, docs/internal/releasing.md).
//
//   bun scripts/desktop-release.ts assets <version>
//   bun scripts/desktop-release.ts updater <version> <dir> <notes.md> <latest.json>
//
// `assets` prints each release file of the app and the workflow artifact it comes from, one
// `<file> <artifact>` line each (scripts/release-assets.sh). For `updater`, <dir> holds the
// packages of every build; the command signs each file the updater installs with `cargo tauri
// signer sign` (the key in TAURI_SIGNING_PRIVATE_KEY and its password) and writes the manifest from
// the `.sig` files next to them.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseVersion, type Version } from "./versions";

// One build of the app: what scripts/package-desktop.sh leaves for it, and which file the updater
// installs there.
type Build = {
  os: "darwin" | "windows" | "linux";
  arch: "amd64" | "arm64";
  files: string[];
  updater: { file: string; installer: string };
};

const updaterArch = { amd64: "x86_64", arm64: "aarch64" } as const;

export function desktopBuilds(version: Version): Build[] {
  const name = (build: string, suffix: string) =>
    `zeabur-desktop-${version.tag}-${build}${suffix}`;
  const builds: Build[] = [
    {
      os: "darwin",
      arch: "arm64",
      files: [
        name("darwin-arm64", ".dmg"),
        name("darwin-arm64", ".app.tar.gz"),
      ],
      updater: { file: name("darwin-arm64", ".app.tar.gz"), installer: "app" },
    },
    {
      os: "windows",
      arch: "amd64",
      files: [
        name("windows-amd64", "-setup.exe"),
        name("windows-amd64", "-portable.exe"),
      ],
      updater: { file: name("windows-amd64", "-setup.exe"), installer: "nsis" },
    },
  ];
  for (const arch of ["amd64", "arm64"] as const)
    builds.push({
      os: "linux",
      arch,
      files: [".deb", ".rpm", ".AppImage"].map((ext) =>
        name(`linux-${arch}`, ext),
      ),
      updater: {
        file: name(`linux-${arch}`, ".AppImage"),
        installer: "appimage",
      },
    });
  return builds;
}

// The updater's public key in desktop/tauri.conf.json; empty until the project has an updater key,
// and then the release has no latest.json and the app does not check.
export function updaterKey(): string {
  const config = JSON.parse(
    readFileSync(join(import.meta.dir, "../desktop/tauri.conf.json"), "utf8"),
  );
  return config.plugins?.updater?.pubkey ?? "";
}

// Every release asset of the app and the workflow artifact it comes from.
export function desktopAssets(
  version: Version,
  updater: boolean,
): [string, string][] {
  const assets: [string, string][] = [];
  for (const build of desktopBuilds(version))
    for (const file of build.files)
      assets.push([file, `desktop-${build.os}-${build.arch}`]);
  if (updater) assets.push(["latest.json", "desktop-updater"]);
  return assets;
}

export type Manifest = {
  version: string;
  notes: string;
  pub_date: string;
  platforms: Record<string, { url: string; signature: string }>;
};

// The manifest of the Tauri updater's static JSON format. Each build is listed under
// `<os>-<arch>-<installer>`, which the updater looks for first, and under `<os>-<arch>`, which a
// copy without an installer type (a .deb, the portable .exe) finds: the app offers those people a
// download instead (desktop/src/update.rs).
export function manifest(
  version: Version,
  repository: string,
  notes: string,
  date: Date,
  signature: (file: string) => string,
): Manifest {
  const platforms: Manifest["platforms"] = {};
  for (const build of desktopBuilds(version)) {
    const entry = {
      url: `https://github.com/${repository}/releases/download/${version.tag}/${build.updater.file}`,
      signature: signature(build.updater.file).trim(),
    };
    if (!entry.signature)
      throw new Error(`the signature of ${build.updater.file} is empty`);
    const platform = `${build.os}-${updaterArch[build.arch]}`;
    platforms[platform] = entry;
    platforms[`${platform}-${build.updater.installer}`] = entry;
  }
  return {
    version: version.plain,
    notes: notes.trim(),
    pub_date: date.toISOString().replace(/\.\d{3}Z$/, "Z"),
    platforms,
  };
}

async function updater(
  version: Version,
  dir: string,
  notes: string,
  out: string,
) {
  for (const build of desktopBuilds(version)) {
    const signer = Bun.spawnSync(
      [
        "cargo",
        "tauri",
        "signer",
        "sign",
        "--app-version",
        version.plain,
        join(dir, build.updater.file),
      ],
      { stdout: "ignore", stderr: "inherit" },
    );
    if (signer.exitCode !== 0) process.exit(1);
  }
  const repository = process.env.GITHUB_REPOSITORY || "zeabur-oss/zeabur";
  const result = manifest(
    version,
    repository,
    await Bun.file(notes).text(),
    new Date(),
    (file) => readFileSync(join(dir, `${file}.sig`), "utf8"),
  );
  await Bun.write(out, `${JSON.stringify(result, null, 2)}\n`);
  console.log(
    `${out}: ${version.plain}, ${Object.keys(result.platforms).join(", ")}`,
  );
}

if (import.meta.main) {
  const [command, tag, dir, notes, out] = process.argv.slice(2);
  const version = tag ? parseVersion(tag) : undefined;
  if (command === "assets" && version && !dir)
    for (const [file, artifact] of desktopAssets(version, updaterKey() !== ""))
      console.log(`${file} ${artifact}`);
  else if (command === "updater" && version && dir && notes && out)
    await updater(version, dir, notes, out);
  else {
    console.error(
      "usage: bun scripts/desktop-release.ts assets <version>\n       bun scripts/desktop-release.ts updater <version> <dir> <notes.md> <latest.json>",
    );
    process.exit(2);
  }
}
