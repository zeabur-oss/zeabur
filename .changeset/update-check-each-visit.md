---
default: patch
---

# Zeabur checks for a newer version each time a page opens, and zeaburctl no longer misses it

The cluster page used to ask GitHub from each browser once a day, and a failed request stopped the check for the rest of that day. Now the Zeabur server asks GitHub at most once an hour and keeps the answer, and each page reads it when it opens and hourly while it stays open. People behind one office address no longer share GitHub's limit of 60 requests an hour, and browsers no longer contact GitHub.

`zeaburctl` dropped the answer when a command ended before GitHub replied, and then waited a day to ask again, so quick commands never showed a notice. It now asks on every command and waits at most a second after the command for the answer. When GitHub's limit for the address is used up, it says to set `GH_TOKEN` or `GITHUB_TOKEN`, and sends that token when set.

**Settings** › **About** shows Zeabur's version, and **How to upgrade** there and in the cluster page's notice opens the steps for how Zeabur was installed, with the commands to copy. The new [Upgrade Zeabur](https://oss.zeabur.com/docs/zeabur/upgrade/steps/) page covers zeabur-installer, Helm, Argo CD, Flux and `zeaburctl ui`.
