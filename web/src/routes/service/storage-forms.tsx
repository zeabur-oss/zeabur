import { useForm } from "@tanstack/react-form"
import CaretRightIcon from "~icons/ph/caret-right"

import { ReviewWord } from "@/components/plan/draft"
import type { Schemas } from "@/api/client"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Select } from "@zeabur/design-system/components/select"
import { Switch } from "@zeabur/design-system/components/switch"
import { useT } from "@/i18n"
import { FormField, quantity } from "@/components/form-field"
import {
  accessModes,
  defaultClaimName,
  subdomain,
  volumeRequest,
  type VolumeValues,
} from "@/routes/service/volume"

export function ClaimForm({
  mount,
  disabled,
  onExpand,
  onMount,
}: {
  mount: Schemas["Mount"]
  disabled: boolean
  onExpand: (size: string) => void
  onMount: (mountPath: string, readOnly: boolean) => void
}) {
  const t = useT()
  const size = useForm({
    defaultValues: { size: mount.size ?? "" },
    onSubmit: ({ value }) => onExpand(value.size.trim()),
  })
  const place = useForm({
    defaultValues: { mountPath: mount.mountPath, readOnly: mount.readOnly ?? false },
    onSubmit: ({ value }) => onMount(value.mountPath.trim(), value.readOnly),
  })
  return (
    <div className="flex flex-col gap-3 border-t pt-3">
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault()
          void size.handleSubmit()
        }}
      >
        <size.Field
          name="size"
          validators={{
            onChange: ({ value }) =>
              quantity.test(value.trim()) ? undefined : t("form.badQuantity"),
          }}
        >
          {(f) => (
            <FormField
              label={t("form.size")}
              errors={f.state.meta.errors}
              hint={t("form.sizeHint")}
            >
              <Input
                size="sm"
                mono
                value={f.state.value}
                disabled={disabled}
                onChange={(e) => f.handleChange(e.target.value)}
                className="w-28"
              />
            </FormField>
          )}
        </size.Field>
        <size.Subscribe selector={(s) => [s.canSubmit, s.isDirty] as const}>
          {([canSubmit, dirty]) => (
            <Button type="submit" size="xs" disabled={disabled || !canSubmit || !dirty}>
              {t("form.expand")}
            </Button>
          )}
        </size.Subscribe>
      </form>
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault()
          void place.handleSubmit()
        }}
      >
        <place.Field
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
                className="w-56"
              />
            </FormField>
          )}
        </place.Field>
        <place.Field name="readOnly">
          {(f) => (
            <label className="flex items-center gap-2 pb-1 text-xs">
              <Switch
                checked={f.state.value}
                disabled={disabled}
                onCheckedChange={(checked) => f.handleChange(checked)}
              />
              {t("form.readOnly")}
            </label>
          )}
        </place.Field>
        <place.Subscribe selector={(s) => [s.canSubmit, s.isDirty] as const}>
          {([canSubmit, dirty]) => (
            <Button
              type="submit"
              variant="outline"
              size="xs"
              disabled={disabled || !canSubmit || !dirty}
            >
              <ReviewWord />
            </Button>
          )}
        </place.Subscribe>
      </form>
    </div>
  )
}

export function AddVolume({
  workload,
  disabled,
  onPlan,
}: {
  workload: string
  disabled: boolean
  onPlan: (volume: Schemas["VolumeRequest"]) => void
}) {
  const t = useT()
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
      className="flex flex-wrap items-end gap-3 rounded-xl border border-dashed p-4"
      onSubmit={(event) => {
        event.preventDefault()
        void form.handleSubmit()
      }}
    >
      <span className="w-full text-sm font-medium">{t("form.addVolume")}</span>
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
              className="w-36"
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
              className="w-56"
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
              className="w-24"
            />
          </FormField>
        )}
      </form.Field>
      <form.Subscribe selector={(s) => [s.canSubmit, s.values.mountPath] as const}>
        {([canSubmit, mountPath]) => (
          <Button type="submit" size="xs" disabled={disabled || !canSubmit || !mountPath}>
            <ReviewWord />
          </Button>
        )}
      </form.Subscribe>
      <Collapsible className="w-full">
        <CollapsibleTrigger variant="disclosure">
          <CaretRightIcon aria-hidden="true" />
          {t("form.advanced")}
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="flex flex-wrap items-start gap-3 pt-3">
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
                        className="w-56"
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
    </form>
  )
}
