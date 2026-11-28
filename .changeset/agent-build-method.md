---
default: minor
---

# Build a repository without a Dockerfile with Zeabur Builder's agent

When the cluster's Zeabur Builder has its agent set up, **Build with** offers **Agent**: a model reads the repository or folder and writes a Dockerfile, and later builds reuse it. The **Builds** tab shows the newest plan (where the Dockerfile came from, the language, port and variables the model found, the model tokens it used, or why it stopped) and **Plan again on every build**, which has each build ask the model again.

An agent build of an uploaded folder with no Dockerfile starts with the **Strict** security profile, because Zeabur Builder v0.3.1 refuses a Dockerfile the model writes if its image runs as root. A folder with its own Dockerfile starts at **Compatible**, because the agent builds it as it is, and so do a Git repository and a `.tar.gz`, whose files the form cannot see. With Zeabur Builder v0.3.0, the model's image may run as root and not start under Strict: upgrade Zeabur Builder to v0.3.1 (Cluster → Components installs it), or choose **Compatible**. The pinned Zeabur Builder is now v0.3.1.
