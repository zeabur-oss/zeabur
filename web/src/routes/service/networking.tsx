import { useForm } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"
import { Fragment, lazy, Suspense, useState, type ReactElement, type ReactNode } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import ArrowBendUpRightIcon from "~icons/ph/arrow-bend-up-right"
import ArrowRightIcon from "~icons/ph/arrow-right"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretDownIcon from "~icons/ph/caret-down"
import ClockIcon from "~icons/ph/clock"
import GlobeIcon from "~icons/ph/globe"
import HourglassMediumIcon from "~icons/ph/hourglass-medium-bold"
import LockKeyIcon from "~icons/ph/lock-key"
import LockSimpleBoldIcon from "~icons/ph/lock-simple-bold"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlugsIcon from "~icons/ph/plugs"
import PlusIcon from "~icons/ph/plus"
import SlidersHorizontalIcon from "~icons/ph/sliders-horizontal"
import TrashIcon from "~icons/ph/trash"

import { clusterQuery, traceQuery } from "@/api/queries"
import { DnsHint, DomainSetup, DomainState } from "@/components/domain-setup"
import { GateTip, useGate } from "@/components/gate"
import { ReviewWord, useDraft } from "@/components/plan/draft"
import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { OpenInTerminal } from "@/components/dock/open-in-terminal"
import { usePlanner } from "@/components/plan/planner"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { FilterField } from "@/components/filter-field"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { Button } from "@zeabur/design-system/components/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@zeabur/design-system/components/empty"
import { Input } from "@zeabur/design-system/components/input"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { createdKey, entrance } from "@/lib/created"
import { cn } from "@/lib/utils"
import { domainsLead, filterRoutes, needsSetup, plainFirst } from "@/model/domains"
import { stagedDomains, type DomainEdit } from "@/model/draft"
import type { Gate } from "@/model/rights"
import { isLongList } from "@/model/lists"
import {
  endpointsOf,
  hostUrl,
  publicRoutesOf,
  routesOf,
  type Endpoint,
  type Network,
  type PublicRoute,
  type Service,
} from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import { Reachability } from "@/routes/service/reachability"

const DomainAccess = lazy(() => import("@/routes/service/domain-access"))

// RFC 1123 host names, with the leading wildcard label Ingress and HTTPRoute allow. A hint only:
// the plan checks the name again.
const hostname = /^(\*\.)?([a-z0-9]([-a-z0-9]*[a-z0-9])?\.)*[a-z0-9]([-a-z0-9]*[a-z0-9])?$/
const isHost = (text: string) => {
  const host = text.trim().toLowerCase()
  return host.length > 0 && host.length <= 253 && hostname.test(host)
}

const head = "px-3 py-2 text-left type-meta font-medium text-muted-foreground whitespace-nowrap"
const cell = "px-3 py-2 align-middle max-sm:p-0"
const thead = "border-b bg-muted/30 max-sm:hidden"
const row =
  "max-sm:flex max-sm:flex-wrap max-sm:items-center max-sm:gap-x-3 max-sm:gap-y-1.5 max-sm:px-4 max-sm:py-3"

export function NetworkingTab({
  project,
  service,
  services,
  network,
  reachability,
  unreadable,
}: {
  project: string
  service: Service
  services: Service[]
  network: Network
  /** Absent when it was not read. */
  reachability?: Schemas["NetworkPolicyReachability"]
  unreadable: Schemas["UnreadableView"][]
}) {
  const t = useT()
  const routes = publicRoutesOf(network, service)
  const endpoints = endpointsOf(network, service)
  const ready = service.pods.filter((p) => p.ready).length
  const planner = usePlanner(project)
  const [adding, setAdding] = useState<string>()
  const [renaming, setRenaming] = useState<{ from: string; to: string }>()
  const [access, setAccess] = useState<{ route: Schemas["Route"]; host: string }>()
  const managed = service.provenance.tool ?? undefined
  const workload = { kind: service.kind, name: service.name }
  const draft = useDraft()
  const staged = stagedDomains(
    draft?.changes ?? [],
    workload,
    routes.map((r) => r.ingress),
  )
  const editOf = (route: PublicRoute) =>
    route.host ? staged.edits.get(`${route.ingress}/${route.host}`) : undefined
  const listed = routes.length + staged.added.length
  const [query, setQuery] = useState("")
  const [opened, setOpened] = useState<string>()
  const long = isLongList(routes.length, 10)
  const shown = plainFirst(long ? filterRoutes(routes, query) : routes)
  const lead = domainsLead(routes)
  const undo = (change: DomainEdit) => (
    <Button variant="ghost" size="xs" onClick={() => draft?.unstage([change])}>
      <ArrowCounterClockwiseIcon aria-hidden="true" data-icon="inline-start" />
      {t("vars.undo")}
    </Button>
  )
  const fullRoute = (route: PublicRoute) =>
    routesOf(network, service).find((r) => r.name === route.ingress)
  const routeOf = (route: PublicRoute): Schemas["RouteRef"] => ({
    kind: (fullRoute(route)?.origin.kind ?? "Ingress") as Schemas["RouteKind"],
    name: route.ingress,
  })
  // Redirects and passwords are Traefik middlewares, which only an Ingress that Traefik serves
  // can list.
  const traefikIngress = (route: PublicRoute) => {
    const full = fullRoute(route)
    return full?.origin.kind === "Ingress" && (!full.class || full.class.includes("traefik"))
      ? full
      : undefined
  }
  const canWrite = endpoints.length > 0 && !managed
  const domainGate = useGate(project, "changeDomains")
  const refusedRoutes = refusedAmong(unreadable, refusedPlaces.routes)
  const refusedEndpoints = refusedAmong(unreadable, refusedPlaces.endpoints)
  const trace = useQuery({
    ...traceQuery(project, service.kind, service.name),
    enabled: routes.length > 0 || adding !== undefined,
  })
  const suffix = useQuery(clusterQuery).data?.domains?.suffix ?? undefined
  const checkOf = (host?: string) =>
    host
      ? (trace.data?.routes.find((r) => r.kind === "domain" && r.address === host)?.domain ??
        undefined)
      : undefined
  const hint = (host: string) => (
    <DnsHint host={host} addresses={trace.data?.ingressAddresses ?? []} suffix={suffix} />
  )

  const scheduled = service.kind === "CronJob" || service.kind === "Job"
  const noAddress =
    listed === 0 &&
    endpoints.length === 0 &&
    adding === undefined &&
    refusedRoutes.length === 0 &&
    refusedEndpoints.length === 0
  const expose =
    service.kind === "Deployment"
      ? `kubectl -n ${service.namespace} expose deployment ${service.name}`
      : undefined

  return (
    <div className="flex flex-col gap-8">
      {noAddress ? (
        <Empty variant="outline" size="inline">
          <EmptyMedia variant="glyph">
            {scheduled ? <ClockIcon aria-hidden="true" /> : <PlugsIcon aria-hidden="true" />}
          </EmptyMedia>
          <EmptyHeader>
            <EmptyTitle>
              {scheduled ? t("net.scheduledTitle", service.name) : t("net.noneTitle", service.name)}
            </EmptyTitle>
            <EmptyDescription>
              {scheduled ? t("net.scheduledBody") : t("net.noneBody")}
            </EmptyDescription>
            {scheduled ? null : <ExposeHint project={project} command={expose} />}
          </EmptyHeader>
        </Empty>
      ) : (
        <>
          <section className="flex min-w-0 flex-col gap-3" aria-labelledby="net-public">
            <SectionHeader
              id="net-public"
              title={t("net.publicTitle")}
              count={listed > 0 ? listed : undefined}
              description={
                lead.hosts > 0
                  ? t(
                      "net.publicLead",
                      lead.hosts,
                      service.name,
                      ready,
                      service.pods.length,
                      lead.plain,
                    )
                  : t("net.publicAbout")
              }
              action={
                listed > 0 ? (
                  <Managed tool={managed} gate={domainGate}>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={!canWrite || adding !== undefined}
                      onClick={() => setAdding("")}
                    >
                      <PlusIcon aria-hidden="true" data-icon="inline-start" />
                      {t("net.addDomain")}
                    </Button>
                  </Managed>
                ) : undefined
              }
            />
            {adding !== undefined ? (
              <HostForm
                value={adding}
                hint={hint}
                onCancel={() => setAdding(undefined)}
                onReview={(host) => {
                  planner.plan({
                    title: t("title.addDomain", host),
                    changes: [{ op: "addDomain", workload, hostname: host }],
                  })
                  setAdding(undefined)
                }}
              />
            ) : null}
            <RefusedKinds unreadable={refusedRoutes} />
            {listed === 0 ? (
              adding === undefined && refusedRoutes.length === 0 ? (
                <Empty variant="outline" size="inline">
                  <EmptyMedia variant="glyph">
                    <GlobeIcon aria-hidden="true" />
                  </EmptyMedia>
                  <EmptyHeader>
                    <EmptyTitle>{t("net.publicEmpty", service.name)}</EmptyTitle>
                    <EmptyDescription>
                      {endpoints.length > 0
                        ? t("net.publicEmptyBody")
                        : t("net.publicNeedsPrivate")}
                    </EmptyDescription>
                  </EmptyHeader>
                  {endpoints.length > 0 ? (
                    <EmptyContent>
                      <Managed tool={managed} gate={domainGate}>
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={!canWrite}
                          onClick={() => setAdding("")}
                        >
                          <PlusIcon aria-hidden="true" data-icon="inline-start" />
                          {t("net.addDomain")}
                        </Button>
                      </Managed>
                    </EmptyContent>
                  ) : null}
                </Empty>
              ) : null
            ) : (
              <>
                {long ? (
                  <FilterField value={query} onChange={setQuery} label={t("net.filter")} />
                ) : null}
                <Table label={t("net.publicTitle")} minWidth="sm:min-w-160">
                  <thead className={thead}>
                    <tr>
                      <th className={`${head} pl-4`}>{t("table.domain")}</th>
                      <th className={head}>{t("net.path")}</th>
                      <th className={head}>{t("net.sendsTo")}</th>
                      <th className={`${head} pr-4`}>
                        <span className="sr-only">{t("table.actions")}</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y max-sm:block">
                    {shown.length === 0 ? (
                      <tr className={row}>
                        <td
                          colSpan={4}
                          className={`${cell} px-4 py-3 text-xs text-muted-foreground`}
                        >
                          {t("net.noMatch")}
                        </td>
                      </tr>
                    ) : null}
                    {shown.map((route, i) => {
                      const edit = editOf(route)
                      const key = `${route.ingress}/${route.host ?? ""}/${route.path ?? ""}/${i}`
                      const folded = opened !== key && !edit && renaming?.from !== route.host
                      const more = folded ? "max-sm:hidden" : undefined
                      const check = edit ? undefined : checkOf(route.host)
                      const firstOfHost = shown.findIndex((r) => r.host === route.host) === i
                      const traefik = traefikIngress(route)
                      const redirected = traefik?.redirects?.some((r) => r.host === route.host)
                      return (
                        <Fragment key={key}>
                          <tr
                            className={cn(
                              row,
                              route.host && entrance(createdKey.domain(route.host)),
                              check && firstOfHost && needsSetup(check) && "border-b-0",
                            )}
                          >
                            <td className={`${cell} max-w-72 pl-4 max-sm:w-full max-sm:max-w-none`}>
                              {renaming && route.host === renaming.from ? (
                                <HostForm
                                  value={renaming.to}
                                  hint={hint}
                                  onCancel={() => setRenaming(undefined)}
                                  onReview={(to) => {
                                    planner.plan({
                                      title: t("title.renameDomain", renaming.from, to),
                                      service: workload,
                                      changes: [
                                        {
                                          op: "renameDomain",
                                          route: routeOf(route),
                                          from: renaming.from,
                                          to,
                                        },
                                      ],
                                    })
                                    setRenaming(undefined)
                                  }}
                                />
                              ) : edit?.op === "renameDomain" ? (
                                <PendingHost host={edit.to} was={edit.from} state="changed" />
                              ) : edit ? (
                                <PendingHost host={edit.hostname} state="removed" />
                              ) : (
                                <span className="flex min-w-0 flex-col gap-1 max-sm:w-full">
                                  <span className="flex min-w-0 items-center gap-2">
                                    <Host route={route} certificate={check?.certificate} />
                                    {redirected ? (
                                      <span className="flex shrink-0 items-center gap-1 type-meta text-muted-foreground">
                                        <ArrowBendUpRightIcon aria-hidden="true" />
                                        {t("domainAccess.redirects")}
                                      </span>
                                    ) : null}
                                    {traefik?.basicAuth ? (
                                      <span className="flex shrink-0 items-center gap-1 type-meta text-muted-foreground">
                                        <LockKeyIcon aria-hidden="true" />
                                        {t("domainAccess.protected")}
                                      </span>
                                    ) : null}
                                    <span className="flex-1 sm:hidden" />
                                    {route.host && !route.https ? (
                                      <span className="shrink-0 type-meta text-warn-strong">
                                        {t("net.noHttps")}
                                      </span>
                                    ) : route.https ? (
                                      <span className="shrink-0 type-meta text-muted-foreground sm:hidden">
                                        {t("net.https")}
                                      </span>
                                    ) : null}
                                    <Button
                                      variant="muted"
                                      size="icon-xs"
                                      className="sm:hidden"
                                      aria-expanded={!folded}
                                      aria-label={t(
                                        "net.routeMore",
                                        route.host ?? t("net.anyHost"),
                                      )}
                                      onClick={() => setOpened(folded ? key : undefined)}
                                    >
                                      <CaretDownIcon
                                        aria-hidden="true"
                                        className={cn(
                                          "motion-safe:transition-transform",
                                          !folded && "rotate-180",
                                        )}
                                      />
                                    </Button>
                                  </span>
                                  {check && firstOfHost ? <DomainState check={check} /> : null}
                                </span>
                              )}
                            </td>
                            <td className={cn(cell, more)}>
                              <Path route={route} />
                            </td>
                            <td className={cn(cell, more)}>
                              <span className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                                <ArrowRightIcon
                                  className="size-3 shrink-0 text-muted-foreground"
                                  aria-hidden="true"
                                />
                                <span className="whitespace-nowrap">
                                  {route.port
                                    ? t("net.port", portNumber(endpoints, route))
                                    : t("net.anyPort")}
                                </span>
                              </span>
                            </td>
                            <td className={cn(cell, "pr-4 max-sm:ml-auto", more)}>
                              {edit ? (
                                <span className="flex justify-end">{undo(edit)}</span>
                              ) : route.host ? (
                                <span className="flex items-center justify-end gap-0.5">
                                  {traefik ? (
                                    <Managed tool={managed} gate={domainGate}>
                                      <Button
                                        variant="muted"
                                        size="icon-xs"
                                        disabled={Boolean(managed)}
                                        aria-label={t("domainAccess.open", route.host)}
                                        title={t("domainAccess.open", route.host)}
                                        onClick={() =>
                                          route.host
                                            ? setAccess({ route: traefik, host: route.host })
                                            : undefined
                                        }
                                      >
                                        <SlidersHorizontalIcon aria-hidden="true" />
                                      </Button>
                                    </Managed>
                                  ) : null}
                                  <Managed tool={managed} gate={domainGate}>
                                    <Button
                                      variant="muted"
                                      size="icon-xs"
                                      disabled={Boolean(managed) || renaming !== undefined}
                                      aria-label={t("edit.edit", route.host)}
                                      title={t("edit.edit", route.host)}
                                      onClick={() =>
                                        route.host
                                          ? setRenaming({ from: route.host, to: route.host })
                                          : undefined
                                      }
                                    >
                                      <PencilSimpleIcon aria-hidden="true" />
                                    </Button>
                                  </Managed>
                                  <Managed tool={managed} gate={domainGate}>
                                    <Button
                                      variant="muted"
                                      size="icon-xs"
                                      disabled={Boolean(managed)}
                                      aria-label={t("edit.remove", route.host)}
                                      title={t("edit.remove", route.host)}
                                      onClick={() =>
                                        route.host
                                          ? planner.plan({
                                              title: t("title.removeDomain", route.host),
                                              service: workload,
                                              changes: [
                                                {
                                                  op: "removeDomain",
                                                  route: routeOf(route),
                                                  hostname: route.host,
                                                },
                                              ],
                                            })
                                          : undefined
                                      }
                                    >
                                      <TrashIcon aria-hidden="true" />
                                    </Button>
                                  </Managed>
                                </span>
                              ) : null}
                            </td>
                          </tr>
                          {check && firstOfHost && route.host && needsSetup(check) ? (
                            <tr className="max-sm:block">
                              <td colSpan={4} className="px-4 pt-1 pb-3 max-sm:block">
                                <DomainSetup host={route.host} check={check} suffix={suffix} />
                              </td>
                            </tr>
                          ) : null}
                        </Fragment>
                      )
                    })}
                    {staged.added.map((change) => (
                      <tr className={row} key={`added/${change.hostname}`}>
                        <td className={`${cell} max-w-72 pl-4 max-sm:w-full max-sm:max-w-none`}>
                          <PendingHost host={change.hostname} state="added" />
                        </td>
                        <td className={cell} />
                        <td className={cell} />
                        <td className={`${cell} pr-4 max-sm:ml-auto`}>
                          <span className="flex justify-end">{undo(change)}</span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </>
            )}
            <KubeDetails
              facts={routes.map((route) => [
                route.host ?? t("net.anyHost"),
                `${routeOf(route).kind}/${route.ingress} → Service/${route.backend}:${route.port ?? "?"}`,
              ])}
            />
          </section>

          <section className="flex min-w-0 flex-col gap-3" aria-labelledby="net-private">
            <SectionHeader
              id="net-private"
              title={t("net.privateTitle")}
              description={t("net.privateAbout")}
            />
            <RefusedKinds unreadable={refusedEndpoints} />
            {endpoints.length === 0 ? (
              refusedEndpoints.length > 0 ? null : (
                <Empty variant="outline" size="inline">
                  <EmptyMedia variant="glyph">
                    <PlugsIcon aria-hidden="true" />
                  </EmptyMedia>
                  <EmptyHeader>
                    <EmptyTitle>{t("net.privateEmpty", service.name)}</EmptyTitle>
                    <EmptyDescription>{t("net.privateEmptyBody")}</EmptyDescription>
                    <ExposeHint project={project} command={expose} />
                  </EmptyHeader>
                </Empty>
              )
            ) : (
              <ul className="flex flex-col divide-y divide-v2-border overflow-hidden rounded-xl border border-v2-border bg-card">
                {endpoints.map((e) => (
                  <PrivateAddress key={e.name} endpoint={e} namespace={service.namespace} />
                ))}
              </ul>
            )}
            <KubeDetails
              facts={endpoints.map((e) => [
                e.name,
                `Service/${e.name} ${e.type ?? "ClusterIP"}${
                  e.clusterIP && e.clusterIP !== "None" ? ` ${e.clusterIP}` : ""
                }${e.ports
                  .map((p) => ` ${p.name ? `${p.name}=` : ""}${p.port}→${p.targetPort ?? p.port}`)
                  .join("")}`,
              ])}
            />
          </section>
        </>
      )}
      {reachability ? (
        <ReachabilityFor
          project={project}
          service={service}
          services={services}
          reachability={reachability}
          plan={planner.plan}
        />
      ) : null}
      <RefusedKinds unreadable={refusedAmong(unreadable, refusedPlaces.policies)} />
      {planner.dialog}
      {access ? (
        <Suspense fallback={null}>
          <DomainAccess
            project={project}
            service={service}
            route={access.route}
            host={access.host}
            onClose={() => setAccess(undefined)}
          />
        </Suspense>
      ) : null}
    </div>
  )
}

function PrivateAddress({ endpoint: e, namespace }: { endpoint: Endpoint; namespace: string }) {
  const t = useT()
  const fqdn = `${e.name}.${namespace}.svc.cluster.local`
  const ports = e.ports.length > 0 ? e.ports : [undefined]
  const only = e.ports.length === 1 ? e.ports[0]?.port : undefined
  const full = only === undefined ? fqdn : `${fqdn}:${only}`
  return (
    <li className="flex flex-col gap-1.5 px-4 py-3">
      <ul className="flex flex-col gap-1">
        {ports.map((p) => {
          const address = p ? `${e.name}:${p.port}` : e.name
          return (
            <li
              key={p ? `${p.port}/${p.protocol}` : "none"}
              className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5"
            >
              <span className="flex min-w-0 items-center gap-1">
                <span className="truncate font-mono text-sm font-medium" translate="no">
                  {address}
                </span>
                <CopyButton value={address} />
              </span>
              {p ? (
                <span
                  className="flex items-center gap-1.5 type-meta text-muted-foreground"
                  translate="no"
                >
                  {p.targetPort && /^\d+$/.test(p.targetPort) && p.targetPort !== String(p.port) ? (
                    <Tooltip content={t("net.forwardsTo", p.targetPort)}>
                      <span className="font-mono">→ {p.targetPort}</span>
                    </Tooltip>
                  ) : null}
                  <span>{p.protocol}</span>
                </span>
              ) : null}
            </li>
          )
        })}
      </ul>
      <p className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1 type-meta text-muted-foreground">
        {t("net.fullForm")}
        <span className="flex min-w-0 items-center gap-1">
          <code className="truncate font-mono text-foreground" translate="no" title={full}>
            {full}
          </code>
          <CopyButton value={full} />
        </span>
      </p>
    </li>
  )
}

/** A route's Service port as a number, where it names the port. */
function portNumber(endpoints: Endpoint[], route: PublicRoute): string {
  const port = route.port ?? ""
  const named = endpoints.find((e) => e.name === route.backend)?.ports.find((p) => p.name === port)
  return named ? String(named.port) : port
}

function ExposeHint({ project, command }: { project: string; command?: string }) {
  const t = useT()
  if (!command) return null
  return (
    <div className="mt-2 flex w-full flex-col gap-1.5">
      <span className="type-meta text-muted-foreground">{t("net.exposeHint")}</span>
      <div className="flex max-w-full min-w-0 flex-wrap items-center gap-1 self-start">
        <span className="flex max-w-full min-w-0 items-center gap-1 rounded-md bg-muted px-2 py-1">
          <code className="min-w-0 font-mono text-xs break-all" translate="no">
            {command}
          </code>
          <CopyButton value={command} />
        </span>
        <OpenInTerminal project={project} command={command} className="ml-auto" />
      </div>
    </div>
  )
}

/** A domain control, closed when another tool owns the routes or the person's role cannot change
 * domains. */
function Managed({ tool, gate, children }: { tool?: string; gate: Gate; children: ReactElement }) {
  const t = useT()
  if (!tool) return <GateTip gate={gate}>{children}</GateTip>
  return (
    <Tooltip content={t("ops.managedEdit", tool)}>
      <span>{children}</span>
    </Tooltip>
  )
}

function HostForm({
  value,
  hint,
  onCancel,
  onReview,
}: {
  value: string
  /** What the typed host will need, once it is a host name. */
  hint?: (host: string) => ReactNode
  onCancel: () => void
  onReview: (host: string) => void
}) {
  const t = useT()
  const form = useForm({
    defaultValues: { host: value },
    onSubmit: ({ value: { host } }) => {
      if (isHost(host)) onReview(host.trim().toLowerCase())
    },
  })
  return (
    <form
      className="flex flex-wrap items-center gap-2"
      onSubmit={(event) => {
        event.preventDefault()
        void form.handleSubmit()
      }}
    >
      <form.Field name="host">
        {(f) => (
          <FormField label={t("net.domain")} hideLabel className="sm:w-72">
            <Input
              size="sm"
              mono
              placeholder="app.example.com"
              value={f.state.value}
              // oxlint-disable-next-line jsx-a11y/no-autofocus -- the field opens on request, to be typed in
              autoFocus
              onChange={(event) => f.handleChange(event.target.value)}
              onBlur={f.handleBlur}
            />
          </FormField>
        )}
      </form.Field>
      <Button type="button" variant="ghost" size="xs" onClick={onCancel}>
        {t("edit.cancel")}
      </Button>
      <form.Subscribe selector={(s) => isHost(s.values.host)}>
        {(valid) => (
          <Button type="submit" size="xs" disabled={!valid}>
            <ReviewWord />
          </Button>
        )}
      </form.Subscribe>
      {hint ? (
        <form.Subscribe selector={(s) => s.values.host.trim().toLowerCase()}>
          {(host) =>
            isHost(host) && !host.includes("*") ? <div className="w-full">{hint(host)}</div> : null
          }
        </form.Subscribe>
      ) : null}
    </form>
  )
}

function Table({
  label,
  minWidth,
  children,
}: {
  label: string
  minWidth: string
  children: ReactNode
}) {
  return (
    <div className="relative overflow-x-auto rounded-xl border border-v2-border bg-card">
      <table className={`w-full text-sm max-sm:block ${minWidth}`} aria-label={label}>
        {children}
      </table>
    </div>
  )
}

/** A domain the draft adds, renames or removes, before it is applied. */
function PendingHost({
  host,
  was,
  state,
}: {
  host: string
  was?: string
  state: "added" | "changed" | "removed"
}) {
  const t = useT()
  return (
    <span className="flex min-w-0 items-center gap-1.5 text-xs">
      <GlobeIcon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
      <span
        className={cn(
          "truncate font-mono font-medium",
          state === "removed" && "text-tertiary line-through",
        )}
        title={was ? `${was} → ${host}` : host}
        translate="no"
      >
        {host}
      </span>
      <span
        className={cn(
          "shrink-0 type-meta",
          state === "added"
            ? "text-ok-strong"
            : state === "changed"
              ? "text-warn-strong"
              : "text-fail-strong",
        )}
      >
        {state === "added"
          ? t("vars.state.added")
          : state === "changed"
            ? t("vars.state.changed")
            : t("vars.state.removed")}
      </span>
    </span>
  )
}

function Host({
  route,
  certificate,
}: {
  route: PublicRoute
  certificate?: Schemas["DomainCheck"]["certificate"]
}) {
  const t = useT()
  const issuing = route.https && certificate?.state === "issuing"
  const failed = route.https && certificate?.state === "failed"
  const Icon = issuing ? HourglassMediumIcon : route.https ? LockSimpleBoldIcon : GlobeIcon
  const tone = issuing
    ? "text-progress"
    : failed
      ? "text-fail"
      : route.https
        ? "text-ok"
        : "text-muted-foreground"
  const label = issuing
    ? t("domain.pending")
    : failed
      ? t("domain.cert.failed")
      : route.https
        ? t("domains.https")
        : undefined
  return (
    <span className="flex min-w-0 items-center gap-1.5 text-xs">
      <Icon
        className={`size-3.5 shrink-0 ${tone}`}
        aria-label={label}
        aria-hidden={label ? undefined : true}
      />
      {route.host ? (
        <>
          <a
            href={hostUrl({ host: route.host, https: route.https })}
            target="_blank"
            rel="noreferrer noopener"
            title={route.host}
            className="inline-flex min-w-0 items-center gap-1 font-mono font-medium hover:underline"
            translate="no"
          >
            <span className="truncate">{route.host}</span>
            <ArrowSquareOutIcon
              className="size-3 shrink-0 text-muted-foreground"
              aria-hidden="true"
            />
          </a>
          <span className="max-sm:hidden">
            <CopyButton value={route.host} />
          </span>
        </>
      ) : (
        <span className="text-muted-foreground">{t("net.anyHost")}</span>
      )}
    </span>
  )
}

function Path({ route }: { route: PublicRoute }) {
  const t = useT()
  if (route.path === undefined && route.pathType === undefined)
    return <span className="text-xs text-muted-foreground">{t("net.defaultBackend")}</span>
  return (
    <span className="flex items-baseline gap-1.5 text-xs">
      <span className="font-mono" translate="no">
        {route.path ?? "/"}
      </span>
      {route.pathType && route.pathType !== "Prefix" ? (
        <span className="type-micro text-muted-foreground" translate="no">
          {route.pathType}
        </span>
      ) : null}
    </span>
  )
}

function ReachabilityFor({
  project,
  service,
  services,
  reachability,
  plan,
}: {
  project: string
  service: Service
  services: Service[]
  reachability: Schemas["NetworkPolicyReachability"]
  plan: ReturnType<typeof usePlanner>["plan"]
}) {
  const mine = reachability.services.find((s) => s.service === service.id)
  if (!mine) return null
  return (
    <Reachability
      project={project}
      service={service}
      services={services}
      reach={mine}
      policies={reachability.policies}
      plan={plan}
    />
  )
}
