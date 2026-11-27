---
default: minor
---

# `zeaburctl templates export` prints what deploying a template writes

`zeaburctl templates export <id> --project <project>` prints the objects that deploying a Zeabur Catalog template into that project writes, the same ones the web interface writes, with Zeabur's labels, as YAML for `kubectl diff --server-side` and `kubectl apply --server-side`. Inputs that are not given show as `<name>` placeholders. When the cluster lacks a component the template needs, it says so and names the `zeaburctl components export` command. The template docs page has a prompt that has a coding agent deploy a template this way.
