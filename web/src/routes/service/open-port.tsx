import { useState } from "react"
import WarningFillIcon from "~icons/ph/warning-fill"

import { KubeDetails } from "@/components/kube-details"
import { FormField } from "@/components/form-field"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Endpoint, Service } from "@/model/service"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Select } from "@zeabur/design-system/components/select"

/** Open one port of a service's Service to the internet with a `LoadBalancer` Service. */
export default function OpenPortDrawer({
  project,
  service,
  endpoints,
  open,
  onClose,
}: {
  project: string
  service: Service
  /** The service's Services that are not open yet. */
  endpoints: Endpoint[]
  /** Its `LoadBalancer` Services. */
  open: Endpoint[]
  onClose: () => void
}) {
  const t = useT()
  const step = usePlanStep(project, onClose)
  const isOpen = (port: number) => open.some((e) => e.ports.some((p) => p.port === port))
  const choices = endpoints.flatMap((e) =>
    e.ports
      .filter((p) => !isOpen(p.port))
      .map((p) => ({
        value: `${e.name}/${p.port}`,
        label: `${e.name}:${p.port} ${p.protocol}`,
      })),
  )
  const [chosen, setChosen] = useState(choices[0]?.value ?? "")
  const [from, portText] = chosen.split("/") as [string, string]
  const port = Number(portText)
  const database = !!service.dataStore
  const workload = { kind: service.kind, name: service.name }
  const review = () => {
    if (!chosen) return
    step.plan({
      title: t("ports.openTitle", port, service.name),
      service: workload,
      changes: [{ op: "publicPort", workload, service: from, port }],
    })
  }
  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("ports.openTitleShort", service.name)}
      description={t("ports.about")}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("edit.cancel")}
          </Button>
          <Button
            size="sm"
            variant={database ? "destructive" : "default"}
            disabled={!chosen}
            onClick={review}
          >
            {t("edit.review")}
          </Button>
        </div>
      }
    >
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault()
          review()
        }}
      >
        <Alert variant={database ? "destructive" : "warn"} role="note">
          <WarningFillIcon aria-hidden="true" />
          <AlertTitle>
            {database ? t("ports.databaseTitle", service.name) : t("ports.anyoneTitle")}
          </AlertTitle>
          <AlertDescription>
            {database ? t("ports.databaseBody") : t("ports.anyoneBody")}
          </AlertDescription>
        </Alert>
        {choices.length === 0 ? (
          <p className="type-meta text-muted-foreground">{t("ports.allOpen")}</p>
        ) : (
          <FormField label={t("ports.port")} hint={t("ports.portHint")}>
            <Select
              size="sm"
              translate="no"
              value={chosen}
              onValueChange={setChosen}
              items={choices}
            />
          </FormField>
        )}
        <KubeDetails
          facts={[
            [t("ports.kube.object"), `Service ${from}-public-${portText ?? ""} type: LoadBalancer`],
            [t("ports.kube.how"), t("ports.kube.howValue")],
          ]}
        />
      </form>
    </PlanDrawer>
  )
}
