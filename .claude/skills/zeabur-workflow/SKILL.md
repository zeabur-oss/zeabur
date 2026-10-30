---
name: zeabur-workflow
description: How work moves through the Zeabur repository — branches, commits, pull requests, merging, releases and issues (types, labels, which Zeabur repository an issue belongs in) — and the rules for text that lands in the repository or on GitHub. Use when opening a branch, committing, opening or merging a pull request, or filing an issue.
---

# Workflow

Follow `zeabur-oss-workflow` (install: `npx skills add zeabur-oss/skills --skill
zeabur-oss-workflow`; `skills-lock.json` pins it): branches, signed-off Conventional Commits,
pull requests, `gh pr checks --watch`, squash merging, issues and the text rules. This skill adds
what is Zeabur's own.

## Pull requests

- Run the checks for what you touched (`zeabur-dev`, `zeabur-ui`). A red check: `zeabur-ci`.
- A change to the plan or HTTP API shape (a field added, renamed or removed) needs the
  maintainers' agreement on the shape before it is built; propose it in an issue first.

## Releases

A maintainer releases: they bump the version (`Cargo.toml`, `Chart.yaml`, a `CHANGELOG.md`
section), merge it, push the tag `v<version>`, check the draft release the workflow leaves and
publish it by hand (`docs/internal/releasing.md`). A change to `scripts/release.sh` or a
`scripts/release-*.sh` is checked with `shellcheck` and `actionlint`, and on Linux by running
`release-build.sh` and
`release-smoke.sh`; the macOS build runs on a Mac. A failed release run: `zeabur-ci`.

## Issues

- Zeabur embeds Zeabur Catalog's release bundle at the commit in `flake.lock`
  (`nix flake update catalog` moves it), so a catalogue issue goes to `zeabur-oss/catalog` even
  when it shows up in Zeabur.
- **Labels.** `bug` or `enhancement`, plus the areas it touches: `area/ui`, `area/api`,
  `area/graph`, `area/diagnosis`, `area/cli`, `area/ci`; `performance`, `i18n`, `accessibility`,
  `documentation`, `dependencies` where they apply.
