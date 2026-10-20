---
name: zeabur-spec-work
description: Implement or change a security- or protocol-relevant behaviour in Zeabur (Kubernetes API semantics, server-side apply, RBAC, OAuth/OIDC with Zeabur Identity, MCP, HTTP caching) the way the codebase expects — from the specification text, with citations and adversarial tests. Use for any such feature or fix.
---

# Specification-driven work

1. **Read the source.** Find the exact section of the RFC, KEP, Kubernetes documentation, MCP
   revision or upstream source the behaviour depends on. Note MUST/SHOULD (RFC 2119, RFC 8174).
   For prior art, read the other project's code at a pinned tag or commit. Never cite something
   you have not read; if a fact cannot be verified, write "not verified" instead of inventing a
   section number, URL, author or date.
2. **Decide and write it down where the code is.** In the module's rustdoc: what it does, the
   invariant, why (the threat or the spec rule), and the citation as a markdown reference link;
   collect several sources in a `# References` section (IEEE-like: authors, title, venue, date,
   URL/DOI). State decisions as facts about the design, not as history.
3. **Fail closed.** Ambiguous answers, unknown inputs and races are errors or explicit outcomes,
   never silent success. A plan is applied only against the resource versions it was computed
   from; anything changed in between is refused.
4. **Test the attack, not just the happy path.** Tampered plan tokens, replays, concurrent
   applies (64 tasks behind a barrier, exactly one wins), cross-site requests, expired tokens,
   revoked permissions, privilege boundaries (Zeabur's own service account must never gain a
   right its users lack; no `impersonate`). Use the real kube-apiserver for anything RBAC or
   admission related (`zeabur-cluster-tests`).
5. **Never leak a Secret's value.** Values are read only on an explicit "show" request, never
   cached, never logged, never returned through MCP. Test it.
