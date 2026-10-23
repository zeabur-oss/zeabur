import { FormField } from "@/components/form-field"
import { useT } from "@/i18n"
import { Select } from "@zeabur/design-system/components/select"

/** The container to change, asked only of a service with more than one. */
export function ContainerField({
  containers,
  value,
  onChange,
}: {
  containers: readonly { name: string }[]
  value: string
  onChange: (name: string) => void
}) {
  const t = useT()
  if (containers.length < 2) return null
  return (
    <FormField label={t("tasks.container")}>
      <Select
        size="sm"
        translate="no"
        value={value}
        onValueChange={onChange}
        items={containers.map((c) => ({ value: c.name, label: c.name }))}
      />
    </FormField>
  )
}
