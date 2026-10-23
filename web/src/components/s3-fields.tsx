import type { ReactNode } from "react"

import { FormField } from "@/components/form-field"
import { useT } from "@/i18n"
import type { BackupTarget, Frequency, TargetProblem } from "@/model/databases"
import { Input } from "@zeabur/design-system/components/input"
import { Select } from "@zeabur/design-system/components/select"

type S3 = Pick<BackupTarget, "endpoint" | "bucket" | "path" | "region" | "accessKey" | "secretKey">

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
  const text = (key: keyof S3, value: string) => (
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
  return (
    <>
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
        <FormField label={t("db.backup.region")} hint={t("db.backup.regionHint")}>
          {text("region", "")}
        </FormField>
        {children}
      </div>
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
