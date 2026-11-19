---
default: minor
---
# A plan dry-runs the pods it starts

A plan that starts or scales up a Deployment or StatefulSet sends one of its Pods as a dry run. A quota, a LimitRange maximum or a Pod Security level that would refuse the pods now refuses the plan at its change. The pods the plan adds, with the LimitRange's default requests, are held against the project's quotas.
