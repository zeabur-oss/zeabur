/** The person a Kubernetes user name stands for: an OIDC user name carries the issuer's prefix
 * (`zeabur:ada@example.com`), which is not theirs. `system:` names are Kubernetes' own. */
export function personOf(username: string): string {
  if (username.startsWith("system:")) return username
  const colon = username.lastIndexOf(":")
  return colon >= 0 ? username.slice(colon + 1) : username
}

/** A time of day, 24-hour in every language. */
export function formatClock(iso: string, locale: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return iso
  return new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).format(date)
}
