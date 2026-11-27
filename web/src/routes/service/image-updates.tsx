import { useQuery } from "@tanstack/react-query"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"
import { Select } from "@zeabur-oss/design-system/components/select"
import { useState } from "react"

import type { Schemas } from "@/api/client"
import { capabilitiesQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { GateNote, useGate } from "@/components/gate"
import { InstallAlert } from "@/components/lazy-installer-sheet"
import { useHold } from "@/components/plan/draft"
import { useI18n, useT } from "@/i18n"
import { missingKeel } from "@/model/git-service"
import {
  dayName,
  formOf,
  keptPolicy,
  policyChoices,
  policyHint,
  localMinute,
  tagOf,
  toUtc,
  updateOf,
  windowText,
  type UpdateForm,
  type When,
} from "@/model/image-updates"
import type { Service } from "@/model/service"

const hours = Array.from({ length: 24 }, (_, h) => h)
const days = [1, 2, 3, 4, 5, 6, 0]
const whens: readonly When[] = ["hourly", "daily", "weekly"]

/** Which new versions of its image a service not built from source takes on its own, and when. */
export function ImageUpdatesSection({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  const capabilities = useQuery({ ...capabilitiesQuery, retry: false })
  const missing = missingKeel(capabilities.data?.items)
  const updates = service.imageUpdates ?? undefined
  if (!updates) return null
  const managed = service.provenance.tool ?? undefined
  return (
    <section aria-labelledby="updates-title" className="flex flex-col gap-3">
      <SectionHeader
        id="updates-title"
        title={t("part.updates")}
        description={t("imageUpdates.hint")}
      />
      {managed ? (
        <p className="type-meta text-muted-foreground">{t("builds.managedDeploy", managed)}</p>
      ) : (
        <GateNote gate={gate} />
      )}
      {missing ? <KeelMissing install={missing.install} /> : null}
      {updates.allowed.length === 0 ? (
        <p className="type-meta text-muted-foreground">{t("imageUpdates.pinned")}</p>
      ) : null}
      {updates.allowed.length === 0 && !updates.setting ? null : (
        <UpdatesForm
          service={service}
          updates={updates}
          disabled={!!managed || !gate.open || (!!missing && !updates.setting)}
        />
      )}
    </section>
  )
}

export function KeelMissing({ install }: { install?: Schemas.ComponentRef }) {
  const t = useT()
  return install ? (
    <InstallAlert variant="info" text={t("imageUpdates.keelMissing")} components={[install]} />
  ) : (
    <Alert variant="info">
      <AlertDescription>{t("imageUpdates.keelUnavailable")}</AlertDescription>
    </Alert>
  )
}

function UpdatesForm({
  service,
  updates,
  disabled,
}: {
  service: Service
  updates: Schemas.ImageUpdates
  disabled: boolean
}) {
  const [now] = useState(() => new Date())
  const [initial] = useState(() => formOf(updates, now))
  const [form, setForm] = useState(initial)
  const set = (patch: Partial<UpdateForm>) => setForm((f) => ({ ...f, ...patch }))
  const workload = { kind: service.kind, name: service.name }
  const update = updateOf(updates, form, now)
  const edited =
    !disabled && JSON.stringify(update) !== JSON.stringify(updateOf(updates, initial, now))
  useHold(
    "updates",
    edited && update
      ? {
          changes: [{ op: "autoUpdate", workload, update }],
          scope: { ops: ["autoUpdate"], workload },
        }
      : undefined,
  )
  return (
    <div className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5">
      <PolicyField
        image={service.containers.find((c) => !c.init)?.image ?? ""}
        updates={updates}
        form={form}
        disabled={disabled}
        set={set}
      />
      {form.policy === "off" || form.policy === "other" ? null : (
        <WhenFields form={form} now={now} disabled={disabled} set={set} />
      )}
    </div>
  )
}

export function PolicyField({
  image,
  hint,
  updates,
  form,
  disabled,
  set,
}: {
  image: string
  hint?: string
  updates: Schemas.ImageUpdates
  form: UpdateForm
  disabled: boolean
  set: (patch: Partial<UpdateForm>) => void
}) {
  const t = useT()
  const tag = tagOf(image)
  const foreign = keptPolicy(updates)
  return (
    <FormField label={t("imageUpdates.versions")} hint={hint ?? policyHint(t, updates, form, tag)}>
      <Select
        size="sm"
        value={form.policy}
        disabled={disabled}
        onValueChange={(policy) => set({ policy: policy as UpdateForm["policy"] })}
        items={[
          ...(foreign ? [{ value: "other", label: t("imageUpdates.policy.other", foreign) }] : []),
          { value: "off", label: t("imageUpdates.policy.off") },
          ...policyChoices(updates).map((p) => ({
            value: p,
            label: t(`imageUpdates.policy.${p}`),
          })),
        ]}
      />
    </FormField>
  )
}

/** When Keel looks: every hour, or a day and hour of the browser's, with the UTC time below. */
export function WhenFields({
  form,
  now,
  disabled,
  set,
}: {
  form: UpdateForm
  now: Date
  disabled: boolean
  set: (patch: Partial<UpdateForm>) => void
}) {
  const { t, locale } = useI18n()
  const window = toUtc(form, now)
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-4 sm:flex-row">
        <FormField label={t("imageUpdates.when")}>
          <Select
            size="sm"
            value={form.when}
            disabled={disabled}
            onValueChange={(when) => set({ when: when as When })}
            items={whens.map((w) => ({
              value: w,
              label: t(`imageUpdates.when.${w}`),
            }))}
          />
        </FormField>
        {form.when === "weekly" ? (
          <FormField label={t("imageUpdates.day")}>
            <Select
              size="sm"
              value={String(form.day)}
              disabled={disabled}
              onValueChange={(day) => set({ day: Number(day) })}
              items={days.map((d) => ({
                value: String(d),
                label: dayName(d, locale),
              }))}
            />
          </FormField>
        ) : null}
        {form.when === "hourly" ? null : (
          <FormField label={t("imageUpdates.hour")}>
            <Select
              size="sm"
              value={String(form.hour)}
              disabled={disabled}
              onValueChange={(hour) => set({ hour: Number(hour) })}
              items={hours.map((h) => ({
                value: String(h),
                label: `${String(h).padStart(2, "0")}:${String(localMinute(now)).padStart(2, "0")}`,
              }))}
            />
          </FormField>
        )}
      </div>
      <p className="type-body text-secondary">
        {window ? t("imageUpdates.utc", windowText(t, window)) : t("imageUpdates.hourlyHint")}
      </p>
    </div>
  )
}
