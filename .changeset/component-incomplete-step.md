---
default: patch
---

# A component with a step that has not started shows as incomplete

When Zeabur OSS had written only some of a component's steps, as with CloudNativePG without its Barman Cloud plugin, the Components page showed the component as installed. Now its card says it is incomplete and names the step that has not started, and the overview marks it the same way. **Manage** installs the rest.
