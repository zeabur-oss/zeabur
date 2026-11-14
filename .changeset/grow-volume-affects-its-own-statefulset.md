---
default: patch
---

# Growing a volume names only the StatefulSet it belongs to

Before you grow a volume, the confirmation says which services the change also affects. It also named a StatefulSet whose name only ended the volume's name, such as `replica` for `data-db-replica-0`. Now it names the StatefulSet that created the volume from its claim template or controls it, and the services that mount it.
