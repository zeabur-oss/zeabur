// Gates breaking changes to the Zeabur API: oasdiff compares crates/zeabur-api/openapi.yaml with
// the base commit, and a breaking change passes only when a commit since the base carries a
// `BREAKING CHANGE:` footer.
//
// The base is the first parent of HEAD in CI (the pull request's merge commit or the merge queue
// commit), else the merge base with `main`. Run inside `nix develop`, which provides oasdiff.
//
//   bun scripts/api-break.ts [<base>]
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const spec = "crates/zeabur-api/openapi.yaml";
const footer = /^BREAKING[ -]CHANGE: /m;

export type Verdict = { ok: boolean; report: string };

// oasdiff's breaking changes from `base` to `head`, both OpenAPI documents as text; empty when
// there are none. Warnings count: one rule for the whole file.
export async function breakingChanges(
  base: string,
  head: string,
): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "api-break-"));
  try {
    await Bun.write(join(dir, "base.yaml"), base);
    await Bun.write(join(dir, "head.yaml"), head);
    const run = Bun.spawnSync(
      [
        "oasdiff",
        "breaking",
        "base.yaml",
        "head.yaml",
        "--fail-on",
        "WARN",
        "--flatten-allof",
      ],
      { cwd: dir },
    );
    const out = run.stdout.toString() + run.stderr.toString();
    if (run.exitCode === 0) return "";
    if (run.exitCode === 1) return out;
    throw new Error(`oasdiff failed (exit ${run.exitCode}):\n${out}`);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
}

export function verdict(changes: string, messages: string): Verdict {
  if (changes === "")
    return { ok: true, report: `${spec}: no breaking change` };
  if (footer.test(messages)) {
    return {
      ok: true,
      report: `${spec}: breaking change marked with a footer\n${changes}`,
    };
  }
  return {
    ok: false,
    report:
      `${spec} has a breaking change and no commit since the base has a ` +
      `\`BREAKING CHANGE:\` footer. Add one that says what changes for API clients.\n${changes}`,
  };
}

function git(...args: string[]): string {
  const run = Bun.spawnSync(["git", ...args]);
  if (run.exitCode !== 0)
    throw new Error(`git ${args.join(" ")}: ${run.stderr.toString()}`);
  return run.stdout.toString().trim();
}

function base(): string | undefined {
  if (process.argv[2]) return process.argv[2];
  if (process.env.GITHUB_ACTIONS === "true") return "HEAD^1";
  const main = Bun.spawnSync(["git", "rev-parse", "--verify", "-q", "main"]);
  return main.exitCode === 0 ? git("merge-base", "HEAD", "main") : undefined;
}

if (import.meta.main) {
  const from = base();
  if (from === undefined) {
    console.log(`${spec}: no base (no main branch); skipped`);
    process.exit(0);
  }
  const old = Bun.spawnSync(["git", "show", `${from}:${spec}`]);
  if (old.exitCode !== 0) {
    console.log(`${spec}: not at ${from}; skipped`);
    process.exit(0);
  }
  const changes = await breakingChanges(
    old.stdout.toString(),
    await Bun.file(spec).text(),
  );
  const { ok, report } = verdict(
    changes,
    git("log", "--format=%B", `${from}..HEAD`),
  );
  (ok ? console.log : console.error)(report);
  process.exit(ok ? 0 : 1);
}
