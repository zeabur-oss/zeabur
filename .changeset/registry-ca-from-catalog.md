---
default: patch
---

# Built images are read with the CA the catalog publishes

Zeabur OSS now reads the build registry's CA from the Secret that the installed registry component, such as Zeabur Builder, names in the catalog (`provides.registry.ca`), as the person. The image form and folder uploads use it, so a project that never uploaded a folder can also read its built images. Zeabur OSS no longer reads `ca.crt` from upload Secrets.
