import type { Schemas } from "@/api/client"
import { StatusDot } from "@/components/status"
import { useT, type Translate } from "@/i18n"

/** What kube-apiserver warned about one write, in its own words. */
export function KubeWarnings({ warnings }: { warnings?: string[] }) {
  const t = useT()
  if (!warnings?.length) return null
  return (
    <ul className="flex flex-col gap-1" aria-label={t("plan.kubeWarnings")}>
      {warnings.map((warning) => (
        <li key={warning} className="flex items-start gap-2 text-sm">
          <StatusDot health="warn" className="mt-1.5" />
          <span className="min-w-0 break-words">{warning}</span>
        </li>
      ))}
    </ul>
  )
}

/** Each warning of each object, with the object it is about. */
export function objectWarnings(t: Translate, objects: readonly Schemas.PlannedObject[]): string[] {
  return objects.flatMap((object) =>
    (object.warnings ?? []).map((warning) =>
      t("plan.kubeWarning", `${object.target.kind}/${object.target.name}`, warning),
    ),
  )
}
