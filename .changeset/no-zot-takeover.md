---
default: major
---

# The installer no longer takes over a standalone zot when it installs Zeabur Builder

`container-registry/zot` in `enable` or `disable` is now an unknown component, and `apply` no longer turns a zot that zeabur-installer 0.1.0 or 0.1.1 installed into Zeabur Builder's. If the server has that zot, remove it before you run `apply` with this version. `kubectl -n kube-system get helmchart zot -o jsonpath='{.metadata.annotations.installer\.zeabur\.com/component}'` prints `container-registry/zot` on such a server, and `builds/zeabur-builder` or nothing on any other. Remove `container-registry/zot` from `enable` and `disable`, then run `kubectl -n kube-system delete helmchart zot`, wait until `kubectl -n zot get pods` lists none, and run `kubectl delete namespace zot`. The namespace holds the images zot stored; builds push them again. `apply` then installs Zeabur Builder with its own zot.
