import type { ComponentProps } from "react"

import { Skeleton } from "@zeabur/design-system/components/skeleton"

export function LoadingRows({
  rows = 3,
  label,
  delay,
}: {
  rows?: number
  label: string
  delay?: ComponentProps<typeof Skeleton>["delay"]
}) {
  return (
    <output className="flex flex-col gap-2" aria-live="polite">
      <span className="sr-only">{label}</span>
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} delay={delay} className="h-10 w-full" />
      ))}
    </output>
  )
}
