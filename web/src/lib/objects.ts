// Where an object lives in the objects API (`group/version/resource/name`, `core` for the empty
// group), for the kinds the projection names and for any object the Objects page lists.
import type { ObjectAddress } from "@/api/queries"

/** The API resource of each kind the projection names. */
const known: Record<string, Omit<ObjectAddress, "name">> = {
  Deployment: { group: "apps", version: "v1", resource: "deployments" },
  StatefulSet: { group: "apps", version: "v1", resource: "statefulsets" },
  DaemonSet: { group: "apps", version: "v1", resource: "daemonsets" },
  Job: { group: "batch", version: "v1", resource: "jobs" },
  CronJob: { group: "batch", version: "v1", resource: "cronjobs" },
  ConfigMap: { group: "", version: "v1", resource: "configmaps" },
  Secret: { group: "", version: "v1", resource: "secrets" },
  PersistentVolumeClaim: { group: "", version: "v1", resource: "persistentvolumeclaims" },
  Service: { group: "", version: "v1", resource: "services" },
  Ingress: { group: "networking.k8s.io", version: "v1", resource: "ingresses" },
  HTTPRoute: { group: "gateway.networking.k8s.io", version: "v1", resource: "httproutes" },
}

/** The address of an object of a kind the projection names; `undefined` for any other kind. */
export function addressOf(kind: string, name: string): ObjectAddress | undefined {
  const base = known[kind]
  return base ? { ...base, name } : undefined
}

/** `group/version/resource/name`, the Objects page's `?object=`. */
export function objectKey(address: ObjectAddress): string {
  return [address.group || "core", address.version, address.resource, address.name].join("/")
}

/** The address in `?object=`, or in an object's id (`projects/<p>/objects/<key>`). */
export function parseObjectKey(key: string): ObjectAddress | undefined {
  const parts = key.replace(/^projects\/[^/]+\/objects\//, "").split("/")
  if (parts.length !== 4 || parts.some((part) => !part)) return undefined
  const [group, version, resource, name] = parts as [string, string, string, string]
  return { group: group === "core" ? "" : group, version, resource, name }
}
