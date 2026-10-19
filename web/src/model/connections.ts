// Edges between a project's parts, with the evidence for each. Service-to-service edges come from
// the server (`Project.connections`, computed in engawa-graph from Kubernetes objects and variable
// values); domain-to-service edges are read here from each service's Ingress routes. When the
// project document carries NetworkPolicy reachability, each edge says whether the policies let it
// through (whether they are enforced depends on the cluster's network plugin).
import type { Schemas } from "@/api/client"
import { domainsOf, serviceId, type Project, type Service } from "@/model/service"

export type Evidence =
  | { basis: "route"; host: string; ingress: string }
  | Schemas["ConnectionEvidence"]

export interface Edge {
  id: string
  /** `Kind/name`, or `domain:<host>`. */
  from: string
  to: string
  /** True when every piece of evidence is a variable value: drawn dashed. */
  inferred: boolean
  evidence: Evidence[]
  /** What the project's NetworkPolicy objects allow; absent when none was read. */
  access?: Schemas["NetworkPolicyAccess"]
}

type Document = Project & { extensions?: Schemas["ProjectExtensions"] }

const domainNode = (host: string) => `domain:${host}`

/** Every edge of the project: domains to services, then services to services. */
export function edgesOf(project: Document): Edge[] {
  const reach = project.extensions?.networkPolicy
  // The reachability speaks of API ids (`projects/shop/services/deployment/web`); edges of
  // `Kind/name`.
  const byApiId = new Map(project.services.map((s) => [s.id, serviceId(s)]))
  const fromIngress = new Map(
    reach?.services.map((s) => [byApiId.get(s.service), s.fromIngressController]) ?? [],
  )
  const between = new Map(
    reach?.connections.map((c) => [`${byApiId.get(c.from)}->${byApiId.get(c.to)}`, c.access]) ?? [],
  )
  const edges: Edge[] = []
  for (const service of project.services)
    for (const domain of domainsOf(project, service))
      edges.push({
        id: `${domainNode(domain.host)}->${serviceId(service)}`,
        from: domainNode(domain.host),
        to: serviceId(service),
        inferred: false,
        evidence: [{ basis: "route", host: domain.host, ingress: domain.ingress }],
        access: fromIngress.get(serviceId(service)),
      })
  for (const connection of project.connections) {
    const from = serviceId(connection.from)
    const to = serviceId(connection.to)
    const id = `${from}->${to}`
    edges.push({
      id,
      from,
      to,
      inferred: connection.inferred,
      evidence: connection.evidence,
      access: between.get(id),
    })
  }
  return edges
}

/** Upstream (edges into the service) and downstream (edges out of it). */
export function neighboursOf(edges: Edge[], service: Service) {
  const id = serviceId(service)
  return {
    upstream: edges.filter((e) => e.to === id),
    downstream: edges.filter((e) => e.from === id),
  }
}
