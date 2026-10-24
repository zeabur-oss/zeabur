import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretRightIcon from "~icons/ph/caret-right"

import { isStatus, isUnreachable } from "@/api/client"
import { clusterUsageQuery, nodesQuery, projectQuery, usageQuery } from "@/api/queries"
import { Segmented } from "@/components/bits"
import { LoadingRows } from "@/components/page"
import { Problem } from "@/components/problem"
import { StatusDot } from "@/components/status"
import { NotRead } from "@/components/unreachable"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { useQuantityFormat } from "@/lib/quantity"
import { cn } from "@/lib/utils"
import {
  groups,
  largest,
  lead,
  maximum,
  nearMaximum,
  reserved,
  shownName,
  together,
  type ClusterUsage,
  type NamespaceUsage,
  type Resource,
} from "@/model/cluster-usage"
import { serviceId } from "@/model/service"
import { hasUsage, serviceUsage } from "@/model/usage"
import { Forbidden, overviewRole } from "@/routes/cluster/forbidden"
import { Section } from "@/routes/cluster/section"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { Meter } from "@zeabur/design-system/components/meter"

const ROWS_SHOWN = 8
const SERVICES_SHOWN = 5

interface Scale {
  resource: Resource
  measured: boolean
  /** The largest row's use or reservation; every bar is drawn against it. */
  top: Record<Resource, number>
  /** The nodes' allocatable, when known. */
  cluster: Partial<Record<Resource, number>>
}

/** Amounts as the page writes them: measured CPU with two decimals, reserved CPU with one. */
function useAmount() {
  const t = useT()
  const f = useQuantityFormat()
  return (resource: Resource, value: number, measured: boolean) =>
    resource === "memory"
      ? f.memory(value)
      : measured
        ? f.cpu(value)
        : t("cluster.cores", f.cores(value))
}

export function UsageSection() {
  const t = useT()
  const usage = useQuery(clusterUsageQuery)
  const [resource, setResource] = useState<Resource>("memory")
  const data = usage.data
  return (
    <Section
      id="cluster-usage"
      title={t("clusterUsage.title")}
      action={
        data && data.items.length > 1 ? (
          <Segmented
            label={t("clusterUsage.sort")}
            value={resource}
            onChange={setResource}
            options={[
              { value: "memory", label: t("cluster.memory") },
              { value: "cpu", label: t("cluster.cpu") },
            ]}
          />
        ) : null
      }
    >
      {usage.isPending ? (
        <LoadingRows label={t("common.loading")} rows={2} />
      ) : usage.isError ? (
        <NotShown error={usage.error} />
      ) : (
        <Usage data={usage.data} resource={resource} />
      )}
    </Section>
  )
}

function NotShown({ error }: { error: Error }) {
  const t = useT()
  if (isUnreachable(error)) return <NotRead />
  if (!isStatus(error, 403)) return <Problem error={error} />
  return (
    <Forbidden
      error={error}
      what={t("clusterUsage.whatPods")}
      missing={[{ group: "", resources: ["pods"] }]}
      role={overviewRole}
    />
  )
}

type Allocatable = { cpuMillicores: number; memoryBytes: number } | undefined

function scaleOf(data: ClusterUsage, resource: Resource, allocatable: Allocatable): Scale {
  const top = (r: Resource) =>
    Math.max(...data.items.map((i) => Math.max(lead(i, r), reserved(i, r))), 1)
  return {
    resource,
    measured: data.metrics === "available",
    top: { cpu: top("cpu"), memory: top("memory") },
    cluster: {
      cpu: allocatable?.cpuMillicores || undefined,
      memory: allocatable?.memoryBytes || undefined,
    },
  }
}

function Usage({ data, resource }: { data: ClusterUsage; resource: Resource }) {
  const t = useT()
  const allocatable = useQuery(nodesQuery).data?.totals.allocatable
  if (data.items.length === 0)
    return <p className="text-muted-foreground">{t("clusterUsage.nothingRunning")}</p>
  const scale = scaleOf(data, resource, allocatable)
  const { projects, others, system } = groups(data.items, resource)
  const known = data.items.some((i) => i.adopted !== undefined)
  return (
    <div className="flex flex-col gap-5">
      <Summary data={data} scale={scale} />
      <Notes data={data} />
      {projects.length > 0 ? (
        <Group title={t("clusterUsage.projects")} items={projects} scale={scale} />
      ) : null}
      {others.length > 0 ? (
        <Group
          title={known ? t("clusterUsage.others") : t("clusterUsage.namespaces")}
          items={others}
          scale={scale}
        />
      ) : null}
      {system.length > 0 ? <SystemGroup items={system} scale={scale} /> : null}
    </div>
  )
}

/** The namespace that uses the most, then what each group uses together. */
function Summary({ data, scale }: { data: ClusterUsage; scale: Scale }) {
  const t = useT()
  const { percent } = useQuantityFormat()
  const amount = useAmount()
  const { resource, measured } = scale
  const first = largest(data.items, resource)
  const { projects, others, system } = groups(data.items, resource)
  const say = (items: NamespaceUsage[]) => amount(resource, together(items, resource), measured)
  const key = measured
    ? resource === "memory"
      ? "clusterUsage.mostMemory"
      : "clusterUsage.mostCpu"
    : resource === "memory"
      ? "clusterUsage.reservesMostMemory"
      : "clusterUsage.reservesMostCpu"
  const parts = [
    projects.length > 0 ? t("clusterUsage.projectsTotal", say(projects)) : "",
    others.length > 0 ? t("clusterUsage.othersTotal", say(others)) : "",
    system.length > 0 ? t("clusterUsage.systemTotal", say(system)) : "",
  ].filter(Boolean)
  return (
    <div className="flex flex-col gap-1 rounded-lg bg-plane px-4 py-4">
      {first ? (
        <p className="font-medium">
          {t(
            key,
            shownName(first),
            amount(resource, lead(first, resource), measured),
            scale.cluster[resource]
              ? percent(lead(first, resource) / scale.cluster[resource])
              : undefined,
          )}
        </p>
      ) : null}
      <p className="type-meta text-muted-foreground">{parts.join(t("common.sep"))}</p>
    </div>
  )
}

function Notes({ data }: { data: ClusterUsage }) {
  const t = useT()
  const lines: string[] = []
  if (data.metrics !== "available") lines.push(t(`clusterUsage.unknown.${data.metrics}`))
  if (data.podScan === "partial") lines.push(t("cluster.partialPods", data.podsScanned))
  const metricsRefused = data.metrics === "unreadable"
  const namespacesRefused = data.unreadable.some((u) => u.resource === "namespaces")
  return (
    <>
      {lines.length > 0 ? (
        <ul className="flex flex-col gap-1 type-meta text-muted-foreground">
          {lines.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      ) : null}
      {metricsRefused || namespacesRefused ? (
        <Forbidden
          what={metricsRefused ? t("clusterUsage.whatUsage") : t("clusterUsage.whatProjects")}
          missing={[
            ...(metricsRefused ? [{ group: "metrics.k8s.io", resources: ["pods"] }] : []),
            ...(namespacesRefused ? [{ group: "", resources: ["namespaces"] }] : []),
          ]}
          role={overviewRole}
        />
      ) : null}
    </>
  )
}

function Group({ title, items, scale }: { title: string; items: NamespaceUsage[]; scale: Scale }) {
  const t = useT()
  const [all, setAll] = useState(false)
  const shown = all ? items : items.slice(0, ROWS_SHOWN)
  return (
    <div className="flex flex-col gap-1">
      <h3 className="flex items-baseline gap-2 type-meta font-medium text-muted-foreground">
        {title}
        <span className="font-normal">{items.length}</span>
      </h3>
      <Rows items={shown} scale={scale} label={title} />
      {items.length > ROWS_SHOWN ? (
        <div>
          <Button variant="ghost" size="sm" onClick={() => setAll(!all)} aria-expanded={all}>
            <CaretDownIcon aria-hidden="true" data-icon="inline-start" />
            {all ? t("cluster.fewer") : t("cluster.more", items.length - ROWS_SHOWN)}
          </Button>
        </div>
      ) : null}
    </div>
  )
}

/** Kubernetes' own namespaces, folded behind what they use together. */
function SystemGroup({ items, scale }: { items: NamespaceUsage[]; scale: Scale }) {
  const t = useT()
  const amount = useAmount()
  return (
    <Collapsible>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {t(
          "clusterUsage.systemTotal",
          amount(scale.resource, together(items, scale.resource), scale.measured),
        )}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <Rows items={items} scale={scale} label={t("clusterUsage.system")} />
      </CollapsibleContent>
    </Collapsible>
  )
}

function Rows({ items, scale, label }: { items: NamespaceUsage[]; scale: Scale; label: string }) {
  return (
    <ul aria-label={label} className="flex flex-col">
      {items.map((item) => (
        <NamespaceRow key={item.namespace} item={item} scale={scale} />
      ))}
    </ul>
  )
}

function NamespaceRow({ item, scale }: { item: NamespaceUsage; scale: Scale }) {
  const t = useT()
  const project = item.adopted === true
  const unmeasured = item.usage ? item.pods - (item.usage.pods ?? 0) : 0
  const meta = [
    t("clusterUsage.pods", item.pods),
    unmeasured > 0 ? t("clusterUsage.notMeasured", unmeasured) : "",
  ].filter(Boolean)
  const head = (
    <div className="flex min-w-48 flex-1 flex-col gap-0.5">
      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
        {project ? (
          <Link
            to="/projects/$project"
            params={{ project: item.namespace }}
            className="font-semibold hover:underline"
            translate="no"
          >
            {shownName(item)}
          </Link>
        ) : (
          <span className="font-semibold" translate="no">
            {item.namespace}
          </span>
        )}
        {nearMaximum(item) ? (
          <span className="flex items-center gap-1.5 type-meta text-warn-strong">
            <StatusDot health="warn" />
            {t("clusterUsage.nearMaximum")}
          </span>
        ) : null}
      </div>
      <span className="type-meta text-muted-foreground">{meta.join(t("common.sep"))}</span>
    </div>
  )
  const gauges = (
    <div className="grid w-full grid-cols-2 items-start gap-x-4 gap-y-3 sm:flex sm:w-auto sm:gap-x-6">
      <Gauge item={item} resource="cpu" scale={scale} />
      <Gauge item={item} resource="memory" scale={scale} />
    </div>
  )
  if (!project)
    return (
      <li className="flex flex-wrap items-center gap-x-6 gap-y-3 border-t border-rule py-3">
        {head}
        {gauges}
      </li>
    )
  return (
    <li className="border-t border-rule py-3">
      <Collapsible>
        <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
          <div className="flex min-w-48 flex-1 flex-col gap-1">
            {head}
            <CollapsibleTrigger variant="disclosure">
              <CaretRightIcon aria-hidden="true" />
              {t("project.services")}
            </CollapsibleTrigger>
          </div>
          {gauges}
        </div>
        <CollapsibleContent>
          <Services project={item.namespace} resource={scale.resource} />
        </CollapsibleContent>
      </Collapsible>
    </li>
  )
}

/** A gauge's heading, the line under its bar, and the whole as one label for assistive tech. */
function useGaugeText(item: NamespaceUsage, resource: Resource, scale: Scale) {
  const t = useT()
  const { percent } = useQuantityFormat()
  const amount = useAmount()
  const { measured } = scale
  const cluster = scale.cluster[resource]
  const value = lead(item, resource)
  const asked = reserved(item, resource)
  const most = maximum(item, resource)
  const title = resource === "memory" ? t("cluster.memory") : t("cluster.cpu")
  const details = [
    cluster ? t("clusterUsage.ofCluster", percent(value / cluster)) : "",
    measured && asked > 0 ? t("clusterUsage.reserved", amount(resource, asked, false)) : "",
    most ? t("cluster.limits", amount(resource, most, false)) : "",
  ].filter(Boolean)
  const now = measured
    ? t("cluster.used", amount(resource, value, true))
    : t("clusterUsage.reserved", amount(resource, value, false))
  const label = [title, now, ...details].join(t("common.sep"))
  return { title, details: details.join(t("common.sep")), label }
}

function Gauge({
  item,
  resource,
  scale,
}: {
  item: NamespaceUsage
  resource: Resource
  scale: Scale
}) {
  const amount = useAmount()
  const { title, details, label } = useGaugeText(item, resource, scale)
  const whole = scale.top[resource]
  const value = lead(item, resource)
  const asked = reserved(item, resource)
  return (
    <div className="flex min-w-0 flex-col gap-1 sm:w-44">
      <div className="flex items-baseline justify-between gap-2 type-meta">
        <span className="text-muted-foreground">{title}</span>
        <span className={cn(resource === scale.resource && "font-medium")}>
          {amount(resource, value, scale.measured)}
        </span>
      </div>
      <Meter
        size="sm"
        value={value / whole}
        mark={scale.measured && asked > 0 ? asked / whole : undefined}
        tone={resource === "memory" && nearMaximum(item) ? "warn" : "ok"}
        label={label}
      />
      {details ? <span className="type-meta text-muted-foreground">{details}</span> : null}
    </div>
  )
}

/** A project's services with what each uses now, from the project's own usage read. */
function Services({ project, resource }: { project: string; resource: Resource }) {
  const t = useT()
  const amount = useAmount()
  const [all, setAll] = useState(false)
  const read = useQuery(projectQuery(project, { poll: false }))
  const usage = useQuery(usageQuery(project))
  if (read.isPending || usage.isPending)
    return (
      <div className="pt-3">
        <LoadingRows label={t("common.loading")} rows={2} />
      </div>
    )
  if (read.isError)
    return (
      <p className="pt-3 type-meta text-muted-foreground">{t("clusterUsage.servicesUnreadable")}</p>
    )
  const rows = read.data.services
    .filter(hasUsage)
    .map((service) => ({ service, used: serviceUsage(service, usage.data) }))
    .toSorted(
      (a, b) =>
        (b.used?.[resource] ?? -1) - (a.used?.[resource] ?? -1) ||
        a.service.name.localeCompare(b.service.name),
    )
  if (rows.length === 0)
    return <p className="pt-3 type-meta text-muted-foreground">{t("clusterUsage.noServices")}</p>
  const shown = all ? rows : rows.slice(0, SERVICES_SHOWN)
  return (
    <div className="mt-3 flex flex-col gap-1">
      <ul
        aria-label={t("project.services")}
        className="flex flex-col rounded-lg bg-plane px-4 py-1"
      >
        {shown.map(({ service, used }) => (
          <li
            key={serviceId(service)}
            className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-rule py-2 type-meta first:border-t-0"
          >
            <Link
              {...serviceLink(project, service)}
              className="min-w-0 flex-1 truncate font-medium hover:underline"
              translate="no"
            >
              {service.name}
            </Link>
            {used?.nearLimit ? (
              <span className="flex items-center gap-1.5 text-warn-strong">
                <StatusDot health="warn" />
                {t("clusterUsage.nearMaximum")}
              </span>
            ) : null}
            {used ? (
              <span className="flex gap-4 tabular-nums">
                <span className="w-20 text-right">{amount("cpu", used.cpu ?? 0, true)}</span>
                <span className="w-20 text-right">{amount("memory", used.memory ?? 0, true)}</span>
              </span>
            ) : (
              <span className="text-muted-foreground">{t("usage.noSampleShort")}</span>
            )}
          </li>
        ))}
      </ul>
      {rows.length > SERVICES_SHOWN ? (
        <div>
          <Button variant="ghost" size="sm" onClick={() => setAll(!all)} aria-expanded={all}>
            <CaretDownIcon aria-hidden="true" data-icon="inline-start" />
            {all ? t("cluster.fewer") : t("cluster.more", rows.length - SERVICES_SHOWN)}
          </Button>
        </div>
      ) : null}
    </div>
  )
}
