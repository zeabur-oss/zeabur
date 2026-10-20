// A service's settings as forms: compute (replicas, and requests and limits per container against
// the largest node) and health checks (readiness, liveness and startup probes, which also gate a
// rollout), then the sections modules add. Storage is edited on its own tab, next to the volumes.
// Every form ends in the plan dialog; nothing is written from here. This tab is its own chunk, with
// the form library.
import { useForm } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"
import type { ReactNode } from "react"
import CaretDownIcon from "~icons/ph/caret-down"

import type { Schemas } from "@/api/client"
import { nodesQuery, releasesQuery } from "@/api/queries"
import { Hint } from "@/components/bits"
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
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { healthChecksSection } from "@/lib/stores"

const probeKinds = ["readiness", "liveness", "startup"] as const
type ProbeKind = (typeof probeKinds)[number]

export function SettingsTab({
  project,
  service,
  network,
}: {
  project: string
  service: Service
  network: Network
}) {
  const t = useT()
  const planner = usePlanner(project)
  const workload = { kind: service.kind, name: service.name }
  const managed = service.provenance.tool
  const sections = serviceSettingsSections.filter((s) => s.applies(service))
  const editable = service.kind !== "Job"
  const healthOpen = healthChecksSection.useValue()

  return (
    <div className="flex max-w-3xl flex-col gap-8">
      {managed ? (
        <p className="rounded-lg border border-dashed px-3 py-2 text-sm text-muted-foreground">
          {t("common.managedByDetail", managed)}
        </p>
      ) : null}
      {editable ? (
        <>
          <Section title={t("form.compute")}>
            <Compute
              service={service}
              disabled={!!managed}
              onPlan={(changes) =>
                planner.plan({ title: t("title.compute", service.name), changes })
              }
              workload={workload}
              refused={planner.fields}
            />
          </Section>
          {/* Adjusting, not everyday (web/DESIGN.md, "Depth"): folded until opened. */}
          <Collapsible open={healthOpen} onOpenChange={(open) => healthChecksSection.set(open)}>
            <CollapsibleTrigger variant="section">
              {t("form.health")}
              <CaretDownIcon aria-hidden="true" />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="pt-3">
                <Probes
                  project={project}
                  service={service}
                  endpoints={endpointsOf(network, service)}
                  disabled={!!managed}
                  onPlan={(change) =>
                    planner.plan({ title: t("title.probe", service.name), changes: [change] })
                  }
                />
              </div>
            </CollapsibleContent>
          </Collapsible>
        </>
      ) : (
        <p className="text-sm text-muted-foreground">{t("form.jobs")}</p>
      )}
      {sections.map(({ module, Section: Module }) => (
        <Module key={module} project={project} service={service} plan={planner.plan} />
      ))}
      {planner.dialog}
    </div>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-medium">{title}</h2>
      {children}
    </section>
  )
}

// ── Compute ──────────────────────────────────────────────────────────────────────────────────

type Change = Schemas["Change"]
type Amounts = { cpuRequest: string; memoryRequest: string; cpuLimit: string; memoryLimit: string }

/** Where each amount is in a container, as kube-apiserver names a field it refused. */
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
  onPlan,
  refused,
}: {
  service: Service
  workload: Schemas["WorkloadRef"]
  disabled: boolean
  onPlan: (changes: Change[]) => void
  /** The fields the cluster refused in the last plan, marked where they are. */
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
  const form = useForm({
    defaultValues: initial,
    onSubmit: ({ value }) => {
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
      if (changes.length > 0) onPlan(changes)
    },
  })
  const amount = ({ value }: { value: string }) =>
    value.trim() === "" || quantity.test(value.trim()) ? undefined : t("form.badQuantity")

  return (
    <form
      className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-4"
      onSubmit={(event) => {
        event.preventDefault()
        void form.handleSubmit()
      }}
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
        <fieldset key={container.name} className="flex flex-col gap-2">
          <legend className="mb-1 font-mono text-xs" translate="no">
            {container.name}
          </legend>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            {(
              [
                ["cpuRequest", t("form.cpuRequest")],
                ["memoryRequest", t("form.memoryRequest")],
                ["cpuLimit", t("form.cpuLimit")],
                ["memoryLimit", t("form.memoryLimit")],
              ] as const
            ).map(([key, label]) => (
              <form.Field
                key={key}
                name={`containers[${index}].${key}`}
                validators={{ onChange: amount }}
              >
                {(f) => (
                  <FormField
                    label={label}
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
                      placeholder={key.startsWith("cpu") ? "250m" : "256Mi"}
                      onChange={(e) => f.handleChange(e.target.value)}
                      onBlur={f.handleBlur}
                      disabled={disabled}
                    />
                  </FormField>
                )}
              </form.Field>
            ))}
          </div>
        </fieldset>
      ))}
      <div className="flex flex-wrap items-center gap-3">
        {largest ? (
          <span className="type-meta text-muted-foreground">
            {t("form.largestNode", cores(largest.cpuMillicores), memory(largest.memoryBytes))}
          </span>
        ) : null}
        <form.Subscribe selector={(s) => [s.canSubmit, s.isDirty] as const}>
          {([canSubmit, dirty]) => (
            <Button
              type="submit"
              size="xs"
              className="ml-auto"
              disabled={disabled || !canSubmit || !dirty}
            >
              {t("edit.review")}
            </Button>
          )}
        </form.Subscribe>
      </div>
    </form>
  )
}

// ── Health checks ────────────────────────────────────────────────────────────────────────────

type ProbeValues = {
  handler: Schemas["ProbeHandler"] | "none"
  path: string
  port: string
  command: string
  periodSeconds: string
  failureThreshold: string
}

/** The container's probe as the current release's template holds it. */
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

function Probes({
  project,
  service,
  endpoints,
  disabled,
  onPlan,
}: {
  project: string
  service: Service
  /** The Kubernetes Services selecting its pods. */
  endpoints: Endpoint[]
  disabled: boolean
  onPlan: (change: Change) => void
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
  return (
    <div className="flex flex-col gap-3">
      {probeKinds.map((kind) => (
        <ProbeForm
          key={`${kind}-${releases.dataUpdatedAt}`}
          kind={kind}
          current={currentProbe(template, container.name, kind)}
          preset={port === undefined ? undefined : String(port)}
          disabled={disabled}
          onPlan={(set) =>
            onPlan({
              op: "probe",
              workload: { kind: service.kind, name: service.name },
              container: container.name,
              probe: kind,
              set,
            })
          }
        />
      ))}
    </div>
  )
}

function ProbeForm({
  kind,
  current,
  preset,
  disabled,
  onPlan,
}: {
  kind: ProbeKind
  current: ProbeValues
  preset?: string
  disabled: boolean
  onPlan: (set: Schemas["ProbeRequest"] | null) => void
}) {
  const t = useT()
  const form = useForm({
    defaultValues: current,
    onSubmit: ({ value }) => {
      if (value.handler === "none") {
        onPlan(null)
        return
      }
      const number = (text: string) => (text.trim() === "" ? undefined : Number(text))
      onPlan({
        handler: value.handler,
        path: value.handler === "HTTPGet" ? value.path || "/" : undefined,
        port: value.handler === "Exec" ? undefined : number(value.port),
        command: value.handler === "Exec" ? value.command.trim().split(/\s+/) : undefined,
        periodSeconds: number(value.periodSeconds),
        failureThreshold: number(value.failureThreshold),
      })
    },
  })
  const positive = ({ value }: { value: string }) =>
    value.trim() === "" || /^[1-9]\d*$/.test(value.trim()) ? undefined : t("form.badNumber")
  return (
    <form
      className="flex flex-col gap-3 rounded-xl border border-v2-border bg-card p-4"
      onSubmit={(event) => {
        event.preventDefault()
        void form.handleSubmit()
      }}
    >
      <div className="flex items-center gap-1.5">
        <h3 className="text-sm font-medium">{t(`form.probe.${kind}`)}</h3>
        <Hint label={t(`form.probe.${kind}`)}>{t(`form.probeEffect.${kind}`)}</Hint>
      </div>
      <form.Subscribe selector={(s) => s.values.handler}>
        {(handler) => (
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
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
                  <FormField label={t("form.probePort")}>
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
                  onChange: ({ value }) =>
                    value.trim() ? undefined : t("form.probeCommandMissing"),
                }}
              >
                {(f) => (
                  <FormField label={t("form.probeCommand")} errors={f.state.meta.errors}>
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
                    <FormField label={t("form.probePeriod")} errors={f.state.meta.errors}>
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
          </div>
        )}
      </form.Subscribe>
      <div className="flex items-center gap-2">
        {preset && current.handler === "none" ? (
          <Button
            type="button"
            variant="ghost"
            size="xs"
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
        <form.Subscribe selector={(s) => [s.canSubmit, s.isDirty] as const}>
          {([canSubmit, dirty]) => (
            <Button
              type="submit"
              size="xs"
              className="ml-auto"
              disabled={disabled || !canSubmit || !dirty}
            >
              {t("edit.review")}
            </Button>
          )}
        </form.Subscribe>
      </div>
    </form>
  )
}
