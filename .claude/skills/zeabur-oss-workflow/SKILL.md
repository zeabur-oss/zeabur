---
name: zeabur-oss-workflow
description: How work moves through every zeabur-oss repository — taking an issue, milestones, branches, signed-off Conventional Commits, pull requests, waiting for checks, squash merging, releases (the maintainers' steps), issues (which repository, type, labels, body) and the rules for text that lands in a repository or on GitHub. Use when taking an issue, opening a branch, committing, opening, reviewing or merging a pull request, answering Codex, preparing a release, or filing an issue in any zeabur-oss repository.
---

<!-- Edit this in zeabur-oss/skills (skills/zeabur-oss-workflow/SKILL.md); the weekly skills sync overwrites the copy in other repositories. -->

# Zeabur OSS workflow

The repository's own skill (`<repo>-dev`, `<repo>-workflow`) names its checks, labels and release
files. This skill is what every zeabur-oss repository shares.

## Taking an issue

- Claim the issue before you start, so two people don't do the same work. With triage access or
  more, assign it to yourself (`gh issue edit <n> --add-assignee @me`). Without it, comment on
  the issue that you are taking it; a maintainer assigns you.
- Read its "Depends on" links first. Work that waits on an open issue or pull request starts
  after it lands.
- Milestones order the work. `Next` ships before `Later`: a pull request for a `Later` issue
  stays unmerged until `Next` is released. Every pull request carries its issue's milestone. Set
  it yourself if you can (`gh pr edit <n> --milestone <name>`); otherwise name the issue in the
  body ("Closes #n") and a maintainer sets it.
- Fix the cause, not the symptom. A small root fix that breaks an old behaviour is better than a
  workaround around it; say what breaks in the pull request.

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
   `nix develop -c scripts/ci/<job>.sh`, see `zeabur-oss-ci`), push, and open the pull request with
   `gh pr create --base main`. The body: what was wrong or missing (link the failed run or the
   issue), the cause and how it was shown, what changed. Name what was not verified. Write it in
   English. A change people can see carries before and after screenshots in the body; host them
   anywhere the reviewer can open them, such as an image pasted into the pull request.
5. Wait for every check: `gh pr checks <n> --watch` returns once all have finished, non-zero if
   any failed. Without `--watch` it only reports the current state and exits 8 while checks are
   pending, so never merge on its answer alone. A red check is investigated (`zeabur-oss-ci`),
   not re-run until green.
6. Pull requests are squash-merged (`gh pr merge <n> --squash`); no other merge method is used.
   The subject on `main` is the commit's for a single commit and the pull request title
   otherwise, so keep both in Conventional Commits form. With a merge queue, `gh pr merge --squash`
   enqueues the pull request; the slow checks run there on the commit about to land.

## Review

1. Open the pull request as a draft. Review your own diff against `zeabur-oss-standards` and the
   issue's "Done when" before anyone else does.
2. When your review finds nothing and every check is green, mark it ready (`gh pr ready <n>`).
   Codex reviews it then.
3. Fix each Codex finding that holds, or reply why it doesn't, and answer on its thread
   ("Fixed in <sha>."). Before you push more commits, check the pull request is still open
   (`gh pr view <n> --json state`); a maintainer may merge while you work.
4. A maintainer decides when it merges.

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
