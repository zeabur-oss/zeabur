/** Whether the person's own browser reaches a public address: from their network, not from
 * everywhere. With `no-cors` the browser hides the response and the cause of a failure, so a
 * reply means reachable and a failure says nothing about DNS or the certificate in particular. */
export type BrowserCheck =
  | { state: "checking" }
  | { state: "reachable"; at: number }
  | { state: "unreachable"; at: number }
  /** The browser does not check plain http: mixed content from an https page, and the CSP
   * allows https only. */
  | { state: "skipped" }

/** How long a result stands before the address is checked again. */
export const CHECK_EVERY = 60_000
const CHECK_TIMEOUT = 5_000

/** The URL to check for a domain, or nothing when the browser does not check it. */
export function checkUrl(address: string, tls: boolean): string | undefined {
  return tls && !address.includes("*") ? `https://${address}/` : undefined
}

/** Whether `last`, checked or not, is due for another check at `now`. */
export function due(last: BrowserCheck | undefined, now: number): boolean {
  if (!last) return true
  if (last.state === "checking" || last.state === "skipped") return false
  return now - last.at >= CHECK_EVERY
}

type Fetch = (url: string, init: RequestInit) => Promise<unknown>

/** Checks `url` once: any reply within `timeout` is reachable; a refusal, a TLS or DNS failure
 * or no reply is not. */
export async function check(
  url: string,
  fetcher: Fetch,
  now: () => number,
  timeout = CHECK_TIMEOUT,
): Promise<Exclude<BrowserCheck, { state: "checking" | "skipped" }>> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeout)
  try {
    await fetcher(url, {
      mode: "no-cors",
      cache: "no-store",
      credentials: "omit",
      redirect: "follow",
      signal: controller.signal,
    })
    return { state: "reachable", at: now() }
  } catch {
    return { state: "unreachable", at: now() }
  } finally {
    clearTimeout(timer)
  }
}
