---
default: patch
---

# Uploading a folder and deploying from Git work with Zeabur Builder v0.2.4 and later

"Review changes" for **Upload a folder** and **Deploy from Git** no longer fails with `spec.source.type: Required value`. Zeabur now writes and reads the Build source in the shape Zeabur Builder v0.2.4 introduced: `spec.source.type` is `git` or `archive`, the repository's credentials are under `spec.source.git`, and the directory and Dockerfile are on `spec.source`. Builds in the old shape are not read; Zeabur Builder v0.2.4 does not take them either.
