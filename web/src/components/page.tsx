import { Skeleton } from "@zeabur/design-system/components/skeleton"

/** Placeholder rows; `now` drops the skeleton's own delay, for one that replaces a page. */
export function LoadingRows({
  rows = 3,
  label,
  now = false,
}: {
  rows?: number
  label: string
  now?: boolean
}) {
  return (
    <output
      className={now ? "flex flex-col gap-2 skeleton-now" : "flex flex-col gap-2"}
      aria-live="polite"
    >
      <span className="sr-only">{label}</span>
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className="h-10 w-full" />
      ))}
    </output>
  )
}
