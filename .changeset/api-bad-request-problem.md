---
default: patch
---

# The API refuses a malformed request body or path as a problem

A request body that wasn't valid JSON, didn't match the request, or came without `Content-Type: application/json` got a plain-text answer, and so did a path parameter that didn't parse. These now get an `application/problem+json` `400` like every other refusal. The detail says where the body is wrong, such as the field `changes[0].op` or a line and column. It no longer lists every value the field accepts.
