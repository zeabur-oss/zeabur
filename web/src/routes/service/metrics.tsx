// A service's metrics: usage against requests and limits, live (per replica) and over time
// (`?range`, `?split`). CronJobs and Jobs, whose pods come and go between samples, and services
// without metrics keep their requests and limits alone.
import { Hint, Panel } from "@/components/bits"
import { useT } from "@/i18n"
import type { Service } from "@/model/service"
import { hasUsage } from "@/model/usage"
import { HistoryPanel, UsagePanel } from "@/routes/service/usage"

export function MetricsTab({ project, service }: { project: string; service: Service }) {
  return (
    <div className="flex flex-col gap-4">
      {hasUsage(service) ? (
        <>
          <UsagePanel project={project} service={service} />
          <HistoryPanel project={project} service={service} />
        </>
      ) : (
        <ResourcesCard service={service} />
      )}
    </div>
  )
}

function cell(value?: string | null) {
  return value ? (
    <span className="font-mono">{value}</span>
  ) : (
    <span className="text-muted-foreground">—</span>
  )
}

function ResourcesCard({ service }: { service: Service }) {
  const t = useT()
  const containers = service.containers.filter((c) => !c.init)
  return (
    <Panel
      title={t("resources.title")}
      action={<Hint label={t("resources.title")}>{t("resources.hint")}</Hint>}
      bodyClassName="p-0"
    >
      <table className="w-full text-xs">
        <thead>
          <tr className="border-b text-left text-muted-foreground">
            <th className="px-5 py-2 font-medium">
              {containers.length > 1 ? t("resources.container") : ""}
            </th>
            <th className="px-2 py-2 font-medium">{t("resources.cpu")}</th>
            <th className="px-2 py-2 pr-5 font-medium">{t("resources.memory")}</th>
          </tr>
        </thead>
        <tbody className="divide-y">
          {containers.flatMap((container) =>
            (["requests", "limits"] as const).map((which) => (
              <tr key={`${container.name}/${which}`}>
                <td className="px-5 py-2 text-muted-foreground">
                  {containers.length > 1 ? (
                    <span className="mr-1.5 font-mono text-foreground" translate="no">
                      {container.name}
                    </span>
                  ) : null}
                  {t(`resources.${which}`)}
                </td>
                <td className="px-2 py-2">{cell(container.resources?.[which]?.cpu)}</td>
                <td className="px-2 py-2 pr-5">{cell(container.resources?.[which]?.memory)}</td>
              </tr>
            )),
          )}
        </tbody>
      </table>
    </Panel>
  )
}
