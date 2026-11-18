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
  ["zeabur-desktop-v0.3.0-darwin-arm64.dmg", sum("d")],
  ["zeabur-desktop-v0.3.0-darwin-arm64.app.tar.gz", sum("e")],
]);

describe("formula", () => {
  const rb = formula(v, checksums);

  test("installs each platform's archive with the checksum SHA256SUMS lists for it", () => {
    for (const [platform, c] of [
      ["darwin-arm64", "a"],
      ["linux-amd64", "b"],
      ["linux-arm64", "c"],
    ])
      expect(rb).toMatch(
        new RegExp(
          `url "${base}/zeaburctl-v0\\.3\\.0-${platform}\\.tar\\.gz"\\n\\s*sha256 "${sum(c)}"`,
        ),
      );
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

describe("write", () => {
  test("puts the formula under Formula and the cask under Casks", async () => {
    const out = await mkdtemp(join(tmpdir(), "homebrew-"));
    await write(v, checksums, out);
    expect(await readdir(join(out, "Formula"))).toEqual(["zeaburctl.rb"]);
    expect(await readdir(join(out, "Casks"))).toEqual(["zeabur-desktop.rb"]);
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
