---
default: patch
---

# Zeabur Desktop checks for updates again

Check for updates failed with "Could not fetch a valid release JSON from the remote", because the app asked for the newest release and GitHub's newest release skips pre-releases. Zeabur Desktop now looks for the newest release that has an update: a release build takes newer releases, and a pre-release build takes newer pre-releases too. **Check for updates** moves into a small menu beside the language picker, with the version. The answer comes as a notification: up to date, or a short message with **Try again**. A newer version shows an **Update to** button in the sidebar, and its dialog links to the release notes on GitHub.
