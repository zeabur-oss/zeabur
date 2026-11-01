import { useRef, type ChangeEvent } from "react"

import { FormField } from "@/components/form-field"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"

/** A text field that can also take the text of a local file. */
export function TextFile({
  label,
  hint,
  upload,
  value,
  rows,
  placeholder,
  accept,
  errors,
  onChange,
  autoFocus,
}: {
  label: string
  hint: string
  upload: string
  value: string
  rows: number
  placeholder?: string
  accept?: string
  errors?: string[]
  onChange: (text: string) => void
  autoFocus?: boolean
}) {
  const input = useRef<HTMLInputElement>(null)
  const read = (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (file) void file.text().then(onChange)
    event.target.value = ""
  }
  return (
    <FormField label={label} hint={hint} errors={errors}>
      <div className="flex flex-col gap-2">
        <Textarea
          mono
          className="max-h-80"
          rows={rows}
          value={value}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
          autoFocus={autoFocus}
          onChange={(event) => onChange(event.target.value)}
        />
        <input ref={input} type="file" accept={accept} hidden onChange={read} />
        <Button
          variant="outline"
          size="sm"
          className="self-start"
          onClick={() => input.current?.click()}
        >
          {upload}
        </Button>
      </div>
    </FormField>
  )
}
