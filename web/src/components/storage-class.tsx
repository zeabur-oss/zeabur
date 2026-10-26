import { useQuery } from "@tanstack/react-query"

import { storageQuery } from "@/api/queries"
import { AmountInput } from "@/components/amount-input"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { permission, Refused } from "@/components/refused"
import { useT, type Translate } from "@/i18n"
import {
  canGrow,
  classNamed,
  offeredClasses,
  type Placement,
  type ProjectStorage,
  type StorageClass,
} from "@/model/storage"
import { Select } from "@zeabur/design-system/components/select"

const CLASSES = "storageclasses.storage.k8s.io"

export const useStorage = (project: string) => useQuery(storageQuery(project)).data

const classesRefused = (storage: ProjectStorage | undefined) =>
  storage?.unreadable.some((u) => u.resource === CLASSES) ?? false

export function kindWord(t: Translate, c: StorageClass): string {
  if (c.kind === "local") return t("storage.kind.local")
  if (c.kind === "replicated") return t("storage.kind.replicated")
  return t("storage.kind.other", c.provisioner)
}

function aboutWords(t: Translate, c: StorageClass): string {
  if (c.kind === "local") return t("storage.about.local")
  if (c.kind === "replicated") return t("storage.about.replicated")
  return t("storage.about.other")
}

/** Whether a new volume on `name` can grow, as a sentence under its size. */
export function growthWords(t: Translate, storage: ProjectStorage | undefined, name?: string) {
  const grows = canGrow(storage, name)
  if (grows === undefined) return t("storage.growUnknown")
  return grows ? t("storage.grows") : t("storage.fixedSize")
}

/** Whether an existing volume on class `name` can grow, and a sentence that says so. */
export function useGrowth(project: string, name: string | null | undefined) {
  const t = useT()
  const storage = useStorage(project)
  const storageClass = classNamed(storage, name)
  const growable = storage?.classes.some((c) => c.allowExpansion) ?? false
  const fixed = storageClass?.allowExpansion === false
  const description =
    storageClass === undefined
      ? t("storage.growUnknown")
      : fixed
        ? t("storage.fixed", storageClass.name, growable)
        : t("storage.grows")
  return { storageClass, fixed, description }
}

export function placementWords(t: Translate, placement: Placement): string {
  switch (placement.kind) {
    case "node":
      return t("storage.on.node", placement.node)
    case "pinned":
      return t("storage.on.pinned")
    case "firstUse":
      return t("storage.on.firstUse")
    case "moves":
      return t("storage.on.moves")
  }
}

/** A class's fields, each after its plain words, for a section's Kubernetes details. */
export function classFacts(t: Translate, c: StorageClass): [string, string][] {
  const reclaim =
    c.reclaimPolicy === "Delete" || c.reclaimPolicy === "Retain"
      ? t(`storage.reclaim.${c.reclaimPolicy}`)
      : c.reclaimPolicy
  const binding =
    c.bindingMode === "WaitForFirstConsumer" || c.bindingMode === "Immediate"
      ? t(`storage.binding.${c.bindingMode}`)
      : c.bindingMode
  return [
    [kindWord(t, c), `StorageClass/${c.name} provisioner: ${c.provisioner}`],
    [
      t(c.allowExpansion ? "storage.canGrow" : "storage.cannotGrow"),
      `allowVolumeExpansion: ${c.allowExpansion}`,
    ],
    [reclaim, `reclaimPolicy: ${c.reclaimPolicy}`],
    [binding, `volumeBindingMode: ${c.bindingMode}`],
  ]
}

/** Where a new volume goes: the cluster's StorageClasses, the default first, each in plain words.
 * When they may not be listed, the volume goes on the default and the refusal says so. */
export function StorageClassField({
  storage,
  value,
  onChange,
  engine,
  details = true,
  growth = false,
  disabled,
}: {
  storage: ProjectStorage | undefined
  /** The chosen class; `undefined` for the default. */
  value: string | undefined
  onChange: (name: string) => void
  /** A database engine whose instances copy its data themselves: local disks suit it. */
  engine?: string
  /** Show the class's Kubernetes fields here; off where the section has its own details. */
  details?: boolean
  /** Say whether a volume on the chosen class can grow later. */
  growth?: boolean
  disabled?: boolean
}) {
  const t = useT()
  if (classesRefused(storage))
    return (
      <div className="flex flex-col gap-1.5">
        <Refused
          refusal="unreadable"
          what={t("storage.choosing")}
          missing={[permission("list", CLASSES)]}
          line
        />
        <p className="type-meta text-secondary">
          {growth
            ? t("common.sentences", [t("storage.onDefault"), t("storage.growUnknown")])
            : t("storage.onDefault")}
        </p>
      </div>
    )
  const classes = offeredClasses(storage)
  const chosen = classNamed(storage, value)
  if (classes.length === 0 || !chosen) return null
  const hint = t("common.sentences", [
    aboutWords(t, chosen),
    growth ? growthWords(t, storage, value) : undefined,
    engine && classes.length > 1 ? t("storage.selfReplicating", engine) : undefined,
  ])
  return (
    <div className="flex flex-col gap-2">
      <FormField label={t("storage.class")} hint={hint}>
        <Select
          size="sm"
          value={chosen.name}
          disabled={disabled || classes.length < 2}
          onValueChange={onChange}
          items={classes.map((c) => ({
            value: c.name,
            label: t("storage.option", kindWord(t, c), c.name, c.default),
          }))}
        />
      </FormField>
      {details ? <KubeDetails facts={classFacts(t, chosen)} /> : null}
    </div>
  )
}

/** A volume's size, with what its class allows later. */
export function StorageSizeField({
  label,
  storage,
  storageClass,
  value,
  bad,
  onChange,
}: {
  label: string
  storage: ProjectStorage | undefined
  storageClass?: string
  value: string
  bad: boolean
  onChange: (value: string) => void
}) {
  const t = useT()
  return (
    <FormField
      label={label}
      hint={growthWords(t, storage, storageClass)}
      errors={bad ? [t("storage.problem.badSize")] : []}
    >
      <AmountInput dimension="bytes" value={value} onChange={onChange} />
    </FormField>
  )
}
