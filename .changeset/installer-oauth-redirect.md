---
default: patch
---

# `zeabur-installer apply` moves the redirect URI of Zeabur's OAuth client

It follows a changed `access.zeabur`, as it already did the home URL. It kept the first host's, so signing in to Zeabur at the new host failed with "This app's return address isn't registered".
