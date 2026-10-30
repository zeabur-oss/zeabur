// A project restored from a Velero backup, what comes after it (its domains, then the addresses of
// a copy), and the values in a copy that still name the original: in-cluster addresses such as
// `api.blog.svc` and `api.blog.svc.cluster.local`.
import type { Schemas } from "@/api/client"
import type { Target } from "@/model/connect"

type Change = Schemas.Change
type Env = Target["containers"][number]["env"][number]

type Project = { name: string; adopted?: boolean; system?: boolean }

type Restore = Pick<Schemas.VeleroRestore, "backup" | "namespaces" | "into" | "resources">

/** What follows a project restore: open the project, bring its domains back from the backup, and,
 * for a copy, rewrite the addresses of the original. Nothing follows a restore of only domains. */
export function afterRestore(restore: Restore):
  | {
      project: string
      attach: Extract<Schemas.BackupsPlanRequest, { op: "attachDomains" }>
      rewrite?: { source: string; into: string }
    }
  | undefined {
  const source = restore.namespaces[0]
  if (!source || (restore.resources?.length ?? 0) > 0) return undefined
  const project = restore.into ?? source
  return {
    project,
    attach: { op: "attachDomains", backup: restore.backup, namespace: source, to: project },
    ...(restore.into ? { rewrite: { source, into: restore.into } } : {}),
  }
}

/** The projects a backup can restore: those it names, or, when it names none, every project
 * Zeabur manages and the deleted projects it holds. */
export function restoreChoices(
  named: readonly string[],
  projects: readonly Project[],
  deleted: readonly string[] = [],
): string[] {
  if (named.length > 0) return [...named].sort()
  const current = projects.filter((p) => p.adopted && !p.system).map((p) => p.name)
  return [...new Set([...current, ...deleted])].sort()
}

/** What the backup's restore button does: make a copy when every project it names exists,
 * restore under the original name when every one is gone, else let the person choose. */
export function restoreEntry(
  named: readonly string[],
  projects: readonly Project[],
): "copy" | "sameName" | "choose" {
  const exists = named.map((name) => projects.some((p) => p.name === name))
  if (exists.length === 0) return "choose"
  if (exists.every(Boolean)) return "copy"
  return exists.some(Boolean) ? "choose" : "sameName"
}

/** Why the restore cannot be planned yet: no project chosen, or a new name that is taken. A
 * project that is gone restores under its own name, so it needs no new one. */
export function restoreProblem(
  namespace: string,
  into: string,
  projects: readonly Project[],
): "restoreNone" | "intoTaken" | undefined {
  if (!namespace) return "restoreNone"
  const taken = (name: string) => projects.some((p) => p.name === name)
  if (!taken(namespace)) return undefined
  return !into || into === namespace || taken(into) ? "intoTaken" : undefined
}

/** The Secrets of a project to look in: not those a controller owns, which it writes back, nor
 * Helm's release records. */
export function ownSecrets(objects: { name: string; ownerKind?: string | null }[]): string[] {
  return objects
    .filter((o) => !o.ownerKind && !o.name.startsWith("sh.helm.release.v1."))
    .map((o) => o.name)
    .toSorted()
}

const needle = (source: string) => `.${source}.svc`

/** `value` with each address in `source` moved to `into`, or undefined when it names none. */
export function rewritten(value: string, source: string, into: string): string | undefined {
  return value.includes(needle(source)) ? value.replaceAll(needle(source), needle(into)) : undefined
}

function envChanges(targets: Target[], source: string, into: string): Change[] {
  const edit = (env: Env) => {
    if (env.source.kind !== "Literal" || !env.value) return undefined
    const value = rewritten(env.value, source, into)
    return value === undefined ? undefined : { op: "set" as const, name: env.name, value }
  }
  return targets.flatMap((target) =>
    target.containers.flatMap((container): Change[] => {
      const changes = container.env.map(edit).filter((e) => e !== undefined)
      if (changes.length === 0) return []
      return [
        {
          op: "env",
          workload: { kind: target.kind, name: target.name },
          ...(target.containers.length > 1 ? { container: container.name } : {}),
          changes,
        },
      ]
    }),
  )
}

function dataChanges(
  kind: "ConfigMap" | "Secret",
  objects: { name: string; data: Record<string, string> }[],
  source: string,
  into: string,
): Change[] {
  return objects.flatMap((object): Change[] => {
    const changes = Object.entries(object.data).flatMap(([key, value]) => {
      const next = rewritten(value, source, into)
      return next === undefined ? [] : [{ op: "set", key, value: next }]
    })
    return changes.length === 0 ? [] : [{ op: "data", kind, name: object.name, changes }]
  })
}

/** The changes in the copy `into` that make its variables, ConfigMaps and Secrets name `into`
 * instead of `source`. */
export function rewriteChanges(
  source: string,
  into: string,
  targets: Target[],
  configMaps: { name: string; data: Record<string, string> }[],
  secrets: { name: string; data: Record<string, string> }[],
): Change[] {
  return [
    ...envChanges(targets, source, into),
    ...dataChanges("ConfigMap", configMaps, source, into),
    ...dataChanges("Secret", secrets, source, into),
  ]
}
