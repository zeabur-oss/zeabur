---
name: zeabur-oss-spec-work
description: Implement or change a security- or protocol-relevant behaviour in any zeabur-oss repository (Kubernetes API semantics, server-side apply, RBAC, admission, OAuth and OpenID Connect, WebAuthn, HTTP caching) from the specification text, with citations in rustdoc, fail-closed handling and adversarial tests. Use for any such feature or fix.
---

# Zeabur OSS specification-driven work

The repository's spec-work skill names its protocols, its cluster-test skill and any extra step
(a conformance table, a secret-handling rule).

1. **Read the source.** Find the exact section of the RFC, W3C or OIDF specification, KEP,
   Kubernetes documentation revision or upstream source the behaviour depends on. Note MUST and
   SHOULD (RFC 2119, RFC 8174). For prior art, read the other project's code at a pinned tag or
   commit. Cite only what you have read, and never invent a section number, URL, author or
   date. A source you have not read is not cited at all; if it matters, note it in the internal
   docs or an issue.
2. **Cite the rule next to the code, decide in the docs.** In rustdoc: the constraint the code
   keeps (the spec rule or the invariant) and the citation as a markdown reference link; collect
   several sources in a `# References` section (IEEE-like: authors, title, venue, date,
   URL/DOI). The decision and its reasons (the threat, the options) go in the repository's
   docs, stated as facts about the design, not as history. Comments otherwise follow
   `zeabur-oss-standards`, section 1.
3. **Fail closed.** Ambiguous answers (from storage or the API server), unknown inputs and races
   are errors or explicit outcomes, never silent success. A single-use thing (a code, a link, a
   ceremony, a plan) is consumed or checked by one atomic step (create-once, compare-and-swap,
   a write with `uid` or `resourceVersion` preconditions) before anything is issued or applied.
4. **Test the attack, not just the happy path.** Replays, tampered tokens, concurrent
   redemption or applies (64 tasks behind a barrier, exactly one wins), forged or cross-site
   requests, wrong audience, issuer or nonce, expiry, revoked permissions and disabled users,
   privilege boundaries: the project's own service account must never gain a right its users
   lack. Use the real kube-apiserver for anything RBAC or admission related
   (`zeabur-oss-cluster-tests`).
