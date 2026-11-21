---
default: patch
---

# Zeabur Desktop checks for updates again

Check for updates failed with "Could not fetch a valid release JSON from the remote", because the app asked for the newest release and GitHub's newest release skips pre-releases. Zeabur Desktop now looks for the newest release that has an update: a release build takes newer releases, and a pre-release build takes newer pre-releases too. The sidebar shows the version at its foot with **Check for updates**, says when the app is up to date, and shows a short message with **Try again** when the check fails.
