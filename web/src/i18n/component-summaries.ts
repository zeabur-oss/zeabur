// What each catalogue component gives the cluster, in one sentence per locale. Keys are
// `capability/id#hash`, the hash of the catalogue rationale the sentence summarises, so a changed
// rationale leaves the old sentence unused until someone rewrites it (component-summaries.test.ts
// fails meanwhile). The rationale itself stays in the install sheet's notes.
import type { Locale } from "@/i18n"

type Summary = { en: string } & Partial<Record<Exclude<Locale, "en">, string>>

/** Locales whose every summary must be written; the others fall back to English. */
export const completeLocales: readonly Locale[] = ["en", "zh-TW", "zh-CN"]

export const summaries: Record<string, Summary> = {
  "certificates/cert-manager#c364a1d4": {
    en: "Issues TLS certificates for your domains and renews them.",
    "zh-TW": "為你的網域簽發 TLS 憑證，並自動續期。",
    "zh-CN": "为你的域名签发 TLS 证书，并自动续期。",
  },
  "config-reload/reloader#96f8c0a5": {
    en: "Restarts services when a ConfigMap or Secret they use changes.",
    "zh-TW": "服務使用的 ConfigMap 或 Secret 變更時，自動重新啟動服務。",
    "zh-CN": "服务使用的 ConfigMap 或 Secret 变更时，自动重启服务。",
  },
  "container-registry/zot#6396ba2c": {
    en: "Stores the images Zeabur Builder builds, inside the cluster.",
    "zh-TW": "在叢集裡存放 Zeabur Builder 建置的映像檔。",
    "zh-CN": "在集群里存放 Zeabur Builder 构建的镜像。",
  },
  "databases/cloudnative-pg#e5be71dc": {
    en: "Runs PostgreSQL databases with failover and backups to S3.",
    "zh-TW": "執行 PostgreSQL 資料庫，支援故障切換和備份到 S3。",
    "zh-CN": "运行 PostgreSQL 数据库，支持故障切换和备份到 S3。",
  },
  "databases/dragonfly-operator#49e3a8b9": {
    en: "Runs Dragonfly, a Redis-compatible store, with replicas and snapshots, under a licence that is not open source.",
    "zh-TW": "執行與 Redis 相容的 Dragonfly，支援副本和快照；採用的不是開放原始碼授權。",
    "zh-CN": "运行与 Redis 兼容的 Dragonfly，支持副本和快照；采用的不是开源许可。",
  },
  "databases/mariadb-operator#9ef81ade": {
    en: "Runs MariaDB databases, a MySQL-compatible choice, with backups to S3.",
    "zh-TW": "執行與 MySQL 相容的 MariaDB 資料庫，並備份到 S3。",
    "zh-CN": "运行与 MySQL 兼容的 MariaDB 数据库，并备份到 S3。",
  },
  "databases/moco#ad806183": {
    en: "Runs MySQL 8 databases with replicas and point-in-time recovery from S3.",
    "zh-TW": "執行 MySQL 8 資料庫，支援副本，並能從 S3 還原到任一時間點。",
    "zh-CN": "运行 MySQL 8 数据库，支持副本，并能从 S3 恢复到任一时间点。",
  },
  "databases/percona-mongodb#925d1dee": {
    en: "Runs MongoDB databases with backups to S3, under the SSPL licence.",
    "zh-TW": "執行 MongoDB 資料庫並備份到 S3，採 SSPL 授權。",
    "zh-CN": "运行 MongoDB 数据库并备份到 S3，采用 SSPL 许可。",
  },
  "dns/external-dns#88abb875": {
    en: "Creates DNS records for your services' hostnames.",
    "zh-TW": "為服務的主機名稱建立 DNS 紀錄。",
    "zh-CN": "为服务的主机名创建 DNS 记录。",
  },
  "event-autoscaling/keda#d2bb368f": {
    en: "Scales services on queues, schedules and other events, down to zero.",
    "zh-TW": "依佇列、排程等事件調整服務的副本數，可降到零。",
    "zh-CN": "按队列、定时等事件调整服务的副本数，可缩到零。",
  },
  "event-autoscaling/keda-http-add-on#8d42d7fa": {
    en: "Scales web services with their traffic, down to zero when idle.",
    "zh-TW": "依流量調整網頁服務的副本數，閒置時可降到零。",
    "zh-CN": "按流量调整 Web 服务的副本数，空闲时可缩到零。",
  },
  "gotify-notifications/alertmanager-gotify-bridge#15013f81": {
    en: "Lets notification channels send alerts to a Gotify server.",
    "zh-TW": "讓通知管道把警示送到 Gotify 伺服器。",
    "zh-CN": "让通知渠道把告警发到 Gotify 服务器。",
  },
  "image-updates/keel#9ece2498": {
    en: "Deploys each new image to the services you turn auto-deploy on for.",
    "zh-TW": "新的映像檔一建好，就部署到開了自動部署的服務。",
    "zh-CN": "新的镜像一构建好，就部署到开了自动部署的服务。",
  },
  "ingress/traefik#da971ab9": {
    en: "Routes traffic from your domains to your services.",
    "zh-TW": "把網域的流量導向你的服務。",
    "zh-CN": "把域名的流量转发到你的服务。",
  },
  "intrusion-prevention/crowdsec#bbe1ad93": {
    en: "Blocks brute-force attempts and scanners before they reach your services.",
    "zh-TW": "在暴力破解與掃描抵達服務前加以阻擋。",
    "zh-CN": "在暴力破解和扫描到达服务前将其拦截。",
  },
  "logs/victoria-logs#4376a766": {
    en: "Keeps and searches logs.",
    "zh-TW": "保存並搜尋日誌。",
    "zh-CN": "保存并搜索日志。",
  },
  "metrics/metrics-server#66b7453b": {
    en: "Shows the CPU and memory each pod uses right now.",
    "zh-TW": "顯示每個 Pod 目前用了多少 CPU 與記憶體。",
    "zh-CN": "显示每个 Pod 当前用了多少 CPU 和内存。",
  },
  "metrics/victoria-metrics#80ed83f2": {
    en: "Keeps metrics history and sends alerts to each project's notification channels.",
    "zh-TW": "保存指標歷史，並把警示送到各專案的通知管道。",
    "zh-CN": "保存指标历史，并把告警发到各项目的通知渠道。",
  },
  "node-updates/system-upgrade-controller#47de8b0b": {
    en: "Upgrades Kubernetes on the nodes, one node at a time.",
    "zh-TW": "逐一升級各節點上的 Kubernetes。",
    "zh-CN": "逐个升级各节点上的 Kubernetes。",
  },
  "object-storage/seaweedfs-operator#1c0622e6": {
    en: "Runs SeaweedFS object storage with an S3 API, in beta.",
    "zh-TW": "執行提供 S3 API 的 SeaweedFS 物件儲存，目前為 Beta。",
    "zh-CN": "运行提供 S3 API 的 SeaweedFS 对象存储，目前为 Beta。",
  },
  "observability-operator/victoria-metrics-operator#c028706b": {
    en: "Runs the metrics and log stores.",
    "zh-TW": "執行指標與日誌的儲存服務。",
    "zh-CN": "运行指标和日志的存储服务。",
  },
  "remote-access/tailscale-operator#479d493c": {
    en: "Puts services on your tailnet with trusted HTTPS and no port open to the internet.",
    "zh-TW": "把服務放上你的 tailnet，提供受信任的 HTTPS，不必對網際網路開放連接埠。",
    "zh-CN": "把服务放到你的 tailnet 上，提供受信任的 HTTPS，无需对互联网开放端口。",
  },
  "replicated-storage/longhorn#18291cd6": {
    en: "Keeps copies of each volume on other nodes, so it survives losing a node.",
    "zh-TW": "把每個磁碟區的副本存在其他節點上，節點故障時資料仍在。",
    "zh-CN": "把每个卷的副本存在其他节点上，节点故障时数据仍在。",
  },
  "resource-recommendations/vertical-pod-autoscaler#e6592278": {
    en: "Suggests CPU and memory requests from actual use.",
    "zh-TW": "依實際用量建議 CPU 與記憶體的請求量。",
    "zh-CN": "根据实际用量建议 CPU 和内存的请求量。",
  },
  "runtime-security/falco#1cfea57b": {
    en: "Flags suspicious activity in containers, such as a shell or a privilege escalation.",
    "zh-TW": "偵測容器內的可疑行為，例如開啟 shell 或提升權限。",
    "zh-CN": "检测容器内的可疑行为，例如打开 shell 或提升权限。",
  },
  "uptime-probes/blackbox-exporter#43d93db5": {
    en: "Checks that your sites and ports answer, and alerts when they don't.",
    "zh-TW": "檢查網站與連接埠是否回應，沒有回應時發出警示。",
    "zh-CN": "检查网站和端口是否响应，无响应时发出告警。",
  },
  "volume-backup/k8up#d57241b0": {
    en: "Backs up each project's volumes to S3-compatible storage.",
    "zh-TW": "備份各專案的磁碟區，存到相容 S3 的儲存空間。",
    "zh-CN": "把各项目的卷备份到兼容 S3 的存储。",
  },
  "volume-backup/velero#386a97cb": {
    en: "Backs up and restores the whole cluster.",
    "zh-TW": "備份並還原整個叢集。",
    "zh-CN": "备份并还原整个集群。",
  },
  "vulnerability-scanning/trivy-operator#10324322": {
    en: "Scans running images and settings for known vulnerabilities.",
    "zh-TW": "掃描執行中的映像檔與設定，找出已知弱點。",
    "zh-CN": "扫描运行中的镜像和配置，找出已知漏洞。",
  },
}

/** FNV-1a (32 bits, hex) of the text's UTF-8 bytes. */
export function sourceHash(text: string): string {
  let hash = 0x811c9dc5
  for (const byte of new TextEncoder().encode(text)) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0
  return hash.toString(16).padStart(8, "0")
}

export const summaryKey = (capability: string, id: string, rationale: string) =>
  `${capability}/${id}#${sourceHash(rationale)}`

/** The component's sentence in `locale`, else in English; none when the rationale changed. */
export function summaryOf(
  capability: string,
  id: string,
  rationale: string,
  locale: Locale,
): string | undefined {
  const summary = summaries[summaryKey(capability, id, rationale)]
  return summary ? (summary[locale] ?? summary.en) : undefined
}
