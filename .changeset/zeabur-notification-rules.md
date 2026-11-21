---
default: minor
---

# Zeabur OSS applies its own alert rules and log access

Zeabur OSS now applies the `zeabur-notifications` VMRule when it installs VictoriaMetrics, and the NetworkPolicy that lets Zeabur Proxy read VictoriaLogs when it installs VictoriaLogs. They come from the Zeabur OSS binary, so the rules match the Zeabur OSS version that reads them. `zeaburctl components export` prints them after each component for Helm, Flux and Argo CD installs. When the rules are missing or come from another Zeabur OSS version, **Cluster** → **Notifications** says so and offers **Install rules** or **Update rules**. The rules read the custom resource metrics as `kube_customresource_*`.
