// The port a new service listens on, from what its image declares (`EXPOSE`), read by the API
// from the registry. The person's own value always wins.
import type { Schemas } from "@/api/client"

type Inspection = Schemas["ImageInspection"]

/** The TCP ports the image declares, lowest first. */
export function tcpPorts(inspection: Inspection | undefined): number[] {
  return (inspection?.exposedPorts ?? []).filter((p) => p.protocol === "TCP").map((p) => p.port)
}

/** The port field's value: what the person typed (`undefined` until they do), else the image's
 * first TCP port, else empty. */
export function shownPort(typed: string | undefined, inspection: Inspection | undefined): string {
  if (typed !== undefined) return typed
  const first = tcpPorts(inspection)[0]
  return first === undefined ? "" : String(first)
}

/** What to say under the port field about the image; `undefined` keeps the usual hint. */
export type PortNote =
  | { kind: "read"; port: number }
  | { kind: "choose"; ports: number[] }
  | { kind: "differs"; ports: number[] }
  | { kind: "unreadable"; reason: Schemas["UnreadableReason"]; detail?: string }

export function portNote(port: string, inspection: Inspection | undefined): PortNote | undefined {
  if (!inspection) return undefined
  if (inspection.unreadable)
    return {
      kind: "unreadable",
      reason: inspection.unreadable,
      detail: inspection.detail ?? undefined,
    }
  const ports = tcpPorts(inspection)
  if (ports.length === 0) return undefined
  if (!ports.includes(Number(port.trim()))) return { kind: "differs", ports }
  const [only] = ports
  return ports.length === 1 && only !== undefined
    ? { kind: "read", port: only }
    : { kind: "choose", ports }
}
