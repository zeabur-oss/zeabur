// Cluster → Backups: which data is backed up by what, and whether that tool is there.
import { Link } from "@tanstack/react-router"
import type { ReactNode } from "react"

import type { Schemas } from "@/api/client"
import { StatusDot } from "@/components/status"
import { useT } from "@/i18n"
import { providerOf } from "@/model/cluster-backups"
import type { Health } from "@/model/service"
import { Section } from "@/routes/cluster/section"

export function BackupCoverage({ data }: { data?: Schemas.ClusterBackups }) {
  const t = useT()
  const velero = data && data.state !== "not-installed" ? data.verdict : undefined
  return (
    <Section id="backup-coverage" title={t("clusterBackups.coverage.title")}>
      <div className="overflow-hidden rounded-lg border border-main bg-card">
        <table className="w-full text-left">
          <thead className="max-sm:sr-only">
            <tr className="border-b border-main type-meta text-muted-foreground">
              <th scope="col" className="px-4 py-2 font-normal sm:w-1/5">
                {t("clusterBackups.coverage.data")}
              </th>
              <th scope="col" className="px-4 py-2 font-normal">
                {t("clusterBackups.coverage.method")}
              </th>
              <th scope="col" className="px-4 py-2 font-normal sm:w-1/4">
                {t("clusterBackups.coverage.status")}
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-main">
            <Row
              data={t("clusterBackups.coverage.databases")}
              method={t("clusterBackups.coverage.databasesMethod")}
              status={<Plain>{t("clusterBackups.coverage.perDatabase")}</Plain>}
            />
            <Row
              data={t("clusterBackups.coverage.volumes")}
              method={t("clusterBackups.coverage.volumesMethod")}
              status={<Tool provider={providerOf(data?.providers, "k8up")} />}
            />
            <Row
              data={t("clusterBackups.coverage.project")}
              method={t("clusterBackups.coverage.projectMethod")}
              status={
                velero ? (
                  <Dot health={velero.health}>
                    {t(`clusterBackups.verdict.${velero.state as "ok"}`)}
                  </Dot>
                ) : (
                  <Tool provider={providerOf(data?.providers, "velero")} />
                )
              }
            />
          </tbody>
        </table>
      </div>
    </Section>
  )
}

function Row({ data, method, status }: { data: string; method: string; status: ReactNode }) {
  return (
    <tr className="align-top max-sm:flex max-sm:flex-col max-sm:gap-1 max-sm:px-4 max-sm:py-3">
      <th scope="row" className="type-body font-medium text-main sm:px-4 sm:py-3">
        {data}
      </th>
      <td className="type-meta text-muted-foreground sm:px-4 sm:py-3">{method}</td>
      <td className="type-meta text-main sm:px-4 sm:py-3">{status}</td>
    </tr>
  )
}

function Plain({ children }: { children: ReactNode }) {
  return <span className="text-muted-foreground">{children}</span>
}

function Dot({ health, children }: { health: Health; children: ReactNode }) {
  return (
    <span className="flex items-start gap-2">
      <StatusDot health={health} className="mt-1.5" />
      <span className="min-w-0">{children}</span>
    </span>
  )
}

function Tool({ provider }: { provider?: Schemas.BackupProvider }) {
  const t = useT()
  const component = provider?.component
  if (!component || provider.state === "refused") return <Plain>—</Plain>
  if (provider.state === "installed")
    return <Dot health="ok">{t("clusterBackups.coverage.installed", component.name)}</Dot>
  return (
    <Dot health="idle">
      <Link
        to="/cluster/components"
        search={{ component: component.id }}
        className="underline underline-offset-2"
      >
        {t("clusterBackups.coverage.install", component.name)}
      </Link>
    </Dot>
  )
}
