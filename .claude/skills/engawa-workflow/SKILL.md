---
name: engawa-workflow
description: How work moves through the engawa repository — branches, commits, pull requests, merging, and issues (types, labels, which engawahq repository an issue belongs in) — and the rules for text that lands in the repository or on GitHub. Use when opening a branch, committing, opening or merging a pull request, or filing an issue.
---

# Workflow

## Branch, commit, pull request
1. Branch from an up-to-date `main`: `git fetch && git switch -c <type>/<topic> origin/main`
   (`fix/ci-image-check-pipefail`, `docs/…`, `feat/…`). One topic per branch.
2. Commit with `git commit -s` (DCO; the `dco` job checks the `Signed-off-by` line) in
   [Conventional Commits](https://www.conventionalcommits.org/) style: `fix(ci): …`,
   `test(ui): …`, `feat(api)!: …` with a `BREAKING CHANGE:` footer. The subject says what changes
   in plain words; the body says why. No tool- or session-specific trailers.
3. Run the checks for what you touched (`engawa-dev`, `engawa-ui`), push, and open the pull
   request with `gh pr create --base main`. The body: what was wrong or missing (link the failed
   run or the issue), the cause and how it was shown, what changed. Name what was not verified.
4. Wait for every check: `gh pr checks <n> --watch` returns once all have finished, non-zero if
   any failed. Without `--watch` it only reports the current state and exits 8 while checks are
   pending, so never merge on its answer alone. A red check is investigated (`engawa-ci`), not
   re-run until green.
5. Pull requests are squash-merged (`gh pr merge <n> --squash`). The subject on main is the
   commit's for a single commit and the pull request title otherwise, so keep both
   Conventional Commits.

A change to the plan or HTTP API shape (a field added, renamed or removed) needs the maintainers'
agreement on the shape before it is built; propose it in an issue first.

## Releases
A maintainer releases: they bump the version (`Cargo.toml`, `Chart.yaml`, a `CHANGELOG.md`
section), merge it, push the tag `v<version>`, check the draft release the workflow leaves and
publish it by hand (`docs/releases.md`). Never push a tag, create, edit or publish a release,
dispatch `release.yml` or change a repository or package setting yourself; prepare the pull
request and name the step that is theirs. A change to a `scripts/release-*.sh` is checked with
`shellcheck` and `actionlint`, and on Linux by running `release-build.sh` and
`release-smoke.sh`; the macOS build runs on a Mac. A failed release run: `engawa-ci`.

## Issues
- **Where.** engawa itself: `engawahq/engawa`. A component in the catalogue (a chart, its
  version, values, detection, security notes, its e2e): `engawahq/recommendations`; engawa
  vendors its signed bundle (`crates/engawa-recommend/bundle`, pin in `PINNED`) and moves the pin
  after a release. Sign-in, tokens, passkeys: `engawahq/genkan`. When work spans repositories,
  open one issue in each and link them both ways.
- **Type.** Bug, Feature or Task (GitHub issue types; set with
  `gh api -X PATCH repos/<owner>/<repo>/issues/<n> -f type=Feature`).
- **Labels.** `bug` or `enhancement`, plus the areas it touches: `area/ui`, `area/api`,
  `area/graph`, `area/diagnosis`, `area/cli`, `area/ci`; `performance`, `i18n`, `accessibility`,
  `documentation`, `dependencies` where they apply. Check the current set with `gh label list`.
- **Body.** Today (what happens now, with file paths), Proposal, Open questions, Done when.
  Facts about an upstream project come from its README, chart or source at a named version;
  render a chart (`helm template`) to state its RBAC rather than guessing. Dependencies between
  issues: say "Depends on #n" and set it as blocked by
  (`gh api -X POST repos/<owner>/<repo>/issues/<n>/dependencies/blocked_by -F issue_id=<id>`).

## Text that lands in the repository or on GitHub
- English, plain words, decisions stated as facts about the design, not as history.
- Cite public sources only, and only what you have read; otherwise write "not verified".
- No private notes, internal plans or names of people's decisions; no milestone code names.
- A claim that engawa does something better or safer has a test behind it.
