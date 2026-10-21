/** The person a Kubernetes user name stands for: an OIDC user name carries the issuer's prefix
 * (`zeabur:ada@example.com`), which is not theirs. `system:` names are Kubernetes' own. */
export function personOf(username: string): string {
  if (username.startsWith("system:")) return username
  const colon = username.lastIndexOf(":")
  return colon >= 0 ? username.slice(colon + 1) : username
}

const ipv4 = /^\d{1,3}(\.\d{1,3}){3}$/

/** The cluster's name when it is one a person gave it; `undefined` for kube-apiserver's in-cluster
 * address (its ClusterIP or `kubernetes.default.svc`), which names nothing. */
export function clusterNameOf(name: string | undefined): string | undefined {
  if (!name) return undefined
  const host = name.replace(/^\[|\]$/g, "")
  if (ipv4.test(host) || host.includes(":") || /^kubernetes(\.default)?(\.|$)/.test(host))
    return undefined
  return name
}

/** A time of day: 24-hour in Chinese, the locale's own clock otherwise. */
export function formatClock(iso: string, locale: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  const chinese = locale.startsWith("zh")
  return new Intl.DateTimeFormat(locale, {
    hour: chinese ? "2-digit" : "numeric",
    minute: "2-digit",
    hourCycle: chinese ? "h23" : undefined,
  }).format(date)
}
