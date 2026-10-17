// Which environment a page or an operation is about: the cluster and the namespace. The cluster is
// named as /cluster names it (the kubeconfig's name for it, else kube-apiserver's host). The
// version is the cluster page's to show, not repeated beside every operation.
import { useQuery } from "@tanstack/react-query"
import HardDrivesIcon from "~icons/ph/hard-drives"

import { clusterQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"

/** `k3d-engawa`, or undefined until the cluster's answer is in. */
function useClusterLabel(): string | undefined {
  return useQuery(clusterQuery).data?.name || undefined
}

/**
 * "Cluster k3d-engawa  Namespace shop  Deployment/worker": labelled pairs set apart by space, not joined
 * with dots, so each value reads with its own label. A page that already names the namespace and
 * the object (a service's header) leaves them out.
 */
export function Identity({
  namespace,
  target,
  className,
}: {
  namespace?: string
  /** The object an operation acts on: `Deployment/worker`. */
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
