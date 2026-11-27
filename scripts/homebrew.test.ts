import { describe, expect, test } from "bun:test";
import { mkdtemp, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { cask, formula, write } from "./homebrew";
import { parseVersion, type Version } from "./versions";

const v = parseVersion("v0.3.0") as Version;
const base = "https://github.com/zeabur-oss/zeabur/releases/download/v0.3.0";
const sum = (c: string) => c.repeat(64);
const checksums = new Map([
  ["zeaburctl-v0.3.0-darwin-arm64.tar.gz", sum("a")],
  ["zeaburctl-v0.3.0-linux-amd64.tar.gz", sum("b")],
  ["zeaburctl-v0.3.0-linux-arm64.tar.gz", sum("c")],
  ["zeaburctl-v0.3.0-darwin-amd64.tar.gz", sum("f")],
  ["zeabur-desktop-v0.3.0-darwin-arm64.dmg", sum("d")],
  ["zeabur-desktop-v0.3.0-darwin-arm64.app.tar.gz", sum("e")],
]);

describe("formula", () => {
  const rb = formula(v, checksums);

  test("installs each platform's archive with the checksum SHA256SUMS lists for it", () => {
    for (const [platform, c] of [
      ["darwin-arm64", "a"],
      ["darwin-amd64", "f"],
      ["linux-amd64", "b"],
      ["linux-arm64", "c"],
    ])
      expect(rb).toMatch(
        new RegExp(
          `url "${base}/zeaburctl-v0\\.3\\.0-${platform}\\.tar\\.gz"\\n\\s*sha256 "${sum(c)}"`,
        ),
      );
  });

  test("runs on Intel Macs too, once the release has their build", () => {
    expect(rb).not.toContain("depends_on arch: :arm64");
    const older = new Map(checksums);
    older.delete("zeaburctl-v0.3.0-darwin-amd64.tar.gz");
    const old = formula(v, older);
    expect(old).toContain("depends_on arch: :arm64");
    expect(old).not.toContain("darwin-amd64");
  });

  test("refuses a release whose SHA256SUMS lacks an archive", () => {
    const partial = new Map(checksums);
    partial.delete("zeaburctl-v0.3.0-linux-arm64.tar.gz");
    expect(() => formula(v, partial)).toThrow(
      "zeaburctl-v0.3.0-linux-arm64.tar.gz",
    );
  });
});

describe("cask", () => {
  test("installs the .dmg with its checksum", () => {
    const rb = cask(v, checksums) ?? "";
    expect(rb).toContain('version "0.3.0"');
    expect(rb).toContain(`sha256 "${sum("d")}"`);
    expect(rb).toContain(
      'url "https://github.com/zeabur-oss/zeabur/releases/download/v#{version}/zeabur-desktop-v#{version}-darwin-arm64.dmg"',
    );
  });

  test("is left out for a release without the macOS app", () => {
    const old = new Map(
      [...checksums].filter(([file]) => !file.startsWith("zeabur-desktop")),
    );
    expect(cask(v, old)).toBeUndefined();
  });
});

const pre = parseVersion("v0.3.0-beta.1") as Version;
const preChecksums = new Map(
  [...checksums].map(([file, c]) => [file.replace("v0.3.0", pre.tag), c]),
);

describe("a normal release", () => {
  test("names the formula and the cask zeaburctl and zeabur-desktop", () => {
    expect(formula(v, checksums)).toContain("class Zeaburctl < Formula");
    expect(cask(v, checksums)).toContain('cask "zeabur-desktop" do');
  });

  test("conflicts with the beta formula and cask", () => {
    expect(formula(v, checksums)).toContain(
      'conflicts_with "zeabur-oss/tap/zeaburctl-beta"',
    );
    expect(cask(v, checksums)).toContain(
      'conflicts_with cask: "zeabur-desktop@beta"',
    );
  });
});

describe("a pre-release", () => {
  test("names the formula zeaburctl-beta, as Homebrew cannot load a formula named zeaburctl@beta", () => {
    expect(formula(pre, preChecksums)).toContain(
      "class ZeaburctlBeta < Formula",
    );
  });

  test("names the cask with Homebrew's @beta channel token", () => {
    expect(cask(pre, preChecksums)).toContain('cask "zeabur-desktop@beta" do');
  });

  test("conflicts with the normal formula and cask, which install the same files", () => {
    expect(formula(pre, preChecksums)).toContain(
      'conflicts_with "zeabur-oss/tap/zeaburctl"',
    );
    expect(cask(pre, preChecksums)).toContain(
      'conflicts_with cask: "zeabur-desktop"',
    );
  });

  test("installs the pre-release's files", () => {
    expect(formula(pre, preChecksums)).toContain(
      'url "https://github.com/zeabur-oss/zeabur/releases/download/v0.3.0-beta.1/zeaburctl-v0.3.0-beta.1-darwin-arm64.tar.gz"',
    );
    expect(cask(pre, preChecksums)).toContain('version "0.3.0-beta.1"');
  });
});

describe("write", () => {
  test("puts the formula under Formula and the cask under Casks", async () => {
    const out = await mkdtemp(join(tmpdir(), "homebrew-"));
    await write(v, checksums, out);
    expect(await readdir(join(out, "Formula"))).toEqual(["zeaburctl.rb"]);
    expect(await readdir(join(out, "Casks"))).toEqual(["zeabur-desktop.rb"]);
  });

  test("puts a pre-release under the beta names, beside the normal release's files", async () => {
    const out = await mkdtemp(join(tmpdir(), "homebrew-"));
    await write(v, checksums, out);
    await write(pre, preChecksums, out);
    expect((await readdir(join(out, "Formula"))).sort()).toEqual([
      "zeaburctl-beta.rb",
      "zeaburctl.rb",
    ]);
    expect((await readdir(join(out, "Casks"))).sort()).toEqual([
      "zeabur-desktop.rb",
      "zeabur-desktop@beta.rb",
    ]);
    expect(await Bun.file(join(out, "Formula/zeaburctl.rb")).text()).toBe(
      formula(v, checksums),
    );
    expect(await Bun.file(join(out, "Casks/zeabur-desktop.rb")).text()).toBe(
      cask(v, checksums) ?? "",
    );
  });

  test("writes no cask for a release without the macOS app", async () => {
    const out = await mkdtemp(join(tmpdir(), "homebrew-"));
    const old = new Map(
      [...checksums].filter(([file]) => !file.startsWith("zeabur-desktop")),
    );
    await write(v, old, out);
    expect(await readdir(out)).toEqual(["Formula"]);
  });
});
