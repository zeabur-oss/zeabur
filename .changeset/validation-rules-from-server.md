---
default: major
---

# Forms check names and limits by the server's rules

The OpenAPI description now carries the length limits, patterns and defaults the server checks: project, task, volume, token, notification, S3 destination and uploaded service names, build paths, bucket names, kept task runs and the backup schedule. New schemas `ProjectName`, `DatabaseName` and `ServiceReplicas` name rules no request field carries. The web reads all of them from the generated rules instead of its own copies, so a bucket name may now have dots everywhere. A backup schedule may leave out `cron` and `keepDays` for daily at 03:00 UTC, kept 7 days. A plan that creates objects is now checked by a dry run, so what Kubernetes refuses in a built object, such as a bad name, is refused at review, and a plan refuses an object whose `zeabur.com/database` label is not a database name. The YAML editor no longer checks documents itself: the review shows what the server refuses, at its line.
