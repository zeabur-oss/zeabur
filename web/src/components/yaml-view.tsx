// An object's YAML that can be edited: read-only until Edit, then a YAML editor (loaded on first
// use) whose text goes to a plan like every other write. A Secret's values arrive hidden; left
// hidden they are kept as they are, never written as the mask. An object a tool manages stays
// read-only, with where to change it instead. Where the host offers it, Delete plans the
// object's deletion, confirmed by typing its name.
import { lazy, Suspense, useState } from "react"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import TrashIcon from "~icons/ph/trash"

import type { ObjectAddress } from "@/api/queries"
import { usePlanner } from "@/components/plan/planner"
import { Button } from "@engawahq/design-system/components/button"
import { Skeleton } from "@engawahq/design-system/components/skeleton"
import { Tooltip } from "@engawahq/design-system/components/tooltip"
import { YamlBlock } from "@/components/yaml"
import { useT } from "@/i18n"

const YamlEditor = lazy(() =>
  import("@/components/yaml-editor").then((m) => ({ default: m.YamlEditor })),
)

/** Kinds Kubernetes makes on its own or that a plan cannot edit as a whole. */
const readOnlyResources = new Set(["pods", "events", "replicasets", "controllerrevisions"])

/** Kinds Kubernetes makes on its own, which come back when deleted. A pod is deleted to be
 * replaced; the server refuses an object a tool or a controller would make again. */
const undeletableResources = new Set([
  "events",
  "replicasets",
  "controllerrevisions",
  "endpoints",
  "endpointslices",
])

export function YamlView({
  project,
  address,
  yaml,
  managedBy,
  kind,
  onDeleted,
}: {
  project: string
  address: ObjectAddress
  yaml: string
  /** The tool that manages the object (Helm, Argo CD, Flux), if any. */
  managedBy?: string | null
  /** The object's kind, for the deletion's title. */
  kind?: string
  /** Offers Delete, and runs once the object is deleted. */
  onDeleted?: () => void
}) {
  const t = useT()
  const [draft, setDraft] = useState<string | null>(null)
  const planner = usePlanner(project, () => setDraft(null))
  const deleter = usePlanner(project, onDeleted)
  const label = t("yaml.label", address.name)
  const blocked = managedBy
    ? t("common.managedByDetail", managedBy)
    : readOnlyResources.has(address.resource)
      ? t("yaml.notEditable")
      : undefined
  const undeletable = managedBy
    ? t("common.managedByDetail", managedBy)
    : undeletableResources.has(address.resource)
      ? t("yaml.notDeletable")
      : undefined
  const planDeletion = () =>
    deleter.plan({
      title: t("title.delete", kind ?? address.resource, address.name),
      changes: [
        {
          op: "delete",
          resource: { group: address.group, version: address.version, resource: address.resource },
          name: address.name,
        },
      ],
    })

  if (draft === null)
    return (
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          {address.resource === "secrets" ? (
            <p className="type-meta text-muted-foreground">{t("yaml.hidden")}</p>
          ) : null}
          <span className="ml-auto flex gap-1.5">
            {onDeleted ? (
              undeletable ? (
                <Tooltip content={undeletable}>
                  <span>
                    <Button variant="outline" size="xs" disabled>
                      <TrashIcon aria-hidden="true" data-icon="inline-start" />
                      {t("yaml.delete")}
                    </Button>
                  </span>
                </Tooltip>
              ) : (
                <Button variant="outline" size="xs" onClick={planDeletion}>
                  <TrashIcon aria-hidden="true" data-icon="inline-start" />
                  {t("yaml.delete")}
                </Button>
              )
            ) : null}
            {blocked ? (
              <Tooltip content={blocked}>
                <span>
                  <Button variant="outline" size="xs" disabled>
                    <PencilSimpleIcon aria-hidden="true" data-icon="inline-start" />
                    {t("yaml.edit")}
                  </Button>
                </span>
              </Tooltip>
            ) : (
              <Button variant="outline" size="xs" onClick={() => setDraft(yaml)}>
                <PencilSimpleIcon aria-hidden="true" data-icon="inline-start" />
                {t("yaml.edit")}
              </Button>
            )}
          </span>
        </div>
        <YamlBlock yaml={yaml} label={label} />
        {deleter.dialog}
      </div>
    )

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        {address.resource === "secrets" ? (
          <p className="type-meta text-muted-foreground">{t("yaml.editSecret")}</p>
        ) : null}
        <span className="ml-auto flex gap-1.5">
          <Button variant="ghost" size="xs" onClick={() => setDraft(null)}>
            {t("edit.cancel")}
          </Button>
          <Button
            size="xs"
            disabled={draft === yaml}
            onClick={() =>
              planner.plan({
                title: t("title.editYaml", address.name),
                changes: [
                  {
                    op: "edit",
                    resource: {
                      group: address.group,
                      version: address.version,
                      resource: address.resource,
                    },
                    name: address.name,
                    yaml: draft,
                  },
                ],
              })
            }
          >
            {t("edit.review")}
          </Button>
        </span>
      </div>
      <Suspense fallback={<Skeleton variant="card" className="h-96 w-full" />}>
        <YamlEditor initial={yaml} label={label} onChange={setDraft} />
      </Suspense>
      {planner.dialog}
    </div>
  )
}
