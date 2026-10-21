import { useState } from "react"
import CheckIcon from "~icons/ph/check"
import CopyIcon from "~icons/ph/copy"

import { Button } from "@zeabur/design-system/components/button"
import { useT } from "@/i18n"

/** A labelled button that copies text made when it is pressed. */
export function CopyText({
  value,
  label,
  primary = false,
}: {
  value: () => string | Promise<string>
  label: string
  /** The one thing the reader can do, such as handing a cluster's problem to its administrator. */
  primary?: boolean
}) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(await value())
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      setCopied(false)
    }
  }
  return (
    <Button
      variant={primary ? "default" : "ghost"}
      size="sm"
      onClick={() => void copy()}
      aria-live="polite"
    >
      {copied ? (
        <CheckIcon aria-hidden="true" data-icon="inline-start" />
      ) : (
        <CopyIcon aria-hidden="true" data-icon="inline-start" />
      )}
      {copied ? t("common.copied") : label}
    </Button>
  )
}
