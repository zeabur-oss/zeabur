import type { Schemas } from "@/api/client"

/** A tool that writes its fields again from its own source. */
export type Reapplier = Schemas.Tool | "kubectl"

export interface Skipped {
  name: string
  tool: Schemas.Tool
}

export type ToolNote =
  | { kind: "skipped"; tool: Schemas.Tool; names: string[] }
  | { kind: "putBack"; tool: Reapplier; object: string; paths: string[] }

/**
 * The tool behind a field manager, when it writes the field again: Helm 3 as `helm`, Flux as
 * `helm-controller` or `kustomize-controller`, Argo CD as `argocd-…`, and `kubectl apply` as
 * `kubectl-client-side-apply` or, server-side, `kubectl`.
 */
export function reapplierOf(manager: string): Reapplier | undefined {
  if (manager === "helm") return "Helm"
  if (manager === "helm-controller" || manager === "kustomize-controller") return "Flux"
  if (manager.startsWith("argocd")) return "Argo CD"
  if (manager === "kubectl-client-side-apply" || manager === "kubectl") return "kubectl"
  return undefined
}

/** What another tool does about a plan: the services it left out, the fields it will put back. */
export function toolNotes(
  plan: Pick<Schemas.PlanResponse, "objects">,
  skipped: Skipped[] = [],
): ToolNote[] {
  const notes: ToolNote[] = []
  const byTool = new Map<Schemas.Tool, string[]>()
  for (const s of skipped) byTool.set(s.tool, [...(byTool.get(s.tool) ?? []), s.name])
  for (const [tool, names] of byTool) notes.push({ kind: "skipped", tool, names })
  for (const object of plan.objects) {
    const paths = new Map<Reapplier, string[]>()
    for (const owner of object.owners) {
      const tools = new Set(owner.managers.flatMap((m) => reapplierOf(m) ?? []))
      for (const tool of tools) paths.set(tool, [...(paths.get(tool) ?? []), owner.path])
    }
    for (const [tool, owned] of paths)
      notes.push({ kind: "putBack", tool, object: object.target.name, paths: owned })
  }
  return notes
}
