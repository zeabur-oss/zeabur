import type { PublicRoute } from "@/model/service"

/** Distinct host names, and how many of them serve plain HTTP. */
export function domainsLead(routes: PublicRoute[]): { hosts: number; plain: number } {
  const hosts = new Map<string, boolean>()
  for (const route of routes)
    if (route.host) hosts.set(route.host, (hosts.get(route.host) ?? true) && route.https)
  return { hosts: hosts.size, plain: [...hosts.values()].filter((https) => !https).length }
}

const isPlain = (route: PublicRoute) => Boolean(route.host) && !route.https

/** Hosts without HTTPS first; otherwise in the order the routes list them. */
export const plainFirst = (routes: PublicRoute[]) =>
  routes.toSorted((a, b) => Number(isPlain(b)) - Number(isPlain(a)))

export function filterRoutes(routes: PublicRoute[], query: string): PublicRoute[] {
  const needle = query.trim().toLowerCase()
  if (!needle) return routes
  return routes.filter((route) =>
    [route.host, route.path].some((text) => text?.toLowerCase().includes(needle)),
  )
}
