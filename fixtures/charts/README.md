# Chart corpus

Rendered Kubernetes manifests of real Helm charts, used to measure how much of real-world
input the projection covers. One file per component of the
[Zeabur Catalog](https://github.com/zeabur-oss/catalog), named
`<component>.yaml`, each holding every release of the component in install
order as one multi-document YAML stream.

- Source: `zeabur-oss/catalog` at commit `ef85ba8004af12abfe69a38ae11886f7d2483c66`.
- Profile: `single-node`, Kubernetes version `1.37.0` (the newest in its
  `config/kubernetes.yaml`).
- Rendered by the catalogue's own `catalog render`: `helm template --include-crds` with the
  release's values, the profile, the components' CI test inputs (`testValue`/`default`) and
  values contributed by other components (CrowdSec's Traefik middleware). `urls` releases are
  the downloaded upstream manifests; `manifests` releases are the inline objects.
- CustomResourceDefinition documents and empty documents are removed. Everything else is
  verbatim, including Helm's `# Source:` comments.
- Deterministic: the catalogue is rendered twice and the script fails if the renders differ.
  The catalogue forbids generated secrets and certificates in charts (`catalog render` checks
  it too), so no value had to be replaced.

Each file keeps the licence of the chart or manifests it was rendered from, named in the table
with the licence file of their repository. The MIT notices of CrowdSec's charts and the MariaDB
operator are in `LICENSE-crowdsec-helm-charts.txt` and `LICENSE-mariadb-operator.txt`,
Trivy Operator's NOTICE (Apache-2.0, section 4(d)) in `LICENSE-trivy-operator-NOTICE.txt`;
the Apache-2.0 text is in the `LICENSE` at the root.

Regenerate with network access to the chart registries:

```sh
nix develop -c scripts/render-chart-corpus.sh
```

| Component | File | Releases (name@version) | Licence | Documents | Bytes |
| --- | --- | --- | --- | --- | --- |
| blackbox-exporter | `blackbox-exporter.yaml` | blackbox-exporter@11.20.2 | [Apache-2.0](https://github.com/prometheus-community/helm-charts/blob/main/LICENSE) | 4 | 4475 |
| cert-manager | `cert-manager.yaml` | cert-manager@v1.21.3, issuers@inline | [Apache-2.0](https://github.com/cert-manager/cert-manager/blob/master/LICENSE) | 47 | 47789 |
| cloudnative-pg | `cloudnative-pg.yaml` | cloudnative-pg@0.29.1, plugin-barman-cloud@0.8.1 | [Apache-2.0](https://github.com/cloudnative-pg/charts/blob/main/LICENSE) | 22 | 45869 |
| crowdsec | `crowdsec.yaml` | ca@inline, crowdsec@0.24.2, bouncer-middleware@inline | crowdsec: [MIT](https://github.com/crowdsecurity/helm-charts/blob/main/LICENSE), notice in `LICENSE-crowdsec-helm-charts.txt`; bouncer-middleware: [Apache-2.0](https://github.com/zeabur-oss/catalog/blob/main/LICENSE) (catalogue) | 18 | 29816 |
| dragonfly-operator | `dragonfly-operator.yaml` | dragonfly-operator@v1.7.1 | [Apache-2.0](https://github.com/dragonflydb/dragonfly-operator/blob/main/LICENSE) | 14 | 15303 |
| external-secrets | `external-secrets.yaml` | external-secrets@2.14.0 | [Apache-2.0](https://github.com/external-secrets/external-secrets/blob/main/LICENSE) | 16 | 19213 |
| falco | `falco.yaml` | policy-report-crds@ed0ca5a1c7788055074c7500915b76fc4518ed33, alert-readers@inline, falco@9.2.1 | [Apache-2.0](https://github.com/falcosecurity/charts/blob/master/LICENSE) | 14 | 32140 |
| k8up | `k8up.yaml` | k8up@4.10.1 | [Apache-2.0](https://github.com/k8up-io/k8up/blob/master/LICENSE) | 12 | 10588 |
| keda-http-add-on | `keda-http-add-on.yaml` | http-add-on@0.16.0 | [Apache-2.0](https://github.com/kedacore/charts/blob/main/LICENSE) | 21 | 23327 |
| keda | `keda.yaml` | keda@2.21.0 | [Apache-2.0](https://github.com/kedacore/charts/blob/main/LICENSE) | 23 | 30550 |
| keel | `keel.yaml` | keel@v1.2.3, webhook-policy@inline | [MPL-2.0](https://github.com/keel-hq/keel/blob/master/LICENSE) | 7 | 4440 |
| longhorn | `longhorn.yaml` | longhorn@1.13.0, ui-policy@inline | longhorn: [Apache-2.0](https://github.com/longhorn/charts/blob/v1.13.x/LICENSE); ui-policy: [Apache-2.0](https://github.com/zeabur-oss/catalog/blob/main/LICENSE) (catalogue) | 37 | 37975 |
| mariadb-operator | `mariadb-operator.yaml` | mariadb-operator-crds@26.10.2, mariadb-operator@26.10.2 | [MIT](https://github.com/mariadb-operator/mariadb-operator/blob/main/LICENSE), notice in `LICENSE-mariadb-operator.txt` | 16 | 19984 |
| metrics-server | `metrics-server.yaml` | metrics-server@3.14.0 | [Apache-2.0](https://github.com/kubernetes-sigs/metrics-server/blob/master/LICENSE) | 9 | 7044 |
| moco | `moco.yaml` | moco@0.28.0 | [Apache-2.0](https://github.com/cybozu-go/moco/blob/main/LICENSE) | 20 | 17121 |
| percona-mongodb | `percona-mongodb.yaml` | psmdb-operator@1.23.2 | [Apache-2.0](https://github.com/percona/percona-helm-charts/blob/main/LICENSE) | 4 | 6350 |
| reloader | `reloader.yaml` | reloader@2.2.19 | [Apache-2.0](https://github.com/stakater/Reloader/blob/master/LICENSE) | 6 | 6888 |
| seaweedfs-operator | `seaweedfs-operator.yaml` | seaweedfs-operator@0.1.46 | [Apache-2.0](https://github.com/seaweedfs/seaweedfs-operator/blob/master/LICENSE) | 14 | 12832 |
| system-upgrade-controller | `system-upgrade-controller.yaml` | crds@v0.20.2, controller@v0.20.2 | [Apache-2.0](https://github.com/rancher/system-upgrade-controller/blob/master/LICENSE) | 10 | 6856 |
| tailscale-operator | `tailscale-operator.yaml` | tailscale-operator@1.104.4 | [BSD-3-Clause](https://github.com/tailscale/tailscale/blob/main/LICENSE) | 10 | 8006 |
| traefik | `traefik.yaml` | gateway-api-crds@v1.6.3, traefik@42.1.3 | gateway-api-crds: [Apache-2.0](https://github.com/kubernetes-sigs/gateway-api/blob/main/LICENSE); traefik: [Apache-2.0](https://github.com/traefik/traefik-helm-chart/blob/master/LICENSE) | 9 | 12571 |
| trivy-operator | `trivy-operator.yaml` | trivy-operator@0.38.1 | [Apache-2.0](https://github.com/aquasecurity/trivy-operator/blob/main/LICENSE), NOTICE in `LICENSE-trivy-operator-NOTICE.txt` | 21 | 67120 |
| velero | `velero.yaml` | velero@12.2.3 | [Apache-2.0](https://github.com/vmware-tanzu/helm-charts/blob/main/LICENSE) | 12 | 10932 |
| vertical-pod-autoscaler | `vertical-pod-autoscaler.yaml` | vpa@0.15.0 | [Apache-2.0](https://github.com/kubernetes/autoscaler/blob/master/LICENSE) | 15 | 14279 |
| victoria-logs | `victoria-logs.yaml` | logs@inline | [Apache-2.0](https://github.com/zeabur-oss/catalog/blob/main/LICENSE) (catalogue) | 3 | 3150 |
| victoria-metrics-operator | `victoria-metrics-operator.yaml` | operator@0.68.1 | [Apache-2.0](https://github.com/VictoriaMetrics/helm-charts/blob/master/LICENSE) | 18 | 41643 |
| victoria-metrics | `victoria-metrics.yaml` | k8s-stack@0.96.0, notifications@inline | [Apache-2.0](https://github.com/VictoriaMetrics/helm-charts/blob/master/LICENSE) | 30 | 60784 |
| zeabur-builder | `zeabur-builder.yaml` | tls@inline, zot@0.1.130, builder@0.2.1 | tls: [Apache-2.0](https://github.com/zeabur-oss/catalog/blob/main/LICENSE) (catalogue); zot: [Apache-2.0](https://github.com/project-zot/helm-charts/blob/main/LICENSE); builder: [Apache-2.0](https://github.com/zeabur-oss/builder/blob/main/LICENSE) | 30 | 69810 |

Total: 666855 bytes.
