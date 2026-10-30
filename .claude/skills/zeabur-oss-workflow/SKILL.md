---
name: zeabur-oss-workflow
description: How work moves through every zeabur-oss repository — branches, signed-off Conventional Commits, pull requests, waiting for checks, squash merging, releases (the maintainers' steps), issues (which repository, type, labels, body) and the rules for text that lands in a repository or on GitHub. Use when opening a branch, committing, opening or merging a pull request, preparing a release, or filing an issue in any zeabur-oss repository.
---

# Zeabur OSS workflow

The repository's own skill (`<repo>-dev`, `<repo>-workflow`) names its checks, labels and release
files. This skill is what every zeabur-oss repository shares.

## Branch, commit, pull request

1. Branch from an up-to-date `main`: `git fetch && git switch -c <type>/<topic> origin/main`
   (`fix/ci-image-check-pipefail`, `docs/…`, `feat/…`). One topic per branch.
2. Commit with `git commit -s`. Every repository uses the
   [Developer Certificate of Origin 1.1](https://developercertificate.org/): the `dco / DCO`
   check rejects a pull request with a commit whose `Signed-off-by` line does not match its
   author. Fix a branch with `git rebase --signoff main`. Use your own name and email; when
   squashing, keep other contributors' attribution and never sign off for someone else.
3. Write messages in [Conventional Commits](https://www.conventionalcommits.org/) style:
   `fix(ci): …`, `test(ui): …`, `docs: …`, `chore(scope): …`, `feat(api)!: …` with a
   `BREAKING CHANGE:` footer. The subject says what changes in plain words; the body says why.
   No tool- or session-specific trailers.
4. Run the checks for what you touched (the repository's dev skill; every CI job is
   `scripts/ci.sh <job>`, see `zeabur-oss-ci`), push, and open the pull request with
   `gh pr create --base main`. The body: what was wrong or missing (link the failed run or the
   issue), the cause and how it was shown, what changed. Name what was not verified.
5. Wait for every check: `gh pr checks <n> --watch` returns once all have finished, non-zero if
   any failed. Without `--watch` it only reports the current state and exits 8 while checks are
   pending, so never merge on its answer alone. A red check is investigated (`zeabur-oss-ci`),
   not re-run until green.
6. Pull requests are squash-merged (`gh pr merge <n> --squash`); no other merge method is used.
   The subject on `main` is the commit's for a single commit and the pull request title
   otherwise, so keep both in Conventional Commits form.

## Releases

A maintainer releases, through the shared release workflow of `zeabur-oss/.github`:

1. Bump the version in every manifest, add its `CHANGELOG.md` section, merge to `main`.
2. Optionally rehearse: run the repository's Release workflow by hand with the version.
3. Push the tag `vX.Y.Z` from `main`. The workflow leaves a draft release.
4. Check the draft and publish it. Releases are immutable once published.

Never push a tag, create, edit or publish a release, dispatch `release.yml`, or change a
repository or package setting yourself. Prepare the pull request and name the step that is the
maintainers'. A failed release run: `zeabur-oss-ci`.

## Issues

- **Where.** Each repository holds the issues of its own code. Zeabur itself:
  `zeabur-oss/zeabur`. A component in the catalogue (a chart, its version, values, detection,
  security notes, its e2e): `zeabur-oss/catalog`. Sign-in, tokens, passkeys:
  `zeabur-oss/identity`. Components, tokens and styles shared by the interfaces:
  `zeabur-oss/design-system`. Shared CI, DCO and release workflows: `zeabur-oss/.github`. When
  work spans repositories, open one issue in each and link them both ways.
- **Type.** Bug, Feature or Task (GitHub issue types; set with
  `gh api -X PATCH repos/<owner>/<repo>/issues/<n> -f type=Feature`).
- **Labels.** `bug` or `enhancement`, plus the repository's area labels. Check the current set
  with `gh label list`.
- **Body.** Today (what happens now, with file paths), Proposal, Open questions, Done when.
  Facts about an upstream project come from its README, chart or source at a named version;
  render a chart (`helm template`) to state its RBAC rather than guessing. Dependencies between
  issues: say "Depends on #n" and set it as blocked by
  (`gh api -X POST repos/<owner>/<repo>/issues/<n>/dependencies/blocked_by -F issue_id=<id>`).

## Text that lands in a repository or on GitHub

- English, plain words, decisions stated as facts about the design, not as history. How text
  reads, in every language: `zeabur-writing`.
- Cite public sources only, and only what you have read. A source you have not read is not
  cited at all; if it matters, note it in the internal docs or an issue.
- No private notes, internal plans or names of people's decisions; no milestone code names.
- A claim that a project does something better or safer has a test behind it.
