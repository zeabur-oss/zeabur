import { useForm } from "@tanstack/react-form"
import type { ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { ReviewWord } from "@/components/plan/draft"
import type { Schemas } from "@/api/client"
import { Button } from "@zeabur/design-system/components/button"
import { Field, FieldDescription, FieldLabel } from "@zeabur/design-system/components/field"
import { Input } from "@zeabur/design-system/components/input"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Select } from "@zeabur/design-system/components/select"
import { Switch } from "@zeabur/design-system/components/switch"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { FormField, quantity } from "@/components/form-field"
import {
  accessModes,
  defaultClaimName,
  subdomain,
  volumeRequest,
  type VolumeValues,
} from "@/routes/service/volume"

export interface ClaimEdit {
  /** Set when the size grows. */
  size?: string
  /** Set when the path or read-only changes. */
  mount?: { mountPath: string; readOnly: boolean }
}

/** A volume's size, path and read-only switch, saved together through one plan. */
export function ClaimForm({
  mount,
  disabledReason,
  onSave,
}: {
  mount: Schemas["Mount"]
  /** Set when the volume cannot be changed here, and why. */
  disabledReason?: string
  onSave: (edit: ClaimEdit) => void
}) {
  const t = useT()
  const disabled = disabledReason !== undefined
  const form = useForm({
    defaultValues: {
      size: mount.size ?? "",
      mountPath: mount.mountPath,
      readOnly: mount.readOnly ?? false,
    },
    onSubmit: ({ value }) => {
      const size = value.size.trim()
      const mountPath = value.mountPath.trim()
      const moved = mountPath !== mount.mountPath || value.readOnly !== (mount.readOnly ?? false)
      onSave({
        size: size !== (mount.size ?? "") ? size : undefined,
        mount: moved ? { mountPath, readOnly: value.readOnly } : undefined,
      })
    },
  })
  const save = (
    <form.Subscribe selector={(s) => [s.canSubmit, s.isDirty] as const}>
      {([canSubmit, dirty]) => (
        <Button type="submit" size="sm" disabled={disabled || !canSubmit || !dirty}>
          <ReviewWord />
        </Button>
      )}
    </form.Subscribe>
  )
  return (
    <form
      className="flex flex-col gap-4 border-t pt-4"
      onSubmit={(event) => {
        event.preventDefault()
        void form.handleSubmit()
      }}
    >
      <form.Field
        name="size"
        validators={{
          onChange: ({ value }) =>
            quantity.test(value.trim()) ? undefined : t("form.badQuantity"),
        }}
      >
        {(f) => (
          <FormField label={t("form.size")} errors={f.state.meta.errors} hint={t("form.sizeHint")}>
            <Input
              size="sm"
              mono
              value={f.state.value}
              disabled={disabled}
              onChange={(e) => f.handleChange(e.target.value)}
            />
          </FormField>
        )}
      </form.Field>
      <form.Field
        name="mountPath"
        validators={{
          onChange: ({ value }) => (value.startsWith("/") ? undefined : t("form.badPath")),
        }}
      >
        {(f) => (
          <FormField label={t("storage.mountPath")} errors={f.state.meta.errors}>
            <Input
              size="sm"
              mono
              value={f.state.value}
              disabled={disabled}
              onChange={(e) => f.handleChange(e.target.value)}
            />
          </FormField>
        )}
      </form.Field>
      <form.Field name="readOnly">
        {(f) => (
          <SwitchRow
            label={t("form.readOnly")}
            hint={t("form.readOnlyHint")}
            checked={f.state.value}
            disabled={disabled}
            onCheckedChange={(checked) => f.handleChange(checked)}
          />
        )}
      </form.Field>
      <div className="flex justify-end">
        {disabledReason ? (
          <Tooltip content={disabledReason}>
            <span>{save}</span>
          </Tooltip>
        ) : (
          save
        )}
      </div>
    </form>
  )
}

function SwitchRow({
  label,
  hint,
  checked,
  disabled,
  onCheckedChange,
}: {
  label: string
  hint: string
  checked: boolean
  disabled: boolean
  onCheckedChange: (checked: boolean) => void
}) {
  return (
    <Field disabled={disabled}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex flex-col gap-0.5">
          <FieldLabel>{label}</FieldLabel>
          <FieldDescription>{hint}</FieldDescription>
        </div>
        <Switch checked={checked} onCheckedChange={onCheckedChange} />
      </div>
    </Field>
  )
}

/** Adds a volume; with no volume yet, it is also the section's empty state. */
export function AddVolume({
  workload,
  title,
  description,
  disabledReason,
  onPlan,
}: {
  workload: string
  title: ReactNode
  description?: ReactNode
  disabledReason?: string
  onPlan: (volume: Schemas["VolumeRequest"]) => void
}) {
  const t = useT()
  const disabled = disabledReason !== undefined
  const defaultValues: VolumeValues = {
    name: "",
    mountPath: "",
    size: "1Gi",
    claimName: null,
    accessMode: "ReadWriteOnce",
  }
  const form = useForm({
    defaultValues,
    onSubmit: ({ value, formApi }) => {
      onPlan(volumeRequest(workload, value))
      formApi.reset()
    },
  })
  return (
    <form
      aria-label={t("form.addVolume")}
      className="flex flex-col gap-4 rounded-xl border border-dashed p-4"
      onSubmit={(event) => {
        event.preventDefault()
        void form.handleSubmit()
      }}
    >
      <div className="flex flex-col gap-1">
        <h3 className="type-body font-medium">{title}</h3>
        {description ? <p className="type-body text-secondary">{description}</p> : null}
      </div>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <form.Field
          name="name"
          validators={{
            onChange: ({ value }) =>
              value === "" || /^[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?$/.test(value)
                ? undefined
                : t("form.badName"),
          }}
        >
          {(f) => (
            <FormField label={t("form.volumeName")} errors={f.state.meta.errors}>
              <Input
                size="sm"
                mono
                value={f.state.value}
                placeholder="data"
                disabled={disabled}
                onChange={(e) => f.handleChange(e.target.value)}
              />
            </FormField>
          )}
        </form.Field>
        <form.Field
          name="mountPath"
          validators={{
            onChange: ({ value }) => (value.startsWith("/") ? undefined : t("form.badPath")),
          }}
        >
          {(f) => (
            <FormField label={t("storage.mountPath")} errors={f.state.meta.errors}>
              <Input
                size="sm"
                mono
                value={f.state.value}
                placeholder="/data"
                disabled={disabled}
                onChange={(e) => f.handleChange(e.target.value)}
              />
            </FormField>
          )}
        </form.Field>
        <form.Field
          name="size"
          validators={{
            onChange: ({ value }) =>
              quantity.test(value.trim()) ? undefined : t("form.badQuantity"),
          }}
        >
          {(f) => (
            <FormField label={t("form.size")} errors={f.state.meta.errors}>
              <Input
                size="sm"
                mono
                value={f.state.value}
                disabled={disabled}
                onChange={(e) => f.handleChange(e.target.value)}
              />
            </FormField>
          )}
        </form.Field>
      </div>
      <Collapsible>
        <CollapsibleTrigger variant="disclosure">
          <CaretRightIcon aria-hidden="true" />
          {t("form.advanced")}
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="grid grid-cols-1 gap-3 pt-3 sm:grid-cols-2">
            <form.Subscribe selector={(s) => s.values.name}>
              {(name) => (
                <form.Field
                  name="claimName"
                  validators={{
                    onChange: ({ value }) =>
                      value === null || subdomain.test(value.trim())
                        ? undefined
                        : t("form.badClaimName"),
                  }}
                >
                  {(f) => (
                    <FormField
                      label={t("form.claimName")}
                      errors={f.state.meta.errors}
                      hint={t("form.claimNameHint")}
                    >
                      <Input
                        size="sm"
                        mono
                        value={f.state.value ?? defaultClaimName(workload, name)}
                        placeholder={defaultClaimName(workload, "data")}
                        disabled={disabled}
                        onChange={(e) => f.handleChange(e.target.value)}
                      />
                    </FormField>
                  )}
                </form.Field>
              )}
            </form.Subscribe>
            <form.Field name="accessMode">
              {(f) => (
                <FormField label={t("form.accessMode")} hint={t("form.accessModeHint")}>
                  <Select
                    size="sm"
                    value={f.state.value}
                    disabled={disabled}
                    onValueChange={(mode) => f.handleChange(mode)}
                    translate="no"
                    items={accessModes.map((mode) => ({ value: mode, label: mode }))}
                  />
                </FormField>
              )}
            </form.Field>
          </div>
        </CollapsibleContent>
      </Collapsible>
      <div className="flex justify-end">
        <form.Subscribe selector={(s) => [s.canSubmit, s.values.mountPath] as const}>
          {([canSubmit, mountPath]) => {
            const save = (
              <Button type="submit" size="sm" disabled={disabled || !canSubmit || !mountPath}>
                <ReviewWord />
              </Button>
            )
            return disabledReason ? (
              <Tooltip content={disabledReason}>
                <span>{save}</span>
              </Tooltip>
            ) : (
              save
            )
          }}
        </form.Subscribe>
      </div>
    </form>
  )
}
