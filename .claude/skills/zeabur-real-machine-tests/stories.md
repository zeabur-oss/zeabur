# Stories

Each story starts from the docs. "Done when" is what the user can do at the end. "Watch" is what
the record must answer. Run them in this order; a story that needs an earlier one's state says so.

## Order

1. **Install:** S1 on the 4 GB server, and in parallel on the 2 GB one.
2. **Deploy:** S9, S10, S3, S2, S4, S5, S17, S18.
3. **Operate:** S6, S7, S8, S12, S13, S14, S20.
4. **Multi-node:** S19 on three servers.
5. **Move from Zeabur (zeabur.com):** S11, then S21 on a fresh server.
6. **Upgrade:** S15 on a fresh server.
7. **Leave:** S16 last on its server.

## Install

**S1 Install with an agent.** A zeabur.com user rents a 4 GB Ubuntu server and pastes the agent
install prompt, with the server's address, into a coding agent.
- Done when: the agent asks before each change, gives an `https://zeabur.<address>.sslip.io` URL and
  the admin link, and the user signs in with a passkey and sees Overview.
- Watch: steps the docs lack, commands guessed, waits with no progress shown. On the 2 GB server:
  does the docs state a minimum memory, does the install finish, is there room for one small app.

## Deploy

**S2 Deploy a Next.js app from Git.** Install the Builder component and the GitHub App, then deploy
a Next.js repository.
- Done when: the build succeeds and the sslip.io URL shows the home page.
- Watch: the GitHub App flow (needs-a-human; use S3 instead), whether a failed build reads clearly.

**S3 Deploy a Python app by uploading a folder.** A user without GitHub uploads a FastAPI folder.
- Done when: the deploy succeeds and `/docs` opens.

**S4 Create a database and connect an app.** PostgreSQL for the S2 or S3 app.
- Done when: the connection variables reach the app and it reads and writes.

**S5 Deploy the n8n template.** Fill the required values.
- Done when: n8n opens, the user creates a workflow, and it is still there after a redeploy.

**S9 Deploy a public repository, no GitHub App.** A small guestbook web page from a public Git
repository.
- Done when: a message can be posted.
- Watch: a public repository needs no GitHub App; the text of the check-changes button and the
  objects it lists; the "replicas 1/1 ready" text on Overview; the domain is HTTPS by default.

**S10 Deploy Ghost from a template.**
- Done when: the Ghost home page opens.
- Watch: the template list filters; the card says what Ghost is for and its CPU and memory; the
  only required field is the owner email; the dialog lists the services it starts; time from Create
  to ready; the quickstart page opens.

**S17 A newcomer's first site.** With S1: the agent installs, the canvas shows the server; deploy
WordPress from the template and connect MySQL.
- Done when: the node shows a URL and turns green.
- Watch: what the template asks for, the wait, how the node, link, URL and green light look.

**S18 A solo developer.** Drag a folder onto the canvas; deploy `api` from Git; import a
`compose.yaml` that adds `web` and `redis`; set a daily backup for PostgreSQL.
- Done when: the compose import wires its variables, the "no backup" warning shows on PostgreSQL
  and goes away, and a backup scheduled a few minutes ahead runs.

## Operate

**S6 Back up and restore.** Add an S3-compatible destination, back up S4's database, delete a row,
restore.
- Done when: the row is back.
- Watch: which destination the docs offer for a single server and what a real user needs for it;
  whether a failed backup shows on screen.

**S7 Invite a member to one project.** Create an invite link; the member joins with their own
passkey.
- Done when: the member sees only the granted project and cannot change cluster settings.

**S8 Bring your own domain.** Point a domain at the server and add it to a service.
- Done when: the certificate is issued. With no public domain, record "cannot test", not fail.

**S12 Day-to-day.** Change a variable, redeploy, roll back, read logs, change replicas.
Auto-deploy on `git push` needs the GitHub App (needs-a-human).

**S13 Reboot the server.** `reboot`, then wait.
- Done when: services, domains and data are back. Record how long it took and what the interface
  showed meanwhile.

**S14 Run out of resources.** On the 4 GB server, deploy until memory runs out; on the 2 GB server,
deploy one template.
- Watch: does the interface say why, or does it stall without a word.

**S20 A platform engineer.** Sign in with a passkey; link a GitHub account and a company account
(an OpenID Connect provider such as Dex on the test server) to one person; a gap on the graph turns
yellow, fix it, review and apply; set a daily database backup.
- Done when: all of that works. Google sign-in is needs-a-human; GitHub sign-in waits for a
  maintainer.

## Multi-node

**S19 A growing team.** Three servers. Load-test `api` and watch traffic on the canvas; `api` turns
red, set CPU and memory, review the change, apply, it turns green; scale replicas from 1/1 to 3/3
across the three servers; deploy a broken version and roll it back.
- Done when: three replicas run on three servers and the rollback restores service.
- Watch: how traffic shows, why a node is red and what the suggested fix is, how long the rollback
  takes.

## Move from Zeabur (zeabur.com)

**S11 Move projects with their data.** Paste the prompt of the from-Zeabur page into an agent that
uses the signed-in Zeabur CLI, and move three projects.
- Done when: each project runs on the server and its data matches. Exported data files are mode 600
  and deleted at the end.

**S21 Move a whole machine.** Move a whole server managed by Zeabur (zeabur.com), with about 15
services using about 2.5 GB, to a fresh server of the same size. The agent follows the move guide
and inventories with `server exec`, not `server ssh-info`.
- Done when: the inventory is complete and the sslip.io check passes. Try the same size first; ask
  the maintainer before a larger server.

## Upgrade

**S15 Upgrade from the previous release.** On a fresh server, install the previous release with
every default, following that release's docs. Make it look used: create the admin with a passkey
and export it; deploy a public image service with an sslip.io domain; create a PostgreSQL with a
row. Then follow the current upgrade docs to the release under test, with no workaround flag the
docs do not give.
- Done when: the same passkey signs in, the service and its domain answer, the row is there, and
  Overview shows no problem the upgrade caused.
- Watch: which version the docs make you pick; what they say when there is no provider snapshot (a
  snapshot is needs-a-human); each wait.

## Leave

**S16 Delete and remove.** Delete a service and a project, then remove Zeabur by the docs.
- Done when: volumes and domains are gone, the server is clean, and an app that was running keeps
  running after Zeabur is gone.
