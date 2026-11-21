---
default: patch
---

# The components Job trusts extraTrustedCA and uses outboundProxy

The `post-install` Job that installs `components.install` now mounts `extraTrustedCA` and sets the `outboundProxy` variables, as the server does. Before, a `catalog.url` behind a private CA or a proxy made the Job fail with `invalid peer certificate: UnknownIssuer`.
