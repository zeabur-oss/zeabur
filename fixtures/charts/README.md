# Chart corpus

Rendered Kubernetes manifests of real Helm charts, used to measure how much of real-world
input the projection covers. One file per component of the
[Zeabur Catalog](https://github.com/zeabur/catalog), named
`<capability>__<component>.yaml`, each holding every release of the component in install
order as one multi-document YAML stream.

- Source: `zeabur/catalog` at commit `b33ce91c8dd257a474aa7d6707466347c791072c`.
- Profile: `single-node`, Kubernetes version `1.37.0` (the newest in its
  `config/kubernetes.yaml`).
- Rendered by the catalogue's own `recs render`: `helm template --include-crds` with the
  release's values, the profile, the components' CI test inputs (`testValue`/`default`) and
  values contributed by other components (CrowdSec's Traefik middleware). `urls` releases are
  the downloaded upstream manifests; `manifests` releases are the inline objects.
- CustomResourceDefinition documents and empty documents are removed. Everything else is
  verbatim, including Helm's `# Source:` comments.
- Deterministic: the catalogue is rendered twice and the script fails if the renders differ.
  The catalogue forbids generated secrets and certificates in charts (`recs render` checks
  it too), so no value had to be replaced.

Each file keeps the licence of the chart or manifests it was rendered from, named in the table
with the licence file of their repository. The MIT notice of CrowdSec's charts is in
`LICENSE-crowdsec-helm-charts.txt`, Trivy Operator's NOTICE (Apache-2.0, section 4(d)) in
`LICENSE-trivy-operator-NOTICE.txt`; the Apache-2.0 text is in the `LICENSE` at the root.

Regenerate with Nix and network access to the chart registries:

```sh
git clone https://github.com/zeabur/catalog ~/recs
RECS_DIR=~/recs RECS_COMMIT=b33ce91c8dd257a474aa7d6707466347c791072c scripts/render-chart-corpus.sh
```

| Component | File | Releases (name@version) | Licence | Documents | Bytes |
| --- | --- | --- | --- | --- | --- |
| certificates/cert-manager | `certificates__cert-manager.yaml` | cert-manager@v1.21.2, issuers@inline | [Apache-2.0](https://github.com/cert-manager/cert-manager/blob/master/LICENSE) | 47 | 47501 |
| config-reload/reloader | `config-reload__reloader.yaml` | reloader@2.2.18 | [Apache-2.0](https://github.com/stakater/Reloader/blob/master/LICENSE) | 6 | 6727 |
| databases/cloudnative-pg | `databases__cloudnative-pg.yaml` | cloudnative-pg@0.29.1, plugin-barman-cloud@0.8.1 | [Apache-2.0](https://github.com/cloudnative-pg/charts/blob/main/LICENSE) | 22 | 45653 |
| databases/percona-mongodb | `databases__percona-mongodb.yaml` | psmdb-operator@1.23.2 | [Apache-2.0](https://github.com/percona/percona-helm-charts/blob/main/LICENSE) | 4 | 6112 |
| dns/external-dns | `dns__external-dns.yaml` | external-dns@1.23.0 | [Apache-2.0](https://github.com/kubernetes-sigs/external-dns/blob/master/LICENSE.md) | 5 | 4752 |
| event-autoscaling/keda-http-add-on | `event-autoscaling__keda-http-add-on.yaml` | http-add-on@0.16.0 | [Apache-2.0](https://github.com/kedacore/charts/blob/main/LICENSE) | 21 | 23081 |
| event-autoscaling/keda | `event-autoscaling__keda.yaml` | keda@2.21.0 | [Apache-2.0](https://github.com/kedacore/charts/blob/main/LICENSE) | 23 | 30334 |
| ingress/traefik | `ingress__traefik.yaml` | gateway-api-crds@v1.6.2, traefik@41.7.1 | gateway-api-crds: [Apache-2.0](https://github.com/kubernetes-sigs/gateway-api/blob/main/LICENSE); traefik: [Apache-2.0](https://github.com/traefik/traefik-helm-chart/blob/master/LICENSE) | 9 | 12428 |
| intrusion-prevention/crowdsec | `intrusion-prevention__crowdsec.yaml` | crowdsec@0.24.2, bouncer-middleware@inline | crowdsec: [MIT](https://github.com/crowdsecurity/helm-charts/blob/main/LICENSE), notice in `LICENSE-crowdsec-helm-charts.txt`; bouncer-middleware: [Apache-2.0](https://github.com/zeabur/catalog/blob/main/LICENSE) (catalogue) | 12 | 27539 |
| logs/victoria-logs | `logs__victoria-logs.yaml` | logs@inline | [Apache-2.0](https://github.com/zeabur/catalog/blob/main/LICENSE) (catalogue) | 3 | 1815 |
| metrics/metrics-server | `metrics__metrics-server.yaml` | metrics-server@3.14.0 | [Apache-2.0](https://github.com/kubernetes-sigs/metrics-server/blob/master/LICENSE) | 9 | 6972 |
| metrics/victoria-metrics | `metrics__victoria-metrics.yaml` | k8s-stack@0.95.2 | [Apache-2.0](https://github.com/VictoriaMetrics/helm-charts/blob/master/LICENSE) | 27 | 36885 |
| node-updates/system-upgrade-controller | `node-updates__system-upgrade-controller.yaml` | crds@v0.20.2, controller@v0.20.2 | [Apache-2.0](https://github.com/rancher/system-upgrade-controller/blob/master/LICENSE) | 10 | 6856 |
| observability-operator/victoria-metrics-operator | `observability-operator__victoria-metrics-operator.yaml` | operator@0.68.1 | [Apache-2.0](https://github.com/VictoriaMetrics/helm-charts/blob/master/LICENSE) | 18 | 41171 |
| remote-access/tailscale-operator | `remote-access__tailscale-operator.yaml` | tailscale-operator@1.104.1 | [BSD-3-Clause](https://github.com/tailscale/tailscale/blob/main/LICENSE) | 10 | 7862 |
| resource-recommendations/vertical-pod-autoscaler | `resource-recommendations__vertical-pod-autoscaler.yaml` | vpa@0.13.0 | [Apache-2.0](https://github.com/kubernetes/autoscaler/blob/master/LICENSE) | 15 | 14015 |
| runtime-security/falco | `runtime-security__falco.yaml` | policy-report-crds@ed0ca5a1c7788055074c7500915b76fc4518ed33, alert-readers@inline, falco@9.2.0 | [Apache-2.0](https://github.com/falcosecurity/charts/blob/master/LICENSE) | 14 | 31656 |
| uptime-probes/blackbox-exporter | `uptime-probes__blackbox-exporter.yaml` | blackbox-exporter@11.20.0 | [Apache-2.0](https://github.com/prometheus-community/helm-charts/blob/main/LICENSE) | 4 | 4325 |
| volume-backup/k8up | `volume-backup__k8up.yaml` | k8up@4.10.0 | [Apache-2.0](https://github.com/k8up-io/k8up/blob/master/LICENSE) | 12 | 10172 |
| volume-backup/velero | `volume-backup__velero.yaml` | velero@12.2.1 | [Apache-2.0](https://github.com/vmware-tanzu/helm-charts/blob/main/LICENSE) | 13 | 12634 |
| vulnerability-scanning/trivy-operator | `vulnerability-scanning__trivy-operator.yaml` | trivy-operator@0.37.0 | [Apache-2.0](https://github.com/aquasecurity/trivy-operator/blob/main/LICENSE), NOTICE in `LICENSE-trivy-operator-NOTICE.txt` | 21 | 66828 |

Total: 445318 bytes.
