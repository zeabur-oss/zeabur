import { useQuery } from "@tanstack/react-query"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Label } from "@zeabur/design-system/components/label"
import { Switch } from "@zeabur/design-system/components/switch"
import { useId } from "react"
import CaretRightIcon from "~icons/ph/caret-right"

import { configReloadQuery } from "@/api/queries"
import { ManagedTip } from "@/components/bits"
import { useT } from "@/i18n"
import type { Service } from "@/model/service"
import type { ServiceSettingsProps, ServiceSettingsSection } from "@/modules/service-settings"

/** Whether Stakater Reloader replaces the service's pods when a ConfigMap or Secret it reads
 * changes: the workload's `reloader.stakater.com/auto` annotation. */
function ConfigReloadSetting({ service, plan }: ServiceSettingsProps) {
  const t = useT()
  const id = useId()
  const cluster = useQuery(configReloadQuery)
  const reload = service.configReload
  const on = reload?.auto ?? false
  const managed = service.provenance.tool ?? undefined
  const workload = { kind: service.kind, name: service.name }
  const named = [...(reload?.configMaps ?? []), ...(reload?.secrets ?? [])]
  const state = cluster.data?.reloader
  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <div className="flex items-start gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <Label htmlFor={id}>{t("reload.switch")}</Label>
          <p className="type-meta text-muted-foreground">{on ? t("reload.on") : t("reload.off")}</p>
          {on && (state === "notInstalled" || state === "stopped") ? (
            <p className="type-meta text-warn-strong">
              {state === "notInstalled" ? t("reload.notInstalled") : t("reload.stopped")}
            </p>
          ) : null}
        </div>
        <ManagedTip tool={managed}>
          <Switch
            id={id}
            checked={on}
            disabled={Boolean(managed)}
            onCheckedChange={(enabled) =>
              plan({
                title: t("title.configReload", service.name),
                changes: [{ op: "configReload", workload, enabled }],
                service: workload,
              })
            }
          />
        </ManagedTip>
      </div>
      <Collapsible>
        <CollapsibleTrigger variant="disclosure">
          <CaretRightIcon aria-hidden="true" />
          {t("reload.how")}
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="flex flex-col gap-1 pt-1 type-meta text-muted-foreground">
            <p>{t("reload.mechanism")}</p>
            {named.length > 0 ? <p>{t("reload.named", t("common.list", named))}</p> : null}
            {state === "unknown" ? <p>{t("reload.unknown")}</p> : null}
            {state === "running" ? <p>{t("reload.running")}</p> : null}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}

const longRunning = (service: Service) =>
  service.kind === "Deployment" || service.kind === "StatefulSet" || service.kind === "DaemonSet"

export const reloaderSection: ServiceSettingsSection = {
  module: "reloader",
  title: "reload.switch",
  applies: longRunning,
  Section: ConfigReloadSetting,
}
