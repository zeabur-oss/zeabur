import type { Schemas } from "@/api/client"

/** What kubectl does for the same plan. */
export function kubectlCommand(action: Schemas["NodeAction"], node: string): string {
  return action === "cordon" ? `kubectl cordon ${node}` : `kubectl uncordon ${node}`
}
