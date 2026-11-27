import { useState } from "react"
import WarningFillIcon from "~icons/ph/warning-fill"

import { rules } from "@/api/rules"
import { FormField } from "@/components/form-field"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import type { Endpoint, Service } from "@/model/service"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Select } from "@zeabur-oss/design-system/components/select"

const EXTERNAL = rules["PublicPortChange.external"]

/** The external port typed in `text`: `undefined` when empty, `NaN` when it is not a port. */
function externalOf(text: string): number | undefined {
  if (text.trim() === "") return undefined
  const port = /^\d+$/.test(text.trim()) ? Number(text) : Number.NaN
  return port >= EXTERNAL.minimum && port <= EXTERNAL.maximum ? port : Number.NaN
}

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
  const isOpen = (from: string, port: number) =>
    open.some((e) => e.opens?.service === from && e.opens.port === port)
  const choices = endpoints.flatMap((e) =>
    e.ports
      .filter((p) => !isOpen(e.name, p.port))
      .map((p) => ({
        value: `${e.name}/${p.port}`,
        label: `${e.name}:${p.port} ${p.protocol}`,
        from: e.name,
        port: p.port,
      })),
  )
  const [chosen, setChosen] = useState(choices[0]?.value ?? "")
  const choice = choices.find((c) => c.value === chosen)
  const [externalText, setExternalText] = useState("")
  const external = externalOf(externalText)
  const invalid = Number.isNaN(external)
  const database = !!service.dataStore
  const workload = { kind: service.kind, name: service.name }
  const review = () => {
    if (!choice || invalid) return
    step.plan({
      title: t("ports.openTitle", choice.port, service.name),
      service: workload,
      changes: [
        {
          op: "publicPort",
          workload,
          service: choice.from,
          port: choice.port,
          ...(external === undefined || external === choice.port ? {} : { external }),
        },
      ],
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
            disabled={!choice || invalid}
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
          <>
            <FormField label={t("ports.port")}>
              <Select
                size="sm"
                translate="no"
                value={chosen}
                onValueChange={setChosen}
                items={choices}
              />
            </FormField>
            <FormField
              label={t("ports.external")}
              hint={t("ports.externalHint")}
              errors={invalid ? [t("add.problem.port.invalid")] : []}
            >
              <Input
                size="sm"
                mono
                inputMode="numeric"
                autoComplete="off"
                value={externalText}
                placeholder={choice ? String(choice.port) : undefined}
                onChange={(event) => setExternalText(event.target.value)}
                className="w-32"
              />
            </FormField>
          </>
        )}
      </form>
    </PlanDrawer>
  )
}
