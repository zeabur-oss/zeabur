// Release versions and the release tags a release pins (scripts/release.ts). A version is
// vX.Y.Z or vX.Y.Z-<pre-release> (SemVer 2.0.0 §9); build metadata (§10) is refused, as an image
// tag cannot hold its `+`.
import { Failure } from "@zeabur-oss/release";

const number = "(0|[1-9]\\d*)";
const identifier = "(?:0|[1-9]\\d*|\\d*[A-Za-z-][0-9A-Za-z-]*)";
const pattern = new RegExp(
  `^v${number}\\.${number}\\.${number}(?:-(${identifier}(?:\\.${identifier})*))?$`,
);

export type Version = { tag: string; plain: string; prerelease: boolean };

export function parseVersion(tag: string): Version | undefined {
  const m = pattern.exec(tag);
  if (!m) return undefined;
  return { tag, plain: tag.slice(1), prerelease: m[4] !== undefined };
}

// Newest first, by the precedence of SemVer §11 (v0.1.2 after v0.1.2-beta.1), not `sort -V`.
export function newestFirst(versions: Version[]): Version[] {
  return [...versions].sort((a, b) => Bun.semver.order(b.plain, a.plain));
}

// A normal release pins normal releases only; a pre-release may pin pre-releases too.
export function pinnable(release: Version, tag: string): boolean {
  const v = parseVersion(tag);
  return v !== undefined && (!v.prerelease || release.prerelease);
}

// The release tag of a pin, from `git ls-remote --tags` of its repository: the pin itself when
// it is a release tag, else a release tag on the pinned commit. On a commit with several, a
// normal tag wins over a pre-release, then the newest.
export function pinTag(
  repository: string,
  release: Version,
  pin: string,
  lsRemote: string,
): string {
  const refs = lsRemote
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const [commit, ref] = line.split("\t");
      return {
        commit,
        tag: (ref ?? "").replace(/^refs\/tags\//, "").replace(/\^\{\}$/, ""),
      };
    });
  const kind = release.prerelease ? "pre-release" : "normal release";
  if (parseVersion(pin)) {
    if (!pinnable(release, pin))
      throw new Failure(
        `${repository} ${pin} is a pre-release, which a normal release does not pin`,
      );
    if (!refs.some((r) => r.tag === pin))
      throw new Failure(`${repository} has no release tag ${pin}`);
    return pin;
  }
  const tags = [
    ...new Set(refs.filter((r) => r.commit === pin).map((r) => r.tag)),
  ]
    .filter((tag) => pinnable(release, tag))
    .map((tag) => parseVersion(tag) as Version);
  const normal = tags.filter((v) => !v.prerelease);
  const [tag] = newestFirst(normal.length ? normal : tags);
  if (!tag)
    throw new Failure(
      `${repository} ${pin} has no release tag that a ${kind} can pin: tag it, or move the pin`,
    );
  return tag.tag;
}
