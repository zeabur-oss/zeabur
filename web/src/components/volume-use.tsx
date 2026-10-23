// How full a mounted volume is: "7.6 GiB of 10 GiB used" over a bar that turns amber at 85%.
import { useQuery } from "@tanstack/react-query"

import { volumesQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import type { Service } from "@/model/service"
import { missingReason, volumeFill, type MissingReason, type VolumeFill } from "@/model/volume-use"
import { Meter } from "@zeabur/design-system/components/meter"

const tone = { ok: "ok", warn: "warn", fail: "over", progress: "ok", idle: "muted" } as const

/** The fill of each claim the service mounts, by claim, and why some have none once read. */
export function useVolumeFills(project: string, service: Service, claims: readonly string[]) {
  const volumes = useQuery({ ...volumesQuery(project), enabled: claims.length > 0 })
  const items = volumes.data?.items ?? []
  const fill = (claim: string) => volumeFill(items, claim, service.name)
  return { fill, missing: volumes.data && missingReason(volumes.data.state, claims.map(fill)) }
}

/** Why a service's volume use is not shown, in one line under its volumes. */
export function VolumeUseMissing({ reason }: { reason?: MissingReason }) {
  const t = useT()
  if (!reason) return null
  return <p className="type-meta text-muted-foreground">{t(`volume.state.${reason}`)}</p>
}

export function VolumeUse({ fill, className }: { fill: VolumeFill; className?: string }) {
  const t = useT()
  const { memory, percent } = useQuantityFormat()
  const used = t("cluster.usedOf", memory(fill.takenBytes), memory(fill.capacityBytes))
  return (
    <span className={cn("flex min-w-0 flex-col gap-1", className)}>
      <span
        className={cn(
          "type-meta tabular-nums",
          fill.health === "fail"
            ? "text-fail-strong"
            : fill.health === "warn"
              ? "text-warn-strong"
              : "text-secondary",
        )}
      >
        {fill.replicas > 1 ? t("volume.fullestOf", used, fill.replicas) : used}
      </span>
      <Meter
        size="sm"
        value={fill.share}
        tone={tone[fill.health]}
        label={`${used}, ${percent(fill.share)}`}
      />
    </span>
  )
}

/** What a nearly full volume does, and what to do. */
export function VolumeFullNote({ fill, grows }: { fill: VolumeFill; grows?: boolean }) {
  const t = useT()
  if (fill.health !== "warn" && fill.health !== "fail") return null
  return (
    <p
      className={cn("type-meta", fill.health === "fail" ? "text-fail-strong" : "text-warn-strong")}
    >
      {t("volume.full", grows !== false)}
    </p>
  )
}
