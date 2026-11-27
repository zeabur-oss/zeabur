import { useState } from "react"
import WarningFillIcon from "~icons/ph/warning-fill"

import { FormField } from "@/components/form-field"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { externalOf, openPortChange, portChoices } from "@/model/public-ports"
import type { Endpoint, Service } from "@/model/service"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Select } from "@zeabur-oss/design-system/components/select"

/** Who can connect once the port is open, and what is at risk for a database. */
function ExposureAlert({ name, database }: { name: string; database: boolean }) {
  const t = useT()
  return (
    <Alert variant={database ? "destructive" : "warn"} role="note">
      <WarningFillIcon aria-hidden="true" />
      <AlertTitle>{database ? t("ports.databaseTitle", name) : t("ports.anyoneTitle")}</AlertTitle>
      <AlertDescription>
        {database ? t("ports.databaseBody") : t("ports.anyoneBody")}
      </AlertDescription>
    </Alert>
  )
}

/** The optional external port, empty for the same port as `port`. */
function ExternalPortField({
  value,
  onChange,
  port,
}: {
  value: string
  onChange: (value: string) => void
  port: number | undefined
}) {
  const t = useT()
  const invalid = Number.isNaN(externalOf(value))
  return (
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
        value={value}
        placeholder={port === undefined ? undefined : String(port)}
        onChange={(event) => onChange(event.target.value)}
        className="w-32"
      />
    </FormField>
  )
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
  const choices = portChoices(endpoints, open)
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
      changes: [openPortChange(workload, choice, external)],
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
        <ExposureAlert name={service.name} database={database} />
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
            <ExternalPortField
              value={externalText}
              onChange={setExternalText}
              port={choice?.port}
            />
          </>
        )}
      </form>
    </PlanDrawer>
  )
}
