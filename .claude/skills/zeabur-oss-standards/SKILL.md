---
name: zeabur-oss-standards
description: The engineering standards every zeabur-oss repository reviews against — Simplified Technical English for comments, doc comments and commit messages; which tests to keep, merge or delete; design for a caller that exists; no unbounded cost at scale (Kubernetes API, object sizes, replicas, observability, front end). Use when writing a comment, rustdoc or commit message, adding or removing a test, adding a trait, layer, option or flag, or writing code that lists, watches, polls, stores or fans out, in any zeabur-oss repository.
---

# Zeabur OSS engineering standards

Apply [`STANDARDS.md`](STANDARDS.md) and check the change against it. Each item is a fail
condition a reviewer can point at; the "Leave alone" lists stop over-correction.

The repository's dev skill adds its own part: its technical nouns, examples in its language,
extra scale rules (an installer, manifests, components) and extra "Leave alone" items. Read both.
