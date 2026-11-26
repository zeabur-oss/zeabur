---
default: patch
---

# Connect Git to an existing service, and stop building from source

Settings → Build now connects a Git repository to a service that runs an image or an uploaded folder, with the same choices as a new Git service. A service that builds from source can also stop building: Zeabur deletes its Build and the credentials only it reads, and pins the service to the image it runs, so it keeps running.

API clients write every Build with one `build` object: `createBuiltService` replaces `createUploadService` and `createGitService`, `setBuild` gives a running service a Build or changes its source, and `removeBuild` stops building a service.
