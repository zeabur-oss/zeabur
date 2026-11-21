---
default: patch
---
# zeaburctl reads a newer server's answers

`zeaburctl up` against a newer Zeabur OSS server no longer fails on a field or value this version does not know. It reads only the fields it uses, and a problem kind or backup provider state it does not know reads as unknown.
