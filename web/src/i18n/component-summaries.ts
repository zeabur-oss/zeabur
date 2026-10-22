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
  "databases/cloudnative-pg#e5be71dc": {
    en: "Runs PostgreSQL databases with failover and backups to S3.",
    "zh-TW": "執行 PostgreSQL 資料庫，支援故障切換和備份到 S3。",
    "zh-CN": "运行 PostgreSQL 数据库，支持故障切换和备份到 S3。",
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
  "metrics/victoria-metrics#819600ce": {
    en: "Keeps metrics history and sends alerts.",
    "zh-TW": "保存指標歷史並送出警示。",
    "zh-CN": "保存指标历史并发出告警。",
  },
  "node-updates/system-upgrade-controller#1315d20c": {
    en: "Upgrades k3s on the nodes, one node at a time.",
    "zh-TW": "逐一升級各節點上的 k3s。",
    "zh-CN": "逐个升级各节点上的 k3s。",
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
