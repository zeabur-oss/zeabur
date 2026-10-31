import { Failure } from "@zeabur-oss/release";
import { describe, expect, test } from "bun:test";
import { newestFirst, parseVersion, pinTag, type Version } from "./versions";

const v = (tag: string) => parseVersion(tag) as Version;
const commit = "a".repeat(40);
const other = "b".repeat(40);
// `git ls-remote --tags`: an annotated tag is listed twice, the second time peeled to its commit.
const remote = (...lines: [string, string][]) =>
  lines.map(([c, ref]) => `${c}\trefs/tags/${ref}`).join("\n") + "\n";

describe("parseVersion", () => {
  test("takes a normal version and a pre-release", () => {
    expect(parseVersion("v0.1.2")).toEqual({
      tag: "v0.1.2",
      plain: "0.1.2",
      prerelease: false,
    });
    expect(parseVersion("v0.1.2-beta.1")).toEqual({
      tag: "v0.1.2-beta.1",
      plain: "0.1.2-beta.1",
      prerelease: true,
    });
    expect(parseVersion("v1.0.0-rc.1.x-y")?.prerelease).toBe(true);
  });

  test("refuses build metadata, a missing v and what SemVer refuses", () => {
    for (const tag of [
      "v0.1.2+build.1",
      "v0.1.2-beta.1+build.1",
      "0.1.2",
      "v0.1",
      "v01.1.2",
      "v0.1.2-",
      "v0.1.2-beta..1",
      "v0.1.2-01",
      "v0.1.2-beta_1",
    ])
      expect(parseVersion(tag)).toBeUndefined();
  });
});

describe("newestFirst", () => {
  test("orders by SemVer precedence, a normal version after its pre-releases", () => {
    const tags = [
      "v0.1.2-beta.2",
      "v0.1.10",
      "v0.1.2",
      "v0.1.2-beta.10",
      "v0.1.2-alpha",
      "v0.1.9",
    ];
    expect(newestFirst(tags.map(v)).map((x) => x.tag)).toEqual([
      "v0.1.10",
      "v0.1.9",
      "v0.1.2",
      "v0.1.2-beta.10",
      "v0.1.2-beta.2",
      "v0.1.2-alpha",
    ]);
  });
});

describe("pinTag", () => {
  const both = remote(
    [other, "v0.1.2-beta.1"],
    [commit, "v0.1.2-beta.1^{}"],
    [commit, "v0.1.2"],
  );

  test("a commit tagged a pre-release and its normal version resolves to the normal tag", () => {
    expect(pinTag("catalog", v("v0.2.0"), commit, both)).toBe("v0.1.2");
    expect(pinTag("catalog", v("v0.2.0-beta.1"), commit, both)).toBe("v0.1.2");
  });

  test("a normal release pins only normal tags", () => {
    const pre = remote([commit, "v0.1.3-beta.1"]);
    expect(() => pinTag("catalog", v("v0.2.0"), commit, pre)).toThrow(Failure);
    expect(() => pinTag("identity", v("v0.2.0"), "v0.1.3-beta.1", pre)).toThrow(
      "a pre-release, which a normal release does not pin",
    );
  });

  test("a pre-release may pin a pre-release", () => {
    const pre = remote([commit, "v0.1.3-beta.1"]);
    expect(pinTag("catalog", v("v0.2.0-rc.1"), commit, pre)).toBe(
      "v0.1.3-beta.1",
    );
    expect(pinTag("identity", v("v0.2.0-rc.1"), "v0.1.3-beta.1", pre)).toBe(
      "v0.1.3-beta.1",
    );
  });

  test("of several tags on a commit, the newest by SemVer precedence", () => {
    const pres = remote([commit, "v0.1.3-beta.2"], [commit, "v0.1.3-beta.10"]);
    expect(pinTag("catalog", v("v0.2.0-rc.1"), commit, pres)).toBe(
      "v0.1.3-beta.10",
    );
    const normals = remote([commit, "v0.1.9"], [commit, "v0.1.10"]);
    expect(pinTag("catalog", v("v0.2.0"), commit, normals)).toBe("v0.1.10");
  });

  test("a normal tag on the commit wins over a newer pre-release", () => {
    const mixed = remote([commit, "v0.1.2"], [commit, "v0.1.3-beta.1"]);
    expect(pinTag("catalog", v("v0.2.0-rc.1"), commit, mixed)).toBe("v0.1.2");
  });

  test("refuses a commit with no release tag and a tag that does not exist", () => {
    const tags = remote([commit, "nightly"], [other, "v0.1.1"]);
    expect(() => pinTag("catalog", v("v0.2.0"), commit, tags)).toThrow(
      "has no release tag",
    );
    expect(() => pinTag("identity", v("v0.2.0"), "v0.1.2", tags)).toThrow(
      "has no release tag v0.1.2",
    );
  });
});
