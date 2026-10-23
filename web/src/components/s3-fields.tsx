import { queryOptions, useQuery } from "@tanstack/react-query"
import type { ReactNode } from "react"

import { api, unwrap } from "@/api/client"
import { FormField } from "@/components/form-field"
import { useT } from "@/i18n"
import type { Frequency, TargetProblem } from "@/model/databases"
import { type Destination, fromDestination, manual, type S3Target } from "@/model/s3-destinations"
import { Input } from "@zeabur/design-system/components/input"
import { Select } from "@zeabur/design-system/components/select"

type S3 = S3Target
type Field = Exclude<keyof S3, "destination">

/** The cluster's S3 destinations, without their keys. */
export const s3DestinationsQuery = queryOptions({
  queryKey: ["cluster", "s3-destinations"],
  queryFn: async ({ signal }) =>
    unwrap(await api.GET("/api/v1/cluster/s3-destinations", { signal })),
  retryOnMount: false,
})

const MANUAL = "\u0000manual"

/** The cluster's S3 destinations, or typing the fields. Hidden when the cluster has none. Choosing
 * one fills the fields; the plan copies its keys. */
export function DestinationPicker({
  value,
  onPick,
}: {
  value?: string
  onPick: (destination: Destination | undefined) => void
}) {
  const t = useT()
  const found = useQuery(s3DestinationsQuery).data
  const items = found?.items ?? []
  if (items.length === 0 && value === undefined) return null
  if (found?.canUseKeys === "denied")
    return (
      <FormField label={t("s3d.pick")} hint={t("s3d.denied", found.namespace)}>
        <Select
          size="sm"
          disabled
          value={MANUAL}
          items={[{ value: MANUAL, label: t("s3d.manual") }]}
        />
      </FormField>
    )
  return (
    <FormField
      label={t("s3d.pick")}
      hint={value === undefined ? undefined : t("s3d.copied", value)}
    >
      <Select
        size="sm"
        value={value ?? MANUAL}
        onValueChange={(name) => onPick(items.find((d) => d.name === name))}
        items={[
          { value: MANUAL, label: t("s3d.manual") },
          ...items.map((d) => ({ value: d.name, label: d.name })),
        ]}
      />
    </FormField>
  )
}

/** Where backups go in S3-compatible storage, and its keys. `folder` is the folder's placeholder;
 * without it there is no folder field. `children` are more fields in the same grid. */
export function S3Fields({
  target,
  set,
  errors,
  folder,
  folderHint,
  children,
}: {
  target: S3
  set: (patch: Partial<S3>) => void
  errors: (problem: TargetProblem) => string[]
  folder?: string
  folderHint?: string
  children?: ReactNode
}) {
  const t = useT()
  const text = (key: Field, value: string) => (
    <Input
      size="sm"
      mono
      value={target[key]}
      placeholder={value}
      autoComplete="off"
      spellCheck={false}
      onChange={(event) => set({ [key]: event.target.value })}
    />
  )
  const copied = target.destination !== undefined
  return (
    <>
      <DestinationPicker
        value={target.destination}
        onPick={(d) => set(d ? fromDestination(d, folder) : manual())}
      />
      <FormField label={t("db.backup.endpoint")} errors={errors("endpoint")}>
        {text("endpoint", "https://s3.example.com")}
      </FormField>
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField label={t("db.backup.bucket")} errors={errors("bucket")}>
          {text("bucket", "")}
        </FormField>
        {folder === undefined ? null : (
          <FormField label={t("db.backup.path")} hint={folderHint}>
            {text("path", folder)}
          </FormField>
        )}
        {copied ? null : <KeyFields target={target} set={set} errors={errors} text={text} />}
        <FormField label={t("db.backup.region")} hint={t("db.backup.regionHint")}>
          {text("region", "")}
        </FormField>
        {children}
      </div>
    </>
  )
}

function KeyFields({
  target,
  set,
  errors,
  text,
}: {
  target: S3
  set: (patch: Partial<S3>) => void
  errors: (problem: TargetProblem) => string[]
  text: (key: Field, value: string) => ReactNode
}) {
  const t = useT()
  return (
    <>
      <FormField label={t("db.backup.accessKey")} errors={errors("keys")}>
        {text("accessKey", "")}
      </FormField>
      <FormField label={t("db.backup.secretKey")}>
        <Input
          size="sm"
          mono
          type="password"
          value={target.secretKey}
          autoComplete="new-password"
          onChange={(event) => set({ secretKey: event.target.value })}
        />
      </FormField>
    </>
  )
}

/** How often backups run. */
export function FrequencyField({
  value,
  onChange,
}: {
  value: Frequency
  onChange: (frequency: Frequency) => void
}) {
  const t = useT()
  return (
    <FormField label={t("db.backup.frequency")}>
      <Select
        size="sm"
        value={value}
        onValueChange={(frequency) => onChange(frequency as Frequency)}
        items={[
          { value: "daily", label: t("db.backup.daily") },
          { value: "weekly", label: t("db.backup.weekly") },
        ]}
      />
    </FormField>
  )
}
