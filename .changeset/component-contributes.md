---
default: minor
---

# Install components that add values to other components

Components that add values to another component's release, such as CrowdSec's bouncer in Traefik and VictoriaMetrics' Alertmanager in Falco, now install from **Cluster** → **Components**. The plan shows the other component as a step: Zeabur OSS writes its chart again, or a `HelmChartConfig` for Traefik that K3s or RKE2 packages. Removing the component takes its values out again. When another tool installed the other component, or you wrote its `HelmChartConfig`, Zeabur OSS leaves it as it is and the plan shows the values to add yourself.
