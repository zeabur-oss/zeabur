---
name: zeabur-spec-work
description: Implement or change a security- or protocol-relevant behaviour in Zeabur (Kubernetes API semantics, server-side apply, RBAC, OAuth/OIDC with Zeabur Identity, HTTP caching) the way the codebase expects — from the specification text, with citations and adversarial tests. Use for any such feature or fix.
---

# Specification-driven work

Follow `zeabur-oss-spec-work` (install: `npx skills add zeabur-oss/skills --skill
zeabur-oss-spec-work`; `skills-lock.json` pins it): read the source, cite it in rustdoc, fail
closed, test the attack. In Zeabur also:

1. **Plans.** A plan is applied only against the resource versions it was computed from;
   anything changed in between is refused. Test tampered plan tokens, replays and concurrent
   applies.
2. **Zeabur's own rights.** Zeabur's service account never gains a right its users lack, apart
   from pods in the terminal namespace after an access review of the person; no `impersonate`.
   Cluster tests: `zeabur-cluster-tests`.
3. **Never leak a Secret's value.** Values are read only on an explicit "show" request, never
   cached, never logged. Test it.
