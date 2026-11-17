import { describe, expect, test } from "bun:test";
import { desktopAssets, manifest } from "./desktop-release";
import { parseVersion, type Version } from "./versions";

const v = (tag: string) => parseVersion(tag) as Version;
const date = new Date("2026-11-13T08:30:00.123Z");

describe("manifest", () => {
  const made = manifest(
    v("v0.3.0"),
    "zeabur-oss/zeabur",
    "## 0.3.0\n\nNotes.\n",
    date,
    (file) => `sig of ${file}\n`,
  );

  test("gives the plain version, the notes and an RFC 3339 date", () => {
    expect(made.version).toBe("0.3.0");
    expect(made.notes).toBe("## 0.3.0\n\nNotes.");
    expect(made.pub_date).toBe("2026-11-13T08:30:00Z");
  });

  test("points each platform at the file its updater installs, with that file's signature", () => {
    const base =
      "https://github.com/zeabur-oss/zeabur/releases/download/v0.3.0";
    expect(made.platforms["darwin-aarch64-app"]).toEqual({
      url: `${base}/zeabur-desktop-v0.3.0-darwin-arm64.app.tar.gz`,
      signature: "sig of zeabur-desktop-v0.3.0-darwin-arm64.app.tar.gz",
    });
    expect(made.platforms["windows-x86_64-nsis"]?.url).toBe(
      `${base}/zeabur-desktop-v0.3.0-windows-amd64-setup.exe`,
    );
    expect(made.platforms["linux-x86_64-appimage"]?.url).toBe(
      `${base}/zeabur-desktop-v0.3.0-linux-amd64.AppImage`,
    );
    expect(made.platforms["linux-aarch64-appimage"]?.url).toBe(
      `${base}/zeabur-desktop-v0.3.0-linux-arm64.AppImage`,
    );
  });

  test("lists each build without its installer too, for a .deb, .rpm or portable copy", () => {
    for (const [platform, installer] of [
      ["darwin-aarch64", "app"],
      ["windows-x86_64", "nsis"],
      ["linux-x86_64", "appimage"],
      ["linux-aarch64", "appimage"],
    ])
      expect(made.platforms[platform!]).toEqual(
        made.platforms[`${platform}-${installer}`]!,
      );
  });

  test("never points at a package the updater cannot install", () => {
    for (const { url } of Object.values(made.platforms))
      expect(url).toMatch(/(\.app\.tar\.gz|\.AppImage|-setup\.exe)$/);
  });

  test("keeps a pre-release's full version", () => {
    expect(manifest(v("v0.3.0-rc.1"), "o/r", "", date, () => "s").version).toBe(
      "0.3.0-rc.1",
    );
  });

  test("refuses a build without a signature", () => {
    expect(() =>
      manifest(v("v0.3.0"), "o/r", "", date, (file) =>
        file.endsWith(".AppImage") ? "" : "s",
      ),
    ).toThrow("signature");
  });
});

describe("desktopAssets", () => {
  test("names every file once, each from its build's artifact", () => {
    const assets = desktopAssets(v("v0.3.0"), true);
    const files = assets.map(([file]) => file);
    expect(new Set(files).size).toBe(files.length);
    expect(assets).toContainEqual([
      "zeabur-desktop-v0.3.0-darwin-arm64.dmg",
      "desktop-darwin-arm64",
    ]);
    expect(assets).toContainEqual([
      "zeabur-desktop-v0.3.0-windows-amd64-portable.exe",
      "desktop-windows-amd64",
    ]);
    expect(assets).toContainEqual([
      "zeabur-desktop-v0.3.0-linux-arm64.deb",
      "desktop-linux-arm64",
    ]);
    expect(assets).toContainEqual([
      "zeabur-desktop-v0.3.0-linux-amd64.rpm",
      "desktop-linux-amd64",
    ]);
    expect(assets).toContainEqual(["latest.json", "desktop-updater"]);
  });

  test("leaves out latest.json while the app has no updater key", () => {
    expect(
      desktopAssets(v("v0.3.0"), false).map(([file]) => file),
    ).not.toContain("latest.json");
  });
});
