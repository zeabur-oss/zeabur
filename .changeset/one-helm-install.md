---
default: minor
---

# One `helm install` for Zeabur OSS and Zeabur Identity

The chart now installs Zeabur Identity with Zeabur OSS, registers Zeabur OSS as its client and makes the client secret, so no `zeabur-identityctl` step comes first. With no `global.domain`, Zeabur OSS is at `zeabur.<ip>.sslip.io` and Identity at `identity.<ip>.sslip.io`, where `<ip>` is the first node's address. Right after the install, a hook Job installs cert-manager from Zeabur Catalog, and both addresses get Let's Encrypt certificates. The install notes print the link that creates the first administrator. Set `identity.enabled=false` to keep an issuer of your own.
