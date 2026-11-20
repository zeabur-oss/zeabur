---
default: patch
---

# zeaburctl reads the API with the server's own types

`zeaburctl doctor`, `up` and `connect` send and read the API's requests and answers as the types the server writes, not as copies of their own. A field the server renames or retypes now stops the build of zeaburctl instead of failing when a command runs. A refusal reads as the server's problem, so a missing permission is named as before.
