import type { CSSProperties } from "react"

import type { BarShares } from "@/model/resource-bar"

const at = (share: number) => `${share * 100}%`

/** The span from request to limit, a tick at the usage, and the two ends' amounts under it. */
export function ResourceBar({
  shares,
  usage,
  largest,
  label,
}: {
  shares: BarShares
  usage?: string
  largest: string
  label: string
}) {
  const style = {
    "--from": at(shares.from),
    "--to": at(1 - shares.to),
    // The tick is 2px wide: keep it inside the track at both ends.
    "--mark": `calc(${shares.mark ?? 0} * (100% - 2px))`,
  } as CSSProperties
  return (
    <div className="flex flex-col gap-1">
      <div
        role="img"
        aria-label={label}
        style={style}
        className="relative h-1 w-full overflow-hidden rounded-full bg-track"
      >
        <div className="absolute inset-y-0 right-(--to) left-(--from) rounded-full bg-usage-normal" />
        {shares.mark === undefined ? null : (
          <div className="absolute -inset-y-px left-(--mark) w-0.5 bg-foreground" />
        )}
      </div>
      <div
        aria-hidden="true"
        className="flex justify-between gap-2 type-meta text-muted-foreground tabular-nums"
      >
        <span>{usage}</span>
        <span>{largest}</span>
      </div>
    </div>
  )
}
