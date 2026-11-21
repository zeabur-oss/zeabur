---
default: major
---

# Catalog components are named by id

Zeabur OSS now uses Zeabur Catalog v0.5, which names each component by its id alone, such as `traefik` or `cloudnative-pg`, instead of `<capability>/<id>`. `zeaburctl components export` takes these ids, and the Components page links use them. Scripts that pass `ingress/traefik` must pass `traefik` instead. Velero moves to chart 12.2.3.
