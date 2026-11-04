---
default: minor
---

# Zeabur reads the new Zeabur Catalog format

`zeabur-installer` marks a component untested where the catalog did not test it on the server's distribution and platform, and leaves out a component whose Kubernetes versions do not include the cluster's. Component inputs use the catalog's new names, such as `s3Url`, `fileBackup` and `clusterIp`, in `inputs` of the configuration file. A component or template that needs a release type, Pod Security level or generated value that this build does not know is not offered. The installer renames an old input name in the configuration file when it reads the file, and keeps its comments, order and mode.
