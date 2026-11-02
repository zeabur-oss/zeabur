// Writes the catalog-derived responses of the dev API: the Rust planner's output for the pinned
// catalog, from the seeds in fixtures/ (the_web_fixtures_follow_the_catalog in src/gui.rs).
import { execFileSync } from "node:child_process"
import { mkdirSync } from "node:fs"
import { resolve } from "node:path"

export function writeFixtures(dir: string) {
  mkdirSync(dir, { recursive: true })
  execFileSync(
    "cargo",
    ["nextest", "run", "--locked", "-p", "zeabur-installer", "the_web_fixtures_follow_the_catalog"],
    {
      cwd: resolve(import.meta.dirname, ".."),
      env: { ...process.env, ZEABUR_WEB_FIXTURES: dir },
      stdio: "inherit",
    },
  )
}
