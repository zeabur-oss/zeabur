---
default: minor
---

# Browse and change a bucket's files

An object store's page has a **Files** tab for its buckets: open folders, preview text and images, download, upload (larger files in parts), make a folder, rename a file and delete a file or a folder. Zeabur reads and writes through the store's S3 gateway with the store's keys, which stay on the server. Browsing needs the right to read the keys' Secret; changes also need the right to change the bucket. The API has the same operations under `/api/v1/projects/{project}/object-storage/{store}/buckets/{bucket}/`.
