---
default: patch
---

# A failed WAL archive shows the bucket it writes to

When a database could not archive its write-ahead log to the backup destination, the database page showed only "exit status 4". It now also shows the bucket and path the database archives to, so a wrong bucket name is easy to spot, and says that the S3 error itself is only in the backup plugin's log. CloudNativePG reports no more than the exit status of the archive command, so Zeabur cannot show the S3 error code.
