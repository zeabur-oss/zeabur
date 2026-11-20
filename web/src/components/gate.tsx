import { useQuery } from "@tanstack/react-query"
import { useParams } from "@tanstack/react-router"
import { MenuItem } from "@zeabur-oss/design-system/components/menu"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"
import { cloneElement, type ReactElement, type ReactNode } from "react"
import LockSimpleIcon from "~icons/ph/lock-simple"

import type { Schemas } from "@/api/client"
import { accountQuery } from "@/api/members"
import { clusterQuery, namespaceQuery, rightsQuery, whoamiQuery } from "@/api/queries"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import {
  askerOf,
  gate,
  needsOf,
  projectPermission,
  type ClusterRights,
  type Gate,
  type ProjectRight,
  type Right,
} from "@/model/rights"

type Permission = Schemas.Permission

function useAsker() {
  return askerOf(useQuery(whoamiQuery).data)
}

/** Whether the person may do `right` in `project`: a change is closed when the project is read-only
 * for them, a read when its own review refuses it; otherwise left to the plan's dry run. */
export function useGate(project: string | undefined, right: ProjectRight): Gate {
  return useGates(project)(right)
}

/** `useGate` in the project of the current route, for a control deep in a page. */
export function useHereGate(right: ProjectRight): Gate {
  const { project } = useParams({ strict: false }) as { project?: string }
  return useGate(project, right)
}

/** Every project right at once, for a page with many actions. */
export function useGates(project: string | undefined): (right: ProjectRight) => Gate {
  const asker = useAsker()
  const namespace = useQuery({ ...namespaceQuery(project ?? ""), enabled: Boolean(project) })
  const permission = projectPermission(namespace.data?.readOnly)
  // Reads are not closed by a read-only project; each has its own review.
  const reads: Partial<Record<ProjectRight, Permission | undefined>> = {
    readSecrets: namespace.data?.canReadSecrets,
    readLogs: namespace.data?.canReadLogs,
  }
  return (right) => gate(right, right in reads ? reads[right] : permission, asker)
}

/** A right from a hint another answer carries, such as the members list's `canManage`. */
export function useHintGate(right: Right, hint: Permission | undefined): Gate {
  return gate(right, hint, useAsker())
}

export function useClusterGate(right: keyof ClusterRights): Gate {
  const asker = useAsker()
  const cluster = useQuery(clusterQuery)
  return gate(right, cluster.data?.can[right], asker)
}

export function useClusterAdmin(): boolean {
  return useQuery(accountQuery).data?.clusterAdmin === true
}

/** Why an action is closed, in a sentence; nothing when it is open. */
export function useGateReason(): (gate: Gate) => string | undefined {
  const t = useT()
  const rights = useQuery(rightsQuery).data
  return (g) => {
    if (g.open) return undefined
    const needs = needsOf(rights, g.right)
    if (g.member && needs)
      return t(
        "rights.needs",
        t(needs.role === "admin" ? "account.role.admin" : "account.role.developer"),
      )
    return t("rights.needsRule", needs?.rule ?? g.right)
  }
}

/** A control the person's role cannot use: disabled, with the role that can in a tooltip. */
export function GateTip({
  gate: g,
  className,
  children,
}: {
  gate: Gate
  /** For the wrapper a closed control gets, such as the child's own responsive visibility. */
  className?: string
  children: ReactElement
}) {
  const reason = useGateReason()(g)
  if (!reason) return children
  const disabled = cloneElement(children as ReactElement<{ disabled?: boolean }>, {
    disabled: true,
  })
  return (
    <Tooltip content={reason}>
      <span className={cn("inline-flex", className)} tabIndex={0}>
        {disabled}
        <span className="sr-only">{reason}</span>
      </span>
    </Tooltip>
  )
}

/** A menu item the person's role cannot use: disabled, with the role that can under its label. */
export function GatedMenuItem({
  gate: g,
  testid,
  onClick,
  render,
  children,
}: {
  gate: Gate
  testid?: string
  onClick?: () => void
  render?: ReactElement
  children: ReactNode
}) {
  const reason = useGateReason()(g)
  if (!reason)
    return (
      <MenuItem data-testid={testid} onClick={onClick} render={render}>
        {children}
      </MenuItem>
    )
  return (
    <MenuItem disabled>
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className="flex items-center gap-2.5">{children}</span>
        <span className="type-meta">{reason}</span>
      </span>
    </MenuItem>
  )
}

/** One line over a form the person's role cannot change: its fields are disabled, and a field
 * shows no tooltip. */
export function GateNote({ gate: g }: { gate: Gate }) {
  const reason = useGateReason()(g)
  if (!reason) return null
  return (
    <p className="flex items-center gap-2 type-meta text-muted-foreground">
      <LockSimpleIcon aria-hidden="true" className="size-3.5 shrink-0" />
      {reason}
    </p>
  )
}
