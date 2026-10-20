import { useQuery } from "@tanstack/react-query"
import HardDrivesIcon from "~icons/ph/hard-drives"

import { clusterQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"

function useClusterLabel(): string | undefined {
  return useQuery(clusterQuery).data?.name || undefined
}

export function Identity({
  namespace,
  target,
  className,
}: {
  namespace?: string
  target?: string
  className?: string
}) {
  const t = useT()
  const cluster = useClusterLabel()
  return (
    <span
      className={cn(
        "inline-flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground",
        className,
      )}
    >
      <HardDrivesIcon className="-mr-1.5 size-3.5 shrink-0" aria-hidden="true" />
      {cluster ? (
        <span className="inline-flex items-center gap-1.5">
          {t("identity.cluster")}
          <span className="font-mono text-foreground/80" translate="no">
            {cluster}
          </span>
        </span>
      ) : null}
      {namespace ? (
        <span className="inline-flex items-center gap-1.5">
          {t("identity.namespace")}
          <span className="font-mono text-foreground/80" translate="no">
            {namespace}
          </span>
        </span>
      ) : null}
      {target ? (
        <span className="font-mono text-foreground/80" translate="no">
          {target}
        </span>
      ) : null}
    </span>
  )
}
