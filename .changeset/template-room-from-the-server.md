---
default: minor
---
# Template room is counted node by node

Whether the cluster has room for a template now comes from Zeabur (`GET /api/v1/cluster/template-rooms`). Each pod must fit on one ready, schedulable node that keeps a tenth of its room free, so room spread over several nodes no longer counts.
