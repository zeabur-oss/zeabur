---
name: zeabur-real-machine-tests
description: Run Zeabur OSS's user-story tests on real, disposable servers, starting only from the published docs, and turn what goes wrong into ranked issues and a report. Use before a release, or after a change to the install, upgrade or deploy flows.
---

# Real-machine tests

The cluster tests (`zeabur-cluster-tests`) prove the code on a throwaway cluster. These tests prove
the docs and the product together: an agent and a user, on a real server, with nothing but
oss.zeabur.com. The stories are in [`stories.md`](stories.md). Issues, commits and public text follow
`zeabur-oss-workflow` and `zeabur-writing`.

## Machines

A maintainer buys disposable Ubuntu servers and gives you their addresses and how to log in. The
stories need a 4 GB server, a 2 GB server, and three servers for multi-node (S19). A story that
starts "on a fresh server" starts on a rebuilt one.

Ask the maintainer, and wait for a yes, before you buy, delete or rebuild a server, or take or
restore a provider snapshot (a person does that in the provider's panel: needs-a-human). Never
touch a server the stories do not assign you. Say which servers are left running when you finish.

## Rules of the run

- **Docs only.** Start from the page "Let an agent install for you"
  (`https://oss.zeabur.com/docs/zeabur/install/agent.md`) and follow the `.md` copies of the pages
  it links. Never read source code to find a step. When the docs lack a step, record it, then find
  the smallest way through and note how. For the upgrade test, use the docs of the release you
  start from, then the current docs.
- **Both roles.** Play the coding agent and the user. As the agent, ask the user before each
  change to a server, as the docs' prompt tells the agent to; as the user, answer, and note every
  place where the docs ask the user something.
- **The admin setup link belongs to the user.** The agent never opens it. The user stand-in opens
  it in Playwright Chromium with a virtual authenticator (CDP `WebAuthn`), as
  `web/e2e/full/identity.ts` does; run it from `nix develop`. Export the credential
  (`WebAuthn.getCredentials`) to sign in again later or after an upgrade. A second authenticator is
  the invited member.
- **Needs-a-human.** A step only a person can do (GitHub sign-in or App, payment, DNS of a real
  domain, a provider snapshot, a Google sign-in) is marked needs-a-human, and you go on another
  way the docs offer (upload a folder, a public image), naming it.
- **Secrets stay out of every output.** Setup links, tokens, passwords, kubeconfigs, private keys
  and passkey exports are never printed, logged or written into the record or an issue. Write
  `<redacted>`. Keep exported files mode 600 and delete them when the run ends.
- **Reading zeabur.com is read-only.** The stories that move from Zeabur (zeabur.com) do not stop
  or delete a service and do not change DNS. Stop at the sslip.io check.
- **Version.** The install pages write `<version>`. Record which version the docs alone would
  make you pick and why, then install the one the maintainer names.

## What to record

One section per story, written as you go, in a Markdown file outside the repository.

- **Result:** pass, fail or needs-a-human.
- **Steps**, each with the doc page it came from.
- **Stuck, guessed, errors:** where you got stuck, what you had to guess, each error verbatim.
- **Waits:** wall-clock time of each wait (install, first build, certificate, backup, restore,
  reboot recovery).
- **Screenshots** of each story's end state, with one at 390 px width. Redact secrets in them.
- **For a bug:** the exact command, its output, and `kubectl get`, `describe` and `events` of the
  objects involved (on K3s, `k3s kubectl`), enough for someone else to reproduce it.

A story is done when its "Done when" holds and the section has every item above.

## After the run

1. **Rank** each UX problem. Blocker: the user cannot finish. Major: the user likely gets lost or
   needs help. Minor: friction. Give each a one-line fix.
2. **File one issue per fixable problem**, in the repository that owns the code
   (`zeabur-oss-workflow`, Issues), after searching for an open one. Issue text is public: no
   addresses, keys or secrets. Link the story.
3. **Write the report** for the maintainer: a result per story, the ranked problems with their
   issues, the waits, what you guessed, the state of each server and what only the maintainer can
   do next (delete servers, release steps). Done when every record section is covered.
