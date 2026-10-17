// A section the person may not read, shown quietly in its place (components/refused.tsx): "View as
// admin" where admin rights are offered, and folded under "Details" the permissions it lacks, in
// Kubernetes' own words, and a ClusterRole with its binding to the person, ready to hand to a
// cluster administrator.
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"
import CheckIcon from "~icons/ph/check"
import CopyIcon from "~icons/ph/copy"
import WarningIcon from "~icons/ph/warning"

import { toProblem } from "@/api/client"
import { whoamiQuery } from "@/api/queries"
import { permission, Refused } from "@/components/refused"
import { Button } from "@/components/ui/button"
import { useT } from "@/i18n"
import { roleYaml } from "@/model/rbac"

export interface Rule {
  group: string
  resources: string[]
}

/** Every section of the page but Components: one role, so an administrator grants it once. */
export const overviewRole: Role = {
  name: "engawa-cluster-overview",
  rules: [
    { group: "", resources: ["nodes", "namespaces", "pods"] },
    { group: "events.k8s.io", resources: ["events"] },
    { group: "metrics.k8s.io", resources: ["nodes"] },
    { group: "upgrade.cattle.io", resources: ["plans"] },
  ],
}

export interface Role {
  name: string
  rules: Rule[]
  /** `list secrets` in every namespace returns every Secret's data, so the role is as strong as
   * a cluster administrator's for reading; the page and the YAML both say so. */
  readsSecrets?: boolean
}

/** What reading the installed components takes: every probe the detection runs. Listing Helm's
 * release Secrets cluster-wide is close to reading every Secret, so this is an administrator's. */
export const componentsRole: Role = {
  name: "engawa-cluster-components",
  readsSecrets: true,
  rules: [
    { group: "", resources: ["nodes", "secrets"] },
    { group: "apps", resources: ["deployments", "statefulsets", "daemonsets"] },
    { group: "apiextensions.k8s.io", resources: ["customresourcedefinitions"] },
    { group: "apiregistration.k8s.io", resources: ["apiservices"] },
    { group: "networking.k8s.io", resources: ["ingressclasses"] },
    { group: "gateway.networking.k8s.io", resources: ["gatewayclasses"] },
    { group: "helm.cattle.io", resources: ["helmcharts"] },
  ],
}

/** `role` as a ClusterRole that may list what it names, bound to `username`. */
function clusterRoleYaml(role: Role, username: string, warning?: string): string {
  return roleYaml({
    name: role.name,
    rules: role.rules.map((rule) => ({
      apiGroup: rule.group,
      resources: rule.resources,
      verbs: ["list"],
    })),
    username,
    warning,
  })
}

export function Forbidden({
  what,
  missing,
  role,
  error,
}: {
  /** The refused request's error; none for a part the answer marked `unreadable`. */
  error?: unknown
  /** What the person cannot see, as a phrase: "the nodes". */
  what: string
  /** The rules this section lacks. */
  missing: Rule[]
  /** The role to copy. */
  role: Role
}) {
  const t = useT()
  const whoami = useQuery(whoamiQuery)
  const [copied, setCopied] = useState(false)
  const warning = role.readsSecrets ? t("cluster.roleReadsSecrets") : undefined
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(
        clusterRoleYaml(role, whoami.data?.username ?? "", warning),
      )
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      setCopied(false)
    }
  }
  return (
    <Refused
      refusal={error === undefined ? "unreadable" : toProblem(error)}
      what={t("refused.see", what)}
      missing={missing.flatMap((rule) =>
        rule.resources.map((r) => permission("list", r, rule.group)),
      )}
    >
      {warning ? (
        <p className="flex items-start gap-1.5 type-meta font-medium text-warn-strong">
          <WarningIcon aria-hidden="true" className="mt-0.5 size-3.5 shrink-0" />
          {warning}
        </p>
      ) : null}
      <div>
        <Button variant="outline" size="sm" onClick={() => void copy()}>
          {copied ? (
            <CheckIcon aria-hidden="true" data-icon="inline-start" />
          ) : (
            <CopyIcon aria-hidden="true" data-icon="inline-start" />
          )}
          {copied ? t("common.copied") : t("cluster.copyRole")}
        </Button>
      </div>
    </Refused>
  )
}
