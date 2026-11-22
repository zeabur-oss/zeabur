---
default: major
---

# The image form reads zot with the CA Zeabur Builder publishes

Zeabur now reads an image in the build registry, such as `zot.zot.svc:5000`, with the CA in the project's latest upload Secret, read as the person. It trusts that CA only for the registry's host. So the image form gets the ports and the user of an image in zot without a CA set at install, and a new CA after Zeabur Builder is installed again needs no restart.

The chart value `builds.caBundle`, the server flag `--build-registry-ca-file` (`ZEABUR_BUILD_REGISTRY_CA_FILE`) and `registryCa` in `zeabur-info` are removed. If you set `builds.caBundle`, remove it from your values. For a registry whose CA Zeabur Builder does not publish, set `extraTrustedCA`.
