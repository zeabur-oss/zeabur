// Zeabur Desktop's release files and its update manifest, latest.json, which the app's updater
// reads from the newest release (.github/workflows/desktop.yml, docs/internal/releasing.md).
//
//   bun scripts/desktop-release.ts updater <version> <dir> <notes.md> <latest.json>
//
// <dir> holds the packages of every build. The command signs each file the updater installs with
// `cargo tauri signer sign` (the key in TAURI_SIGNING_PRIVATE_KEY and its password), checks each
// signature against the public key in desktop/tauri.conf.json, and writes the manifest.
import { createHash, createPublicKey, verify } from "node:crypto";
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

// Whether `signature` (a .sig of `cargo tauri signer sign`) signs `data` and `version` with
// `publicKey` (tauri.conf.json's): the minisign format, both base64 encoded as Tauri keeps them.
// The updater checks the same with `requireSignedVersion`.
export function signs(
  publicKey: string,
  signature: string,
  data: Uint8Array,
  version: string,
) {
  const key = Buffer.from(lines(publicKey)[1] ?? "", "base64");
  const [, sigLine = "", comment = "", globalLine = ""] = lines(signature);
  const sig = Buffer.from(sigLine, "base64");
  const trusted = comment.replace(/^trusted comment: /, "");
  if (
    key.length !== 42 ||
    sig.length !== 74 ||
    !key.subarray(2, 10).equals(sig.subarray(2, 10))
  )
    return false;
  const ed25519 = createPublicKey({
    key: {
      kty: "OKP",
      crv: "Ed25519",
      x: key.subarray(10).toString("base64url"),
    },
    format: "jwk",
  });
  const algorithm = sig.subarray(0, 2).toString();
  const signed =
    algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  const raw = sig.subarray(10);
  return (
    (algorithm === "ED" || algorithm === "Ed") &&
    verify(null, signed, ed25519, raw) &&
    verify(
      null,
      Buffer.concat([raw, Buffer.from(trusted)]),
      ed25519,
      Buffer.from(globalLine, "base64"),
    ) &&
    trusted.split("\t").includes(`version:${version}`)
  );
}

function lines(base64: string): string[] {
  return Buffer.from(base64.trim(), "base64").toString().split("\n");
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

if (import.meta.main) {
  const [command, tag, dir, notes, out] = process.argv.slice(2);
  const version = tag ? parseVersion(tag) : undefined;
  if (command !== "updater" || !version || !dir || !notes || !out) {
    console.error(
      "usage: bun scripts/desktop-release.ts updater <version> <dir> <notes.md> <latest.json>",
    );
    process.exit(2);
  }
  const publicKey = updaterKey();
  if (!publicKey) {
    console.error("desktop/tauri.conf.json has no updater public key");
    process.exit(1);
  }
  const signatures = new Map<string, string>();
  for (const build of desktopBuilds(version)) {
    const file = join(dir, build.updater.file);
    const signer = Bun.spawnSync(
      [
        "cargo",
        "tauri",
        "signer",
        "sign",
        "--app-version",
        version.plain,
        file,
      ],
      { stdout: "ignore", stderr: "inherit" },
    );
    if (signer.exitCode !== 0) process.exit(1);
    const signature = await Bun.file(`${file}.sig`).text();
    if (
      !signs(publicKey, signature, await Bun.file(file).bytes(), version.plain)
    ) {
      console.error(
        `${build.updater.file}: the signature does not match tauri.conf.json's key`,
      );
      process.exit(1);
    }
    signatures.set(build.updater.file, signature);
  }
  const repository = process.env.GITHUB_REPOSITORY || "zeabur-oss/zeabur";
  const result = manifest(
    version,
    repository,
    await Bun.file(notes).text(),
    new Date(),
    (file) => signatures.get(file) ?? "",
  );
  await Bun.write(out, `${JSON.stringify(result, null, 2)}\n`);
  console.log(
    `${out}: ${version.plain}, ${Object.keys(result.platforms).join(", ")}`,
  );
}
