// Rows this tab just created enter once; rows that load or poll in appear still.
import type { Schemas } from "@/api/client"

type Workload = Schemas.WorkloadRef

const FRESH_MS = 10_000
const created = new Map<string, number>()

export const createdKey = {
  env: (workload: Workload, name: string) => `env:${workload.kind}/${workload.name}:${name}`,
  domain: (hostname: string) => `domain:${hostname}`,
  volume: (workload: Workload, mountPath: string) =>
    `volume:${workload.kind}/${workload.name}:${mountPath}`,
}

export function markCreated(changes: Schemas.Change[], now = Date.now()) {
  for (const [key, at] of created) if (now - at > FRESH_MS) created.delete(key)
  for (const change of changes) {
    if (change.op === "env")
      for (const edit of change.changes)
        if (edit.op === "set") created.set(createdKey.env(change.workload, edit.name), now)
    if (change.op === "addDomain") created.set(createdKey.domain(change.hostname), now)
    if (change.op === "addVolume")
      created.set(createdKey.volume(change.workload, change.volume.mountPath), now)
  }
}

/** The entrance for a row this tab just created; an element already on the page ignores it. */
export function entrance(key: string, now = Date.now()): string | undefined {
  const at = created.get(key)
  return at !== undefined && now - at <= FRESH_MS ? "swap-in" : undefined
}
