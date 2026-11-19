---
default: major
---

# The server names the role and rule behind each right

`GET /api/v1/rights` lists the rights the interface gates actions by, the least role and the Kubernetes rule behind each, and the right that covers each resource. `GET /api/v1/whoami` adds `member` and `admin`. Members carry `canRemove` and `canChangeAccess`, and an API token's `createdBy` is the Zeabur Identity user name without the `zeabur:` prefix. The web no longer keeps its own copy of these.
