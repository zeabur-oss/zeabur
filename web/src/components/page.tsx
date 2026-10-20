// Loading states shared by every page (web/DESIGN.md); refusals are in components/refused.tsx.
import { Skeleton } from "@zeabur/design-system/components/skeleton"

export function LoadingRows({ rows = 3, label }: { rows?: number; label: string }) {
  return (
    <output className="flex flex-col gap-2" aria-live="polite">
      <span className="sr-only">{label}</span>
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className="h-10 w-full" />
      ))}
    </output>
  )
}
