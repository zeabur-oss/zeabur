import type { Schemas } from "@/api/client"

/** `namespace/name/listener` as the Select keeps it. */
export function parentOf(value: string): Schemas["GatewayParent"] | undefined {
  const [namespace, name, sectionName] = value.split("/")
  return namespace && name ? { namespace, name, sectionName } : undefined
}

/** Whether a listener's hostname (absent: any) can serve `host`, as Gateway API matches them: a
 * wildcard covers one or more labels. A host still being typed matches every listener. */
export function listenerServes(listener: string | undefined, host: string): boolean {
  if (!listener || !host.includes(".")) return true
  const covers = (wild: string, other: string) =>
    wild.startsWith("*.") && other.endsWith(wild.slice(1)) && other.length > wild.length - 1
  return listener === host || covers(listener, host) || covers(host, listener)
}

/** The listeners, as `namespace/name/listener`, that accept a route from the project and can
 * serve `host`. */
export function offered(gateways: Schemas["GatewayView"][], host: string): string[] {
  return gateways.flatMap((g) =>
    g.listeners
      .filter((l) => listenerServes(l.hostname ?? undefined, host))
      .map((l) => `${g.namespace}/${g.name}/${l.name}`),
  )
}
