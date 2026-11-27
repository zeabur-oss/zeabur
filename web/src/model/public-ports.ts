import type { Schemas } from "@/api/client"
import { rules } from "@/api/rules"
import type { Endpoint } from "@/model/service"

const EXTERNAL = rules["PublicPortChange.external"]

/** Whether the Service is one that a public port made, and so can be closed. */
export const isOpened = (e: Endpoint): boolean => !!e.opens

/** A Service port that a public port can open. */
export interface PortChoice {
  value: string
  label: string
  from: string
  port: number
}

/** The ports of `endpoints` that no public port in `open` opens yet. */
export function portChoices(endpoints: Endpoint[], open: Endpoint[]): PortChoice[] {
  const isOpen = (from: string, port: number) =>
    open.some((e) => e.opens?.service === from && e.opens.port === port)
  return endpoints.flatMap((e) =>
    e.ports
      .filter((p) => !isOpen(e.name, p.port))
      .map((p) => ({
        value: `${e.name}/${p.port}`,
        label: `${e.name}:${p.port} ${p.protocol}`,
        from: e.name,
        port: p.port,
      })),
  )
}

/** The external port typed in `text`: `undefined` when empty, `NaN` when it is not a port. */
export function externalOf(text: string): number | undefined {
  const trimmed = text.trim()
  if (trimmed === "") return undefined
  const port = /^\d+$/.test(trimmed) ? Number(trimmed) : Number.NaN
  return port >= EXTERNAL.minimum && port <= EXTERNAL.maximum ? port : Number.NaN
}

/** The change that opens `choice`, on `external` when it is another port. */
export function openPortChange(
  workload: Schemas.WorkloadRef,
  choice: PortChoice,
  external: number | undefined,
): Schemas.Change {
  return {
    op: "publicPort",
    workload,
    service: choice.from,
    port: choice.port,
    ...(external === undefined || external === choice.port ? {} : { external }),
  }
}
