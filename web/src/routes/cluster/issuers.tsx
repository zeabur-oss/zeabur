// Cluster → Overview → Certificates: cert-manager's issuers, read only.
import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"

import type { Schemas } from "@/api/client"
import { clusterIssuersQuery } from "@/api/queries"
import { KubeDetails } from "@/components/kube-details"
import { StatusPill } from "@/components/status"
import { useT } from "@/i18n"
import { solverLabel } from "@/model/cluster-issuers"
import { Section } from "@/routes/cluster/section"
import { Badge } from "@zeabur/design-system/components/badge"
import { buttonVariants } from "@zeabur/design-system/components/button"

type Data = Schemas["ClusterIssuers"]

const readyHealth = { ready: "ok", notReady: "fail", unknown: "idle" } as const

export function IssuersSection() {
  const t = useT()
  const issuers = useQuery(clusterIssuersQuery)
  const data = issuers.data
  if (!data || data.state === "refused") return null
  const installed = data.state === "installed"
  return (
    <Section
      id="cluster-certificates"
      title={t("issuers.title")}
      count={installed ? data.issuers.length : undefined}
    >
      {installed ? (
        <Installed data={data} />
      ) : (
        <p className="type-meta text-muted-foreground">
          {t("issuers.notInstalled")}
          {t("common.gap")}
          <Link to="/cluster/components" className={buttonVariants({ variant: "link" })}>
            {t("issuers.install")}
          </Link>
        </p>
      )}
    </Section>
  )
}

function Installed({ data }: { data: Data }) {
  const t = useT()
  return (
    <>
      <Verdict data={data} />
      {data.issuers.length > 0 ? <List data={data} /> : null}
      <KubeDetails
        facts={[
          [t("issuers.kube.issuer"), "ClusterIssuer (cert-manager.io/v1)"],
          [t("issuers.kube.domain"), "Ingress cert-manager.io/cluster-issuer"],
        ]}
      />
    </>
  )
}

function Verdict({ data }: { data: Data }) {
  const t = useT()
  const wildcard = data.issuers.find((i) => i.wildcards && i.ready === "ready")
  return (
    <p className="type-body text-secondary">
      {wildcard ? t("issuers.wildcardsBy", wildcard.name) : t("issuers.noWildcards")}
    </p>
  )
}

function List({ data }: { data: Data }) {
  const t = useT()
  return (
    <ul className="flex flex-col divide-y divide-main rounded-lg border border-main bg-card">
      {data.issuers.map((issuer) => (
        <li key={issuer.name} className="flex min-w-0 flex-col gap-1 px-4 py-2.5">
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
            <span className="min-w-0 truncate type-mono font-medium" translate="no">
              {issuer.name}
            </span>
            <StatusPill health={readyHealth[issuer.ready]} size="sm">
              {t(`issuers.ready.${issuer.ready}`)}
            </StatusPill>
            {issuer.name === data.default ? (
              <Badge variant="outline">{t("issuers.default")}</Badge>
            ) : null}
            {issuer.staging ? <Badge variant="warn">{t("issuers.staging")}</Badge> : null}
          </div>
          <p className="type-meta text-muted-foreground">
            <span translate="no">{issuer.solvers.map(solverLabel).join(", ") || "—"}</span>
            {t("common.sep")}
            {issuer.wildcards ? t("issuers.wildcards") : t("issuers.noWildcard")}
          </p>
          {issuer.ready === "notReady" && issuer.message ? (
            <p className="type-meta break-words text-muted-foreground">{issuer.message}</p>
          ) : null}
        </li>
      ))}
    </ul>
  )
}
