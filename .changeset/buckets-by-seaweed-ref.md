---
default: patch
---

# Object storage lists buckets that point at their store with seaweedRef

An object store now lists `Bucket` objects that name it in `spec.seaweedRef`, not only in `spec.clusterRef`. New buckets use `seaweedRef`, the field the SeaweedFS Operator documents. `clusterRef` is its deprecated alias.
