---
default: minor
---

# Install components that ask for values

Components that ask for values, such as Velero's bucket and credentials, now install from **Cluster** → **Components**. The dialog shows a field for each value; secret values go into a Secret before the chart and never show again. Zeabur OSS makes a random value for a key the catalog lets it generate, keeps every value for the next install, and **Change values** on an installed component writes new ones. A component Zeabur OSS cannot install yet says why on its card, and **How to install** shows the commands that print it for Flux or Argo CD.
