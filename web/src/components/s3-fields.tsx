import { queryOptions, useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useEffect, useRef, type ReactNode } from "react"

import * as zeabur from "@/api/zeabur"
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
  queryFn: async ({ signal }) => await zeabur.getS3Destinations({ signal }),
  retryOnMount: false,
})

const MANUAL = "\u0000manual"

/** The cluster's S3 destinations, or typing the fields. A blank form starts on the first
 * destination. Choosing one fills the fields; the plan copies its keys. */
export function DestinationPicker({
  value,
  blank,
  onPick,
}: {
  value?: string
  /** Nothing is typed yet, so a destination may fill the form. */
  blank: boolean
  onPick: (destination: Destination | undefined) => void
}) {
  const t = useT()
  const found = useQuery(s3DestinationsQuery).data
  const items = found?.items ?? []
  const usable = found !== undefined && found.canUseKeys !== "denied"
  const started = useFirstPick(
    usable && blank && value === undefined ? items[0] : undefined,
    onPick,
  )
  if (items.length === 0 && value === undefined)
    return found?.canChange === "allowed" ? <NoDestinations /> : null
  if (!usable)
    return (
      <FormField label={t("s3d.pick")} hint={found && t("s3d.denied", found.namespace)}>
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
        onValueChange={(name) => {
          started.current = true
          onPick(items.find((d) => d.name === name))
        }}
        items={[
          ...items.map((d) => ({ value: d.name, label: d.name })),
          { value: MANUAL, label: t("s3d.manual") },
        ]}
      />
    </FormField>
  )
}

/** Picks `first` once. The returned ref is set when the person picks, so no later pick follows. */
function useFirstPick(
  first: Destination | undefined,
  onPick: (destination: Destination | undefined) => void,
) {
  const started = useRef(false)
  useEffect(() => {
    if (!first || started.current) return
    started.current = true
    onPick(first)
  }, [first, onPick])
  return started
}

function NoDestinations() {
  const t = useT()
  return (
    <p className="type-meta text-muted-foreground">
      {t("s3d.none")}
      {t("common.gap")}
      <Link
        to="/cluster/backups"
        hash="s3-destinations"
        className="text-foreground hover:underline"
      >
        {t("s3d.add")}
      </Link>
    </p>
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
        blank={!target.endpoint && !target.bucket && !target.accessKey}
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
