import { Select } from "@zeabur/design-system/components/select"

import { SizeDetail } from "@/components/amount-input"
import { FormField } from "@/components/form-field"
import { useT } from "@/i18n"
import { imageResources, imageSizes, type ImageSize } from "@/model/image-service"

/** Small, medium or large, with the CPU and memory each reserves. */
export function ImageSizeField({
  size,
  onSize,
}: {
  size: ImageSize
  onSize: (size: ImageSize) => void
}) {
  const t = useT()
  const resources = imageResources[size]
  return (
    <FormField label={t("db.field.size")} hint={<SizeDetail resources={resources} />}>
      <Select
        size="sm"
        value={size}
        onValueChange={(value) => onSize(value as ImageSize)}
        items={imageSizes.map((p) => ({ value: p, label: t(`db.size.${p}`) }))}
      />
    </FormField>
  )
}
