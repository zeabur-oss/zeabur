// One stage of a release, as the shared release workflow of zeabur-oss/.github runs it, in the dev
// shell of the commit being released: `bun scripts/release.ts <stage> <version> ...`
// (docs/internal/releasing.md). @zeabur-oss/release (zeabur-oss/.github, release/) pushes, signs
// and verifies; the workflow itself checks the CHANGELOG.md section, signs SHA256SUMS and writes
// the notes. This script keeps what is Zeabur's own.
//
//   bump    <version>                                 after knope set Cargo.toml: Cargo.lock, the
//                                                     chart, the web packages and the API document
//   check   <version>                                 the version is the manifests', and the chart
//                                                     runs only images the release publishes
//   build   <version> <os> <arch> <dist>              build one platform (release-build.sh), smoke
//                                                     it on k3s on Linux (release-smoke.sh)
//   sign    <version> <os> <arch> <dist>              macOS: zeaburctl signed with the Developer ID
//                                                     and notarized
//   publish <version> <artifacts> <published>         push and sign the images and the chart by
//                                                     digest
//   finish  <version> <os> <arch> <artifacts> <dist>  the installer, pinned to the catalog, Zeabur
//                                                     Identity and what publish pushed
//   assets  <version> <artifacts> <assets>            copy the release's files into <assets>
//   verify  <version> <os> <arch> <downloaded>        the draft's images, chart and programs, as a
//                                                     user gets them
//
// <artifacts> holds the workflow's earlier artifacts: candidate-<os>-<arch>/ from build,
// published/ from publish and finish-<os>-<arch>/ from finish. The platforms are linux-amd64,
// linux-arm64 and darwin-arm64 (zeaburctl only); each build is native.
import {
  Failure,
  absent,
  pinDigest,
  pushChart,
  pushImage,
  readDigests,
  recorded,
  sha256,
  sums,
  verifyChart,
  verifyImage,
} from "@zeabur-oss/release";
import { $ } from "bun";
import { existsSync, rmSync } from "node:fs";
import { cp, mkdir, mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { parseVersion, pinTag, type Version } from "./versions";

const root = resolve(import.meta.dir, "..");
const scripts = join(root, "scripts");
const chartDir = join(root, "deploy/chart/zeabur");
$.cwd(root);

// The images, ghcr.io/<owner>/<name>, with their label in digests.txt (none for Zeabur's), the
// archive build leaves and the key path of their digest in values.yaml.
const images = [
  {
    name: "zeabur",
    label: "",
    tar: "image.tar",
    values: ["image"],
  },
  {
    name: "proxy",
    label: "proxy",
    tar: "proxy-image.tar",
    values: ["proxy", "image"],
  },
];
const builds = ["linux-amd64", "linux-arm64", "darwin-arm64"];

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

function env(name: string): string {
  return process.env[name] || fail(`${name} is not set`);
}

// The platform of the arguments, which must be the machine's: every build is native.
function platform(os: string, arch: string) {
  if (!builds.includes(`${os}-${arch}`))
    fail(`no release for ${os}-${arch}: ${builds.join(", ")}`);
  const here = `${process.platform}-${{ x64: "amd64" }[process.arch as "x64"] ?? process.arch}`;
  if (here !== `${os}-${arch}`)
    fail(`this machine is ${here}, not ${os}-${arch}`);
}

// A directory of this run's own, removed when it ends, with an empty registry login. Helm reads
// the same file as crane.
async function scratch(): Promise<{
  dir: string;
  env: Record<string, string>;
}> {
  const dir = await mkdtemp(join(tmpdir(), "release-"));
  process.on("exit", () => rmSync(dir, { recursive: true, force: true }));
  const docker = join(dir, "docker");
  await mkdir(docker);
  return {
    dir,
    env: {
      DOCKER_CONFIG: docker,
      HELM_REGISTRY_CONFIG: join(docker, "config.json"),
    },
  };
}

// The digest a checksum list has for a file.
function listed(list: Map<string, string>, file: string): string {
  return list.get(file) ?? fail(`no digest of ${file} in its list`);
}

// Every release asset and the artifact it comes from.
function assetSources(version: Version, artifacts: string): [string, string][] {
  const v = version.tag;
  const files: [string, string][] = [
    ["digests.txt", "published"],
    [`zeabur-${version.plain}.tgz`, "published"],
  ];
  for (const build of builds)
    files.push([`zeaburctl-${v}-${build}.tar.gz`, `candidate-${build}`]);
  for (const arch of ["amd64", "arm64"])
    files.push(
      [`zeabur-${v}-linux-${arch}.tar.gz`, `candidate-linux-${arch}`],
      [`zeabur-installer-${v}-linux-${arch}.tar.gz`, `finish-linux-${arch}`],
    );
  return files.map(([file, dir]) => [file, join(artifacts, dir, file)]);
}

// Every image the chart runs, with each optional workload on.
async function chartImages(chart: string): Promise<string> {
  const docs =
    await $`helm template zeabur ${chart} --set publicUrl=https://zeabur.example.com --set issuer.url=https://identity.example.com --set issuer.clientSecret.existingSecret=zeabur-oidc --set proxy.enabled=true`.text();
  const found = new Set<string>();
  const walk = (value: unknown) => {
    if (Array.isArray(value)) value.forEach(walk);
    else if (value && typeof value === "object") {
      const image = (value as { image?: unknown }).image;
      if (typeof image === "string") found.add(image);
      Object.values(value).forEach(walk);
    }
  };
  walk(Bun.YAML.parse(docs));
  return [...found].sort().join(", ");
}

// The catalog and Zeabur Identity releases that zeabur-installer installs, as ZEABUR_PIN_*
// values. The catalog is the commit in flake.lock, Zeabur Identity the release tag (or a commit
// that carries one) in ZEABUR_IDENTITY_PINNED. Each value comes from the SHA256SUMS that the
// product's release signed, and cosign checks that signature first.
async function pins(version: Version): Promise<Record<string, string>> {
  const work = (await scratch()).dir;
  const tagOf = async (repository: string, pin: string) =>
    pinTag(
      `zeabur-oss/${repository}`,
      version,
      pin,
      await $`git ls-remote --tags https://github.com/zeabur-oss/${repository}`.text(),
    );
  const catalog = await tagOf("catalog", env("ZEABUR_CATALOG_COMMIT"));
  const identity = await tagOf(
    "identity",
    (await Bun.file(join(root, "ZEABUR_IDENTITY_PINNED")).text()).trim(),
  );

  const download = async (repository: string, tag: string, file: string) => {
    const to = join(work, repository, file);
    await $`curl --fail --silent --show-error --location --retry 3 --create-dirs --output ${to} https://github.com/zeabur-oss/${repository}/releases/download/${tag}/${file}`;
    return to;
  };
  // Signed by zeabur-oss/.github's shared release workflow, run for that repository's tag (the
  // signer of every zeabur-oss release from v0.1.1 on).
  const signedList = async (repository: string, tag: string, file: string) => {
    const list = await download(repository, tag, file);
    const bundle = await download(repository, tag, `${file}.sigstore.json`);
    await $`cosign verify-blob --bundle ${bundle} --certificate-identity-regexp ${"^https://github\\.com/zeabur-oss/\\.github/\\.github/workflows/release\\.yml@"} --certificate-github-workflow-repository zeabur-oss/${repository} --certificate-github-workflow-ref refs/tags/${tag} --certificate-oidc-issuer https://token.actions.githubusercontent.com ${list}`;
    return list;
  };

  // Each catalog file this build embeds is one the catalog's release lists.
  const catalogList = sums(
    await Bun.file(await signedList("catalog", catalog, "SHA256SUMS")).text(),
  );
  const local = env("ZEABUR_CATALOG_RELEASE");
  for (const file of await readdir(local))
    if ((await sha256(join(local, file))) !== listed(catalogList, file))
      throw new Failure(
        `${file} of ZEABUR_CATALOG_RELEASE is not the one zeabur-oss/catalog ${catalog} released`,
      );
  const identityList = sums(
    await Bun.file(await signedList("identity", identity, "SHA256SUMS")).text(),
  );
  const digests = await download("identity", identity, "digests.txt");
  if ((await sha256(digests)) !== listed(identityList, "digests.txt"))
    throw new Failure("digests.txt does not match SHA256SUMS");
  const image = recorded(await readDigests(digests), "index").toString();
  if (!/^ghcr\.io\/zeabur-oss\/identity@/.test(image))
    throw new Failure(
      `digests.txt names ${image}, not ghcr.io/zeabur-oss/identity`,
    );
  const ctl = (arch: string) =>
    listed(identityList, `zeabur-identityctl-${identity}-linux-${arch}.tar.gz`);
  return {
    ZEABUR_PIN_CATALOG_VERSION: catalog,
    ZEABUR_PIN_CATALOG_SHA256: listed(catalogList, "index.json"),
    ZEABUR_PIN_IDENTITY_VERSION: identity,
    ZEABUR_PIN_IDENTITY_IMAGE: image,
    ZEABUR_PIN_IDENTITYCTL_AMD64_SHA256: ctl("amd64"),
    ZEABUR_PIN_IDENTITYCTL_ARM64_SHA256: ctl("arm64"),
  };
}

// knope (knope.toml) has set [workspace.package] in Cargo.toml; the workspace's crates in
// Cargo.lock, the chart, the web packages and the API document (openapi.yaml and the client
// generated from it) follow.
async function bump(version: Version) {
  const plain = version.plain;
  const metadata =
    await $`cargo metadata --manifest-path ${join(root, "Cargo.toml")} --no-deps --format-version 1`.json();
  const edit = async (file: string, change: (text: string) => string) => {
    const path = join(root, file);
    const text = await Bun.file(path).text();
    const changed = change(text);
    if (changed === text && !text.includes(plain))
      fail(`${file} has no version to set`);
    await Bun.write(path, changed);
  };
  await edit("Cargo.lock", (lock) => {
    for (const { name } of metadata.packages as { name: string }[]) {
      const entry = new RegExp(`(\nname = "${name}"\nversion = )"[^"]*"`);
      if (!entry.test(lock)) fail(`Cargo.lock has no package ${name}`);
      lock = lock.replace(entry, `$1"${plain}"`);
    }
    return lock;
  });
  await edit("deploy/chart/zeabur/Chart.yaml", (chart) =>
    chart
      .replace(/^version: .*$/m, `version: ${plain}`)
      .replace(/^appVersion: .*$/m, `appVersion: "${plain}"`),
  );
  for (const manifest of [
    "web/package.json",
    "crates/zeabur-installer/web/package.json",
  ])
    await edit(manifest, (text) =>
      text.replace(/^(  "version": )"[^"]*"/m, `$1"${plain}"`),
    );
  await edit("crates/zeabur-api/openapi.yaml", (text) =>
    text.replace(/^(  version: )"[^"]*"/m, `$1"${plain}"`),
  );
  await edit("web/src/api/zeabur.ts", (text) =>
    text.replace(/^( \* OpenAPI spec version: ).*$/m, `$1${plain}`),
  );
}

async function check(version: Version) {
  const plain = version.plain;
  const problems: string[] = [];
  // Every crate of the workspace (Cargo.lock included, through --locked), the chart, the web
  // packages and the API document carry the version.
  const metadata =
    await $`cargo metadata --locked --no-deps --format-version 1`.json();
  for (const p of metadata.packages as { name: string; version: string }[])
    if (p.version !== plain)
      problems.push(`crate ${p.name} is ${p.version}, not ${plain}`);
  const yaml = async (file: string) =>
    Bun.YAML.parse(await Bun.file(join(root, file)).text()) as Record<
      string,
      any
    >;
  const chart = await yaml("deploy/chart/zeabur/Chart.yaml");
  if (String(chart.version) !== plain || String(chart.appVersion) !== plain)
    problems.push(
      `deploy/chart/zeabur/Chart.yaml has version ${chart.version} and appVersion ${chart.appVersion}, not ${plain}`,
    );
  for (const manifest of [
    "web/package.json",
    "crates/zeabur-installer/web/package.json",
  ]) {
    const found = (await Bun.file(join(root, manifest)).json()).version;
    if (found !== plain)
      problems.push(`${manifest} has version ${found}, not ${plain}`);
  }
  const api = (await yaml("crates/zeabur-api/openapi.yaml")).info.version;
  if (String(api) !== plain)
    problems.push(
      `crates/zeabur-api/openapi.yaml has version ${api}, not ${plain}`,
    );
  for (const ref of (await chartImages(chartDir)).split(", ")) {
    const repository = ref.split("@")[0]!.replace(/:[^:/]+$/, "");
    const name = repository.replace(/^ghcr\.io\/zeabur-oss\//, "");
    if (name !== repository && !images.some((i) => i.name === name))
      problems.push(
        `the chart runs ${ref}, which the release does not publish`,
      );
  }
  if (problems.length) fail(problems.join("\n"));
}

async function build(version: Version, os: string, arch: string, dist: string) {
  platform(os, arch);
  await mkdir(dist, { recursive: true });
  // A tag's binaries embed the catalog bundle, which must be the one the catalog's release
  // signed, and its installer installs the pinned Zeabur Identity release: both pins must resolve
  // before anything is built. A rehearsal builds whatever the pins name.
  const tag = process.env.GITHUB_EVENT_NAME === "push";
  if (tag) await pins(version);
  await $`${scripts}/release-build.sh ${version.tag} ${arch} ${dist}`;
  if (os === "linux") {
    // The archive, the chart and the image as built, on the development cluster with Zeabur
    // Identity at ZEABUR_IDENTITY_PINNED (scripts/dev-cluster.sh fetches it).
    await $`${scripts}/release-smoke.sh ${join(dist, `zeaburctl-${version.tag}-linux-${arch}.tar.gz`)} ${chartDir} zeabur:release-${arch}`;
    // A rehearsal publishes nothing, so finish never runs: a development installer, without
    // pins, shows that it builds. It is not kept.
    if (!tag) {
      const { dir } = await scratch();
      await $`${scripts}/release-build.sh ${version.tag} ${arch} ${dir} installer`;
    }
  }
  await $`ls -l ${dist}`;
}

async function sign(version: Version, os: string, arch: string, dist: string) {
  platform(os, arch);
  if (os !== "darwin") fail("only the macOS build is signed");
  // The shared workflow passes the secrets only on main and tags. Without them zeaburctl keeps
  // the linker's ad hoc signature, and the release goes on.
  if (!process.env.APPLE_CERTIFICATE_P12) {
    console.log(
      "::warning::no Developer ID certificate: zeaburctl is not signed or notarized",
    );
    return;
  }
  const password = env("APPLE_CERTIFICATE_PASSWORD");
  const notary = [
    "--key-id",
    env("APPLE_API_KEY_ID"),
    "--issuer",
    env("APPLE_API_ISSUER_ID"),
  ];
  const archive = join(dist, `zeaburctl-${version.tag}-${os}-${arch}.tar.gz`);
  const { dir: work } = await scratch();
  const keychain = join(work, "signing.keychain-db");
  // Apple's tools, not the Nix shell's: its codesign is sigtool's, and its DEVELOPER_DIR and
  // SDKROOT point xcrun at a bare SDK without notarytool.
  const apple: Record<string, string | undefined> = { ...process.env };
  delete apple.DEVELOPER_DIR;
  delete apple.SDKROOT;
  $.env(apple);
  const keychains = (await $`/usr/bin/security list-keychains -d user`.text())
    .split("\n")
    .map((line) => line.trim().replace(/^"|"$/g, ""))
    .filter(Boolean);
  try {
    const dir = join(work, "zeaburctl");
    await mkdir(dir);
    await $`tar -xzf ${archive} -C ${dir}`;
    const file = join(dir, "zeaburctl");

    const secret = crypto.randomUUID();
    await $`/usr/bin/security create-keychain -p ${secret} ${keychain}`;
    await $`/usr/bin/security set-keychain-settings -lut 3600 ${keychain}`;
    await $`/usr/bin/security unlock-keychain -p ${secret} ${keychain}`;
    await $`/usr/bin/security list-keychains -d user -s ${keychain} ${keychains}`;
    const p12 = join(work, "certificate.p12");
    await Bun.write(p12, Buffer.from(env("APPLE_CERTIFICATE_P12"), "base64"));
    await $`/usr/bin/security import ${p12} -k ${keychain} -P ${password} -T /usr/bin/codesign`;
    await $`/usr/bin/security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k ${secret} ${keychain}`.quiet();
    const identity = (
      await $`/usr/bin/security find-identity -v -p codesigning ${keychain}`.text()
    )
      .split("\n")
      .find((line) => line.includes('"Developer ID Application: '))
      ?.trim()
      .split(/\s+/)[1];
    if (!identity)
      throw new Failure(
        "the certificate is not a Developer ID Application one",
      );
    // The hardened runtime and a secure timestamp, which notarization requires.
    await $`/usr/bin/codesign --force --options runtime --timestamp --keychain ${keychain} --sign ${identity} ${file}`;
    await $`/usr/bin/codesign --verify --strict --verbose=2 ${file}`;

    // A bare executable cannot carry the ticket; Gatekeeper looks it up online.
    const key = join(work, "key.p8");
    await Bun.write(key, `${env("APPLE_API_KEY_P8")}\n`);
    const zip = join(work, "zeaburctl.zip");
    await $`/usr/bin/ditto -c -k --keepParent ${file} ${zip}`;
    const submitted =
      await $`/usr/bin/xcrun notarytool submit ${zip} --key ${key} ${notary} --wait --timeout 30m --output-format json`.nothrow();
    let result: { status?: string; id?: string } = {};
    try {
      result = JSON.parse(submitted.stdout.toString());
    } catch {}
    if (result.status !== "Accepted") {
      console.error(`notarization: ${submitted.stdout}${submitted.stderr}`);
      if (result.id)
        await $`/usr/bin/xcrun notarytool log ${result.id} --key ${key} ${notary} 1>&2`.nothrow();
      throw new Failure("zeaburctl was not notarized");
    }
    await $`${file} --version`;

    // The archive again, as release-build.sh makes it.
    const epoch = (await $`git show -s --format=%ct HEAD`.text()).trim();
    await $`tar --format=ustar --sort=name --mtime=@${epoch} --owner=0 --group=0 --numeric-owner -C ${dir} -cf - LICENSE zeaburctl | gzip -n -9 > ${archive}`;
  } finally {
    await $`/usr/bin/security list-keychains -d user -s ${keychains}`.nothrow();
    await $`/usr/bin/security delete-keychain ${keychain}`.nothrow().quiet();
  }
}

async function publish(version: Version, artifacts: string, out: string) {
  const plain = version.plain;
  const repository = env("GITHUB_REPOSITORY");
  const owner = repository.split("/")[0]!.toLowerCase();
  const charts = `ghcr.io/${owner}/charts`;
  const { dir, env: login } = await scratch();
  $.env({ ...process.env, ...login });
  await mkdir(out, { recursive: true });

  // Everything that can refuse does so before the first write; recovering from a failed run is a
  // maintainer's decision (docs/internal/releasing.md).
  if (
    (
      await $`gh release view ${version.tag} --repo ${repository}`
        .nothrow()
        .quiet()
    ).exitCode === 0
  )
    fail(
      `release ${version.tag} already exists; inspect it before trying again`,
    );
  const missing = assetSources(version, artifacts)
    .filter(([, from]) => from.includes("/candidate-") && !existsSync(from))
    .map(([file]) => file);
  if (missing.length) fail(`missing the builds of ${missing.join(", ")}`);
  await $`crane auth login ghcr.io --username ${env("GITHUB_ACTOR")} --password-stdin < ${new Response(env("GH_TOKEN"))}`;
  await absent(
    ...images.map(({ name }) => `ghcr.io/${owner}/${name}:${plain}`),
    `${charts}/zeabur:${plain}`,
  );

  const chart = join(dir, "chart");
  await cp(chartDir, chart, { recursive: true });
  await rm(join(out, "digests.txt"), { force: true });
  await rm(join(out, "subjects.json"), { force: true });
  const valuesFile = join(chart, "values.yaml");
  let values = await Bun.file(valuesFile).text();
  const indexes: string[] = [];
  for (const { name, label, tar, values: key } of images) {
    const index = await pushImage(
      out,
      label,
      `ghcr.io/${owner}/${name}:${plain}`,
      {
        "linux/amd64": join(artifacts, "candidate-linux-amd64", tar),
        "linux/arm64": join(artifacts, "candidate-linux-arm64", tar),
      },
    );
    indexes.push(index.toString());
    values = pinDigest(values, key, index.digest);
  }
  await Bun.write(valuesFile, values);
  // The chart runs each image pinned to its index, and no other.
  const pinned = await chartImages(chart);
  if (pinned !== indexes.sort().join(", "))
    fail(`the chart runs ${pinned}, not the images just pushed`);

  // The chart at the version of Chart.yaml (checked equal to the release's by `check`).
  await $`helm lint --strict ${chart} --set publicUrl=https://zeabur.example.com --set issuer.url=https://identity.example.com --set issuer.clientSecret.existingSecret=zeabur-oidc`;
  await $`helm package ${chart} --destination ${out}`;
  const packaged = join(out, `zeabur-${plain}.tgz`);
  await pushChart(out, `oci://${charts}`, packaged);

  // What the installer pins (`finish`).
  await Bun.write(
    join(out, "outputs.env"),
    `ZEABUR_PIN_ZEABUR_IMAGE=${indexes.find((i) => i.startsWith(`ghcr.io/${owner}/zeabur@`))}\nZEABUR_PIN_ZEABUR_CHART_SHA256=${await sha256(packaged)}\n`,
  );
  await $`cat ${join(out, "subjects.json")} ${join(out, "digests.txt")}`;
}

async function finish(
  version: Version,
  os: string,
  arch: string,
  artifacts: string,
  dist: string,
) {
  platform(os, arch);
  // The installer runs on Linux servers only; release.yml finishes on Linux only.
  if (os !== "linux") fail(`no installer for ${os}-${arch}`);
  // KEY=value lines, never evaluated.
  const published: Record<string, string> = {};
  for (const line of (
    await Bun.file(join(artifacts, "published/outputs.env")).text()
  )
    .split("\n")
    .filter(Boolean)) {
    const m = /^(ZEABUR_PIN_[A-Z0-9_]+)=(.*)$/.exec(line);
    if (!m) fail(`unexpected line in outputs.env: ${line}`);
    published[m[1]!] = m[2]!;
  }
  const pinned = { ...(await pins(version)), ...published };
  await $`${scripts}/release-build.sh ${version.tag} ${arch} ${dist} installer`.env(
    {
      ...process.env,
      ...pinned,
    },
  );
}

async function assets(version: Version, artifacts: string, dir: string) {
  const files = assetSources(version, artifacts);
  const missing = files
    .filter(([, from]) => !existsSync(from))
    .map(([, from]) => from);
  if (missing.length) fail(`missing ${missing.join(", ")}`);
  await mkdir(dir, { recursive: true });
  for (const [file, from] of files) await cp(from, join(dir, file));
}

async function verify(
  version: Version,
  os: string,
  arch: string,
  downloaded: string,
) {
  platform(os, arch);
  console.log(`verifying ${version.tag} on ${os}-${arch}`);
  // A fresh, empty registry configuration: a publisher's login must not hide a private package.
  const { dir: work, env: login } = await scratch();
  $.env({ ...process.env, ...login });
  const want = assetSources(version, downloaded)
    .map(([file]) => file)
    .sort();
  const got = (await readdir(downloaded))
    .filter((f) => !f.startsWith("SHA256SUMS"))
    .sort();
  if (got.join(", ") !== want.join(", "))
    fail(`the draft has ${got.join(", ")}, not ${want.join(", ")}`);

  const digests = join(downloaded, "digests.txt");
  const chart = join(downloaded, `zeabur-${version.plain}.tgz`);
  const indexes: string[] = [];
  for (const { label } of images)
    indexes.push(
      (
        await verifyImage(digests, label, ["linux/amd64", "linux/arm64"])
      ).toString(),
    );
  const chartRef = await verifyChart(digests, chart);
  // The chart runs those images and no other.
  const pinned = await chartImages(chart);
  if (pinned !== indexes.sort().join(", "))
    fail(`the chart runs ${pinned}, not the images in digests.txt`);

  // The programs themselves: on Linux, the published chart on a disposable k3s cluster whose node
  // pulls the published image; on macOS, zeaburctl.
  const archive = join(
    downloaded,
    `zeaburctl-${version.tag}-${os}-${arch}.tar.gz`,
  );
  if (os === "linux")
    await $`${scripts}/release-smoke.sh ${archive} oci://${chartRef}`;
  else {
    const file = join(work, "zeaburctl");
    await $`tar -xOzf ${archive} zeaburctl > ${file}`;
    await $`chmod 0755 ${file}`;
    const said = (await $`${file} --version`.text()).trim();
    // Signed with a Developer ID only when the release had the certificate (`sign`); then
    // Gatekeeper must accept it as notarized.
    const signature = await $`/usr/bin/codesign -dvv ${file}`.nothrow().quiet();
    if (
      /^Authority=Developer ID Application: /m.test(signature.stderr.toString())
    ) {
      const assessed =
        await $`/usr/sbin/spctl --assess -vvv --type open --context context:primary-signature ${file}`
          .nothrow()
          .quiet();
      const text = `${assessed.stdout}${assessed.stderr}`;
      if (
        assessed.exitCode !== 0 ||
        !text.includes("source=Notarized Developer ID")
      )
        fail(`zeaburctl is not notarized: ${text}`);
    } else console.log("zeaburctl is not signed with a Developer ID");
    if (said !== `zeaburctl ${version.plain}`) fail(`zeaburctl says '${said}'`);
  }
  console.log(`release ${version.tag} verified on ${os}-${arch}`);
}

const [stage, tag, ...rest] = process.argv.slice(2);
const usage =
  "usage: bun scripts/release.ts bump|check|build|sign|publish|finish|assets|verify <version> ... (see the script's header)";
if (!tag) fail(usage);
const version =
  parseVersion(tag) ??
  fail(`${tag} is not vX.Y.Z or vX.Y.Z-<pre-release> without build metadata`);
const perPlatform = ["build", "sign", "finish", "verify"].includes(stage ?? "");
const args = rest.map((a, i) => (perPlatform && i < 2 ? a : resolve(a)));
try {
  if (stage === "bump" && args.length === 0) await bump(version);
  else if (stage === "check" && args.length === 0) await check(version);
  else if (stage === "build" && args.length === 3)
    await build(version, args[0]!, args[1]!, args[2]!);
  else if (stage === "sign" && args.length === 3)
    await sign(version, args[0]!, args[1]!, args[2]!);
  else if (stage === "publish" && args.length === 2)
    await publish(version, args[0]!, args[1]!);
  else if (stage === "finish" && args.length === 4)
    await finish(version, args[0]!, args[1]!, args[2]!, args[3]!);
  else if (stage === "assets" && args.length === 2)
    await assets(version, args[0]!, args[1]!);
  else if (stage === "verify" && args.length === 3)
    await verify(version, args[0]!, args[1]!, args[2]!);
  else fail(usage);
} catch (e) {
  if (e instanceof Failure) fail(e.message);
  // The command has printed its own error.
  if (e instanceof $.ShellError) process.exit(e.exitCode);
  throw e;
}
