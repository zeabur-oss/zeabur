---
name: zeabur-workflow
description: How work moves through the Zeabur repository — branches, commits, pull requests, merging, releases and issues (types, labels, which Zeabur repository an issue belongs in) — and the rules for text that lands in the repository or on GitHub. Use when opening a branch, committing, opening or merging a pull request, or filing an issue.
---

# Workflow

What is Zeabur's own about branches, commits, pull requests and issues. `CONTRIBUTING.md` has the
rules.

## Pull requests

- Run the checks for what you touched (`zeabur-dev`, `zeabur-ui`). A red check: `zeabur-ci`.
- A change to the plan or HTTP API shape (a field added, renamed or removed) needs the
  maintainers' agreement on the shape before it is built; propose it in an issue first.

## Releases

A maintainer releases: they run the Release pull request workflow (knope sets the versions that
`knope.toml` lists and writes the `CHANGELOG.md` section), merge its pull request, which tags it,
check the draft release the workflow leaves and publish it by hand (`docs/internal/releasing.md`). A change to a `scripts/release-*.sh` is
checked with `shellcheck`, to `scripts/*.ts` with `bun check scripts/` and `bun test scripts/`, to
the workflow with `jactionlint` and a manual run of the Release workflow, which rehearses, and on
Linux by running `release-build.sh` and `release-smoke.sh`; the macOS build runs on a Mac. A failed release run: `zeabur-ci`.

## Issues

- Zeabur embeds Zeabur Catalog's release bundle at the commit in `flake.lock`
  (`nix flake update catalog` moves it), so a catalogue issue goes to `zeabur-oss/catalog` even
  when it shows up in Zeabur.
- **Labels.** `bug` or `enhancement`, plus the areas it touches: `area/ui`, `area/api`,
  `area/graph`, `area/diagnosis`, `area/cli`, `area/ci`; `performance`, `i18n`, `accessibility`,
  `documentation`, `dependencies` where they apply.
