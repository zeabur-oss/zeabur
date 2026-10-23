import { useForm, useStore } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"

import type { Schemas } from "@/api/client"
import { nodesQuery, releasesQuery } from "@/api/queries"
import { Hint } from "@/components/bits"
import { GateNote, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Select } from "@zeabur/design-system/components/select"
import { useT } from "@/i18n"
import { useQuantityFormat } from "@/lib/quantity"
import { serviceSettingsSections } from "@/modules/service-settings"
import { refusedField } from "@/model/problem"
import { endpointsOf, type Endpoint, type Network, type Service } from "@/model/service"
import { FormField, quantity } from "@/components/form-field"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { useHold } from "@/components/plan/draft"

// One chunk for every section: a chunk of its own moves shared code onto the first page load.
export { BuildSection } from "@/routes/service/build-settings"

const probeKinds = ["readiness", "liveness", "startup"] as const
type ProbeKind = (typeof probeKinds)[number]

/** Replicas and each container's CPU and memory. */
export function ComputeSection({ project, service }: { project: string; service: Service }) {
  const t = useT()
  const planner = usePlanner(project)
  const managed = service.provenance.tool
  const gate = useGate(project, "changeServices")
  return (
    <section aria-labelledby="compute-title" className="flex flex-col gap-3">
      <SectionHeader
        id="compute-title"
        title={t("form.compute")}
        description={t("form.computeHint")}
      />
      {managed ? null : <GateNote gate={gate} />}
      {service.kind === "Job" ? (
        <p className="text-sm text-muted-foreground">{t("form.jobs")}</p>
      ) : (
        <Compute
          service={service}
          disabled={!!managed || !gate.open}
          workload={{ kind: service.kind, name: service.name }}
          refused={planner.fields}
        />
      )}
      {sections(service).map(({ module, Section: Module }) => (
        <Module key={module} project={project} service={service} plan={planner.plan} />
      ))}
      {planner.dialog}
    </section>
  )
}

const sections = (service: Service) => serviceSettingsSections.filter((s) => s.applies(service))

/** The traffic, restart and startup checks of the service's first container. */
export function HealthChecksSection({
  project,
  service,
  network,
}: {
  project: string
  service: Service
  network: Network
}) {
  const t = useT()
  const gate = useGate(project, "changeServices")
  if (service.kind === "Job" || service.kind === "CronJob") return null
  return (
    <section aria-labelledby="health-title" className="flex flex-col gap-3">
      <SectionHeader
        id="health-title"
        title={t("form.health")}
        description={t("form.healthHint")}
      />
      {service.provenance.tool ? null : <GateNote gate={gate} />}
      <Probes
        project={project}
        service={service}
        endpoints={endpointsOf(network, service)}
        disabled={!!service.provenance.tool || !gate.open}
      />
    </section>
  )
}

type Change = Schemas["Change"]
type Amounts = {
  cpuRequest: string
  memoryRequest: string
  cpuLimit: string
  memoryLimit: string
}

/** As kube-apiserver names a field it refused. */
const paths: Record<keyof Amounts, string> = {
  cpuRequest: "requests.cpu",
  memoryRequest: "requests.memory",
  cpuLimit: "limits.cpu",
  memoryLimit: "limits.memory",
}

function Compute({
  service,
  workload,
  disabled,
  refused,
}: {
  service: Service
  workload: Schemas["WorkloadRef"]
  disabled: boolean
  refused: Schemas["FieldCause"][]
}) {
  const t = useT()
  const { cores, memory } = useQuantityFormat()
  const nodes = useQuery(nodesQuery)
  const largest = nodes.data?.items.reduce<Schemas["Resources"] | undefined>(
    (best, node) =>
      !best || node.allocatable.memoryBytes > best.memoryBytes ? node.allocatable : best,
    undefined,
  )
  const scalable = service.kind === "Deployment" || service.kind === "StatefulSet"
  const containers = service.containers.filter((c) => !c.init)
  const initial = {
    replicas: String(service.replicas?.desired ?? 1),
    containers: containers.map((c): Amounts => ({
      cpuRequest: c.resources?.requests?.cpu ?? "",
      memoryRequest: c.resources?.requests?.memory ?? "",
      cpuLimit: c.resources?.limits?.cpu ?? "",
      memoryLimit: c.resources?.limits?.memory ?? "",
    })),
  }
  const changesOf = (value: typeof initial) => {
    const changes: Change[] = []
    if (scalable && value.replicas !== initial.replicas)
      changes.push({ op: "scale", workload, replicas: Number(value.replicas) })
    value.containers.forEach((amounts, index) => {
      const before = initial.containers[index]
      const container = containers[index]
      if (!before || !container || JSON.stringify(amounts) === JSON.stringify(before)) return
      const pick = (cpu: string, memory: string) =>
        Object.fromEntries(
          Object.entries({ cpu: cpu.trim(), memory: memory.trim() }).filter(([, v]) => v),
        )
      changes.push({
        op: "resources",
        workload,
        container: container.name,
        requests: pick(amounts.cpuRequest, amounts.memoryRequest),
        limits: pick(amounts.cpuLimit, amounts.memoryLimit),
      })
    })
    return changes
  }
  const form = useForm({ defaultValues: initial })
  const values = useStore(form.store, (s) => s.values)
  const valid = useStore(form.store, (s) => s.canSubmit)
  const changes = disabled ? [] : changesOf(values)
  useHold(
    "compute",
    changes.length > 0
      ? {
          changes,
          blocked: valid ? undefined : t("form.computeBlocked"),
          scope: { ops: ["scale", "resources"], workload },
        }
      : undefined,
  )
  const amount = ({ value }: { value: string }) =>
    value.trim() === "" || quantity.test(value.trim()) ? undefined : t("form.badQuantity")

  return (
    <form
      aria-label={t("form.compute")}
      className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5"
      onSubmit={(event) => event.preventDefault()}
    >
      {scalable ? (
        <form.Field
          name="replicas"
          validators={{
            onChange: ({ value }) =>
              /^\d+$/.test(value) && Number(value) <= 1000 ? undefined : t("form.badReplicas"),
          }}
        >
          {(f) => (
            <FormField
              label={t("form.replicas")}
              errors={[...f.state.meta.errors, refusedField(refused, "spec.replicas")]}
            >
              <Input
                size="sm"
                mono
                inputMode="numeric"
                value={f.state.value}
                onChange={(e) => f.handleChange(e.target.value)}
                onBlur={f.handleBlur}
                disabled={disabled}
                className="w-24"
              />
            </FormField>
          )}
        </form.Field>
      ) : null}
      {containers.map((container, index) => (
        <fieldset key={container.name} className="flex flex-col gap-3">
          {containers.length > 1 ? (
            <legend className="mb-1 type-label text-tertiary" translate="no">
              {container.name}
            </legend>
          ) : (
            <legend className="sr-only">{container.name}</legend>
          )}
          {(
            [
              [
                ["cpuRequest", t("form.cpuRequest"), t("form.cpuUnset")],
                ["cpuLimit", t("form.cpuLimit"), t("form.noLimit")],
              ],
              [
                ["memoryRequest", t("form.memoryRequest"), t("form.memoryUnset")],
                ["memoryLimit", t("form.memoryLimit"), t("form.noLimit")],
              ],
            ] as const
          ).map((row) => (
            <div key={row[0][0]} className="grid grid-cols-1 items-start gap-3 sm:grid-cols-2">
              {row.map(([key, label, hint]) => (
                <form.Field
                  key={key}
                  name={`containers[${index}].${key}`}
                  validators={{ onChange: amount }}
                >
                  {(f) => (
                    <FormField
                      label={label}
                      hint={f.state.value.trim() === "" ? hint : undefined}
                      errors={[
                        ...f.state.meta.errors,
                        refusedField(
                          refused,
                          `spec.template.spec.containers[${index}].resources.${paths[key]}`,
                        ),
                      ]}
                    >
                      <Input
                        size="sm"
                        mono
                        value={f.state.value}
                        placeholder={t("form.notSet")}
                        onChange={(e) => f.handleChange(e.target.value)}
                        onBlur={f.handleBlur}
                        disabled={disabled}
                      />
                    </FormField>
                  )}
                </form.Field>
              ))}
            </div>
          ))}
        </fieldset>
      ))}
      {largest ? (
        <p className="type-meta text-muted-foreground">
          {t("form.largestNode", cores(largest.cpuMillicores), memory(largest.memoryBytes))}
        </p>
      ) : null}
    </form>
  )
}

type ProbeValues = {
  handler: Schemas["ProbeHandler"] | "none"
  path: string
  port: string
  command: string
  periodSeconds: string
  failureThreshold: string
}

function currentProbe(template: unknown, container: string, kind: ProbeKind): ProbeValues {
  const spec = (template as { spec?: { containers?: Record<string, unknown>[] } } | undefined)?.spec
  const found = spec?.containers?.find((c) => c.name === container)?.[`${kind}Probe`] as
    | Record<string, Record<string, unknown> | number | undefined>
    | undefined
  const http = found?.httpGet as Record<string, unknown> | undefined
  const tcp = found?.tcpSocket as Record<string, unknown> | undefined
  const exec = found?.exec as { command?: string[] } | undefined
  const grpc = found?.grpc as Record<string, unknown> | undefined
  return {
    handler: http ? "HTTPGet" : tcp ? "TCPSocket" : exec ? "Exec" : grpc ? "GRPC" : "none",
    path: String(http?.path ?? ""),
    port: String(http?.port ?? tcp?.port ?? grpc?.port ?? ""),
    command: exec?.command?.join(" ") ?? "",
    periodSeconds: found?.periodSeconds === undefined ? "" : String(found.periodSeconds),
    failureThreshold: found?.failureThreshold === undefined ? "" : String(found.failureThreshold),
  }
}

const summaryHandler = {
  HTTP: "HTTPGet",
  TCP: "TCPSocket",
  gRPC: "GRPC",
  exec: "Exec",
} as const

/** The probe as the project document sums it up, for a workload with no release to read. */
function summaryProbe(probe: Schemas["ProbeSummary"] | null | undefined): ProbeValues {
  return {
    handler: probe ? summaryHandler[probe.handler] : "none",
    path: probe?.path ?? "",
    port: probe?.port ?? "",
    command: "",
    periodSeconds: "",
    failureThreshold: "",
  }
}

function Probes({
  project,
  service,
  endpoints,
  disabled,
}: {
  project: string
  service: Service
  endpoints: Endpoint[]
  disabled: boolean
}) {
  const t = useT()
  const releases = useQuery(releasesQuery(project, service.kind, service.name))
  const template = releases.data?.items.find((r) => r.current)?.template
  const container = service.containers.find((c) => !c.init)
  if (!container) return null
  if (releases.isPending)
    return <p className="type-meta text-muted-foreground">{t("common.loading")}</p>
  // The first port the Services reach this workload on, for the HTTP and TCP presets.
  const port = endpoints[0]?.ports[0]?.targetPort ?? endpoints[0]?.ports[0]?.port
  const workload = { kind: service.kind, name: service.name }
  return (
    <ul className="flex flex-col divide-y divide-v2-border overflow-hidden rounded-xl border border-v2-border bg-card">
      {probeKinds.map((kind) => (
        <ProbeForm
          key={`${kind}-${releases.dataUpdatedAt}`}
          kind={kind}
          current={
            template
              ? currentProbe(template, container.name, kind)
              : summaryProbe(container.probes?.[kind])
          }
          preset={port === undefined ? undefined : String(port)}
          disabled={disabled}
          workload={workload}
          container={container.name}
        />
      ))}
    </ul>
  )
}

function probeRequest(value: ProbeValues): Schemas["ProbeRequest"] | null {
  if (value.handler === "none") return null
  const number = (text: string) => (text.trim() === "" ? undefined : Number(text))
  return {
    handler: value.handler,
    path: value.handler === "HTTPGet" ? value.path || "/" : undefined,
    port: value.handler === "Exec" ? undefined : number(value.port),
    command: value.handler === "Exec" ? value.command.trim().split(/\s+/) : undefined,
    periodSeconds: number(value.periodSeconds),
    failureThreshold: number(value.failureThreshold),
  }
}

function ProbeForm({
  kind,
  current,
  preset,
  disabled,
  workload,
  container,
}: {
  kind: ProbeKind
  current: ProbeValues
  preset?: string
  disabled: boolean
  workload: Schemas["WorkloadRef"]
  container: string
}) {
  const t = useT()
  const form = useForm({ defaultValues: current })
  const values = useStore(form.store, (s) => s.values)
  const canSubmit = useStore(form.store, (s) => s.canSubmit)
  const valid = canSubmit && (values.handler !== "Exec" || values.command.trim() !== "")
  const edited = !disabled && JSON.stringify(values) !== JSON.stringify(current)
  useHold(
    `probe:${kind}`,
    edited
      ? {
          changes: [
            {
              op: "probe",
              workload,
              container,
              probe: kind,
              set: probeRequest(values),
            },
          ],
          blocked: valid ? undefined : t("form.probeBlocked", t(`form.probe.${kind}`)),
          scope: { ops: ["probe"], workload, probe: kind },
        }
      : undefined,
  )
  const positive = ({ value }: { value: string }) =>
    value.trim() === "" || /^[1-9]\d*$/.test(value.trim()) ? undefined : t("form.badNumber")
  const handler = values.handler
  return (
    <li className="flex flex-col gap-3 px-5 py-4">
      <div className="flex min-h-6 items-center gap-1.5">
        <h3 className="text-sm font-medium">{t(`form.probe.${kind}`)}</h3>
        <Hint label={t(`form.probe.${kind}`)}>{t(`form.probeEffect.${kind}`)}</Hint>
        {preset && current.handler === "none" && handler === "none" ? (
          <Button
            type="button"
            variant="ghost"
            size="xs"
            className="ml-auto"
            disabled={disabled}
            onClick={() => {
              form.setFieldValue("handler", "HTTPGet")
              form.setFieldValue("path", "/")
              form.setFieldValue("port", preset)
            }}
          >
            {t("form.probePreset", preset)}
          </Button>
        ) : null}
      </div>
      <form
        aria-label={t(`form.probe.${kind}`)}
        className="grid grid-cols-2 items-start gap-3 sm:grid-cols-5"
        onSubmit={(event) => event.preventDefault()}
      >
        <form.Field name="handler">
          {(f) => (
            <FormField label={t("form.probeCheck")}>
              <Select<ProbeValues["handler"]>
                size="sm"
                value={f.state.value}
                disabled={disabled}
                onValueChange={(handler) => f.handleChange(handler)}
                items={[
                  { value: "none", label: t("form.probeNone") },
                  { value: "HTTPGet", label: "HTTP GET" },
                  { value: "TCPSocket", label: "TCP" },
                  { value: "Exec", label: t("form.probeExec") },
                  // gRPC only for a probe that has it already; the form does not offer it.
                  ...(current.handler === "GRPC"
                    ? [{ value: "GRPC" as const, label: "gRPC" }]
                    : []),
                ]}
              />
            </FormField>
          )}
        </form.Field>
        {handler === "HTTPGet" ? (
          <form.Field name="path">
            {(f) => (
              <FormField label={t("form.probePath")}>
                <Input
                  size="sm"
                  mono
                  value={f.state.value}
                  placeholder="/healthz"
                  disabled={disabled}
                  onChange={(e) => f.handleChange(e.target.value)}
                />
              </FormField>
            )}
          </form.Field>
        ) : null}
        {handler === "HTTPGet" || handler === "TCPSocket" || handler === "GRPC" ? (
          <form.Field name="port">
            {(f) => (
              <FormField label={t("form.probePort")} className="sm:col-start-3">
                <Input
                  size="sm"
                  mono
                  value={f.state.value}
                  placeholder={preset ?? "8080"}
                  disabled={disabled}
                  onChange={(e) => f.handleChange(e.target.value)}
                />
              </FormField>
            )}
          </form.Field>
        ) : null}
        {handler === "Exec" ? (
          <form.Field
            name="command"
            validators={{
              onChange: ({ value }) => (value.trim() ? undefined : t("form.probeCommandMissing")),
            }}
          >
            {(f) => (
              <FormField
                label={t("form.probeCommand")}
                errors={f.state.meta.errors}
                className="sm:col-span-2"
              >
                <Input
                  size="sm"
                  mono
                  value={f.state.value}
                  placeholder="cat /tmp/ready"
                  disabled={disabled}
                  onChange={(e) => f.handleChange(e.target.value)}
                />
              </FormField>
            )}
          </form.Field>
        ) : null}
        {handler === "none" ? null : (
          <>
            <form.Field name="periodSeconds" validators={{ onChange: positive }}>
              {(f) => (
                <FormField
                  label={t("form.probePeriod")}
                  errors={f.state.meta.errors}
                  className="sm:col-start-4"
                >
                  <Input
                    size="sm"
                    mono
                    inputMode="numeric"
                    value={f.state.value}
                    placeholder="10"
                    disabled={disabled}
                    onChange={(e) => f.handleChange(e.target.value)}
                  />
                </FormField>
              )}
            </form.Field>
            <form.Field name="failureThreshold" validators={{ onChange: positive }}>
              {(f) => (
                <FormField label={t("form.probeFailures")} errors={f.state.meta.errors}>
                  <Input
                    size="sm"
                    mono
                    inputMode="numeric"
                    value={f.state.value}
                    placeholder="3"
                    disabled={disabled}
                    onChange={(e) => f.handleChange(e.target.value)}
                  />
                </FormField>
              )}
            </form.Field>
          </>
        )}
      </form>
    </li>
  )
}
