---
default: major
---

# Times in the API are typed and end in `At`

Every time the API returns is an RFC 3339 instant in UTC, stated as `date-time` in `openapi.yaml`. Calendar days, such as release dates and idle measurements, are `date`. A time that another tool wrote and that is not RFC 3339 is left out. Time fields are renamed to end in `At`: for example, `created` becomes `createdAt`, `since` becomes `startedAt`, and a usage `timestamp` becomes `sampledAt`. The commit that renames them lists every field. Clients must read the new names.
