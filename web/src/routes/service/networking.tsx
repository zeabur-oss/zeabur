import { useForm } from "@tanstack/react-form"
import { useQuery } from "@tanstack/react-query"
import { Fragment, lazy, Suspense, useState, type ReactElement, type ReactNode } from "react"
import ArrowCounterClockwiseIcon from "~icons/ph/arrow-counter-clockwise"
import ArrowBendUpRightIcon from "~icons/ph/arrow-bend-up-right"
import ArrowRightIcon from "~icons/ph/arrow-right"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretDownIcon from "~icons/ph/caret-down"
import ClockIcon from "~icons/ph/clock"
import CertificateIcon from "~icons/ph/certificate"
import GlobeIcon from "~icons/ph/globe"
import HardHatIcon from "~icons/ph/hard-hat"
import HourglassMediumIcon from "~icons/ph/hourglass-medium-bold"
import LockKeyIcon from "~icons/ph/lock-key"
import LockSimpleBoldIcon from "~icons/ph/lock-simple-bold"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlugsIcon from "~icons/ph/plugs"
import PlusIcon from "~icons/ph/plus"
import SlidersHorizontalIcon from "~icons/ph/sliders-horizontal"
import TrashIcon from "~icons/ph/trash"

import { clusterIssuersQuery, clusterQuery, projectGatewaysQuery, traceQuery } from "@/api/queries"
import { DnsHint, DomainSetup, DomainState } from "@/components/domain-setup"
import { GateTip, useGate } from "@/components/gate"
import { ReviewWord, useDraft } from "@/components/plan/draft"
import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { usePlanner, type PlanAsk } from "@/components/plan/planner"
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
import { Select } from "@zeabur/design-system/components/select"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import { createdKey, entrance } from "@/lib/created"
import { cn } from "@/lib/utils"
import { domainsLead, filterRoutes, needsSetup, plainFirst } from "@/model/domains"
import { stagedDomains, stagedSettings } from "@/model/draft"
import type { Gate } from "@/model/rights"
import { canIssue } from "@/model/cluster-issuers"
import { offered, parentOf } from "@/model/gateways"
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
import { PublicPorts } from "@/routes/service/public-ports"
import { Reachability } from "@/routes/service/reachability"

const DomainAccess = lazy(() => import("@/routes/service/domain-access"))
const DomainPages = lazy(() => import("@/routes/service/domain-pages"))
const DomainCertificate = lazy(() => import("@/routes/service/domain-certificate"))

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
  const managed = service.provenance.tool ?? undefined
  const workload = { kind: service.kind, name: service.name }
  const draft = useDraft()
  const staged = stagedDomains(
    draft?.changes ?? [],
    workload,
    routes.map((r) => r.ingress),
  )
  /** What the draft changes of a route's host: a domain edit, or its drawers' settings. */
  const pendingOf = (route: PublicRoute): Schemas["Change"][] => {
    if (!route.host) return []
    const edit = staged.edits.get(`${route.ingress}/${route.host}`)
    return edit ? [edit] : stagedSettings(draft?.changes ?? [], route.ingress, route.host)
  }
  const listed = routes.length + staged.added.length
  const [query, setQuery] = useState("")
  const [opened, setOpened] = useState<string>()
  const long = isLongList(routes.length, 10)
  const shown = plainFirst(long ? filterRoutes(routes, query) : routes)
  const lead = domainsLead(routes)
  const undo = (changes: Schemas["Change"][]) => (
    <Button variant="ghost" size="xs" onClick={() => draft?.unstage(changes)}>
      <ArrowCounterClockwiseIcon aria-hidden="true" data-icon="inline-start" />
      {t("vars.undo")}
    </Button>
  )
  const routeOf = (route: PublicRoute): Schemas["RouteRef"] => ({
    kind: (routesOf(network, service).find((r) => r.name === route.ingress)?.origin.kind ??
      "Ingress") as Schemas["RouteKind"],
    name: route.ingress,
  })
  const canWrite = endpoints.length > 0 && !managed
  const domainGate = useGate(project, "changeDomains")
  const refusedRoutes = refusedAmong(unreadable, refusedPlaces.routes)
  const refusedEndpoints = refusedAmong(unreadable, refusedPlaces.endpoints)
  const trace = useQuery({
    ...traceQuery(project, service.kind, service.name),
    enabled: routes.length > 0 || adding !== undefined,
  })
  const suffixes = useQuery(clusterQuery).data?.domains?.suffixes
  const checkOf = (host?: string) =>
    host
      ? (trace.data?.routes.find((r) => r.kind === "domain" && r.address === host)?.domain ??
        undefined)
      : undefined
  const hint = (host: string) => (
    <DnsHint host={host} addresses={trace.data?.ingressAddresses ?? []} suffixes={suffixes} />
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
            {scheduled ? null : <ExposeHint command={expose} />}
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
                choices={project}
                onCancel={() => setAdding(undefined)}
                onReview={(host, choice) => {
                  planner.plan({
                    title: t("title.addDomain", host),
                    changes: [{ op: "addDomain", workload, hostname: host, ...choice }],
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
                      const pending = pendingOf(route)
                      const [edit] = pending
                      const key = `${route.ingress}/${route.host ?? ""}/${route.path ?? ""}/${i}`
                      const folded = opened !== key && !edit && renaming?.from !== route.host
                      const more = folded ? "max-sm:hidden" : undefined
                      const check = edit ? undefined : checkOf(route.host)
                      const firstOfHost = shown.findIndex((r) => r.host === route.host) === i
                      const owner = routeFor(network, service, route)
                      const ingress = owner?.origin.kind === "Ingress" ? owner : undefined
                      const access = withAccess(owner)
                      const removeLabel =
                        route.host && lastHost(network, service, route, route.host)
                          ? t("edit.removeWithRoute", route.host, route.ingress)
                          : t("edit.remove", route.host ?? "")
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
                              ) : edit ? (
                                <PendingEdit edit={edit} host={route.host ?? ""} />
                              ) : (
                                <span className="flex min-w-0 flex-col gap-1 max-sm:w-full">
                                  <span className="flex min-w-0 items-center gap-2">
                                    <Host route={route} certificate={check?.certificate} />
                                    <PagesMarks route={owner} host={route.host} />
                                    <AccessMarks route={access} host={route.host} />
                                    <span className="flex-1 sm:hidden" />
                                    {route.host && route.https === false ? (
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
                                <span className="flex justify-end">{undo(pending)}</span>
                              ) : route.host ? (
                                <span className="flex items-center justify-end gap-0.5">
                                  <CertificateButton
                                    route={ingress}
                                    host={route.host}
                                    managed={managed}
                                    gate={domainGate}
                                    project={project}
                                    service={service}
                                    plan={planner.plan}
                                  />
                                  <PagesButton
                                    route={owner}
                                    host={route.host}
                                    managed={managed}
                                    gate={domainGate}
                                    project={project}
                                    service={service}
                                    plan={planner.plan}
                                  />
                                  <AccessButton
                                    route={access}
                                    host={route.host}
                                    managed={managed}
                                    gate={domainGate}
                                    project={project}
                                    service={service}
                                    plan={planner.plan}
                                  />
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
                                      aria-label={removeLabel}
                                      title={removeLabel}
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
                                <DomainSetup
                                  host={route.host}
                                  check={check}
                                  suffixes={suffixes}
                                  checking={trace.isFetching}
                                  onCheck={() => void trace.refetch()}
                                />
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
                          <span className="flex justify-end">{undo([change])}</span>
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

          <PublicPorts
            project={project}
            service={service}
            endpoints={endpoints}
            plan={planner.plan}
          />

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
                    <ExposeHint command={expose} />
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

function ExposeHint({ command }: { command?: string }) {
  const t = useT()
  if (!command) return null
  return (
    <div className="mt-2 flex w-full flex-col gap-1.5">
      <span className="type-meta text-muted-foreground">{t("net.exposeHint")}</span>
      <span className="flex max-w-full min-w-0 items-center gap-1 self-start rounded-md bg-muted px-2 py-1">
        <code className="min-w-0 font-mono text-xs break-all" translate="no">
          {command}
        </code>
        <CopyButton value={command} />
      </span>
    </div>
  )
}

// On an Ingress, redirects and passwords are Traefik middlewares, which only an Ingress that
// Traefik serves can list. An HTTPRoute redirects with a filter.
function withAccess(route?: Schemas["Route"]): Schemas["Route"] | undefined {
  return route && (isGateway(route) || servedByTraefik(route)) ? route : undefined
}

const isGateway = (route: Schemas["Route"]) => route.origin.kind === "HTTPRoute"

const servedByTraefik = (route: Schemas["Route"]) => !route.class || route.class.includes("traefik")

function routeFor(
  network: Network,
  service: Service,
  publicRoute: PublicRoute,
): Schemas["Route"] | undefined {
  return routesOf(network, service).find((r) => r.name === publicRoute.ingress)
}

/** Whether `host` is all the route routes, so removing it deletes the route. */
function lastHost(network: Network, service: Service, publicRoute: PublicRoute, host: string) {
  const route = routesOf(network, service).find((r) => r.name === publicRoute.ingress)
  return route?.rules.every((rule) => rule.host === host) ?? false
}

/** Maintenance first: the domain does not reach the service. */
function PagesMarks({ route, host }: { route?: Schemas["Route"]; host?: string | null }) {
  const t = useT()
  if (!route) return null
  return (
    <>
      {host && route.maintenance?.includes(host) ? (
        <span
          className="flex shrink-0 items-center gap-1 type-meta text-warn-strong"
          data-testid="in-maintenance"
        >
          <HardHatIcon aria-hidden="true" />
          {t("domainPages.inMaintenance")}
        </span>
      ) : null}
      {route.errorPages ? (
        <span className="shrink-0 type-meta text-muted-foreground max-sm:hidden">
          {t("domainPages.withErrorPages")}
        </span>
      ) : null}
    </>
  )
}

function AccessMarks({ route, host }: { route?: Schemas["Route"]; host?: string | null }) {
  const t = useT()
  if (!route) return null
  return (
    <>
      {route.redirects?.some((r) => r.host === host) ? (
        <span className="flex shrink-0 items-center gap-1 type-meta text-muted-foreground">
          <ArrowBendUpRightIcon aria-hidden="true" />
          {t("domainAccess.redirects")}
        </span>
      ) : null}
      {route.basicAuth ? (
        <span className="flex shrink-0 items-center gap-1 type-meta text-muted-foreground">
          <LockKeyIcon aria-hidden="true" />
          {t("domainAccess.protected")}
        </span>
      ) : null}
    </>
  )
}

/** Opens the redirect and password drawer of one domain. */
function AccessButton({ route, host, ...rest }: RowDrawerProps) {
  const t = useT()
  if (!route || !host) return null
  return (
    <RowDrawer
      {...rest}
      label={t("domainAccess.open", host)}
      icon={<SlidersHorizontalIcon aria-hidden="true" />}
    >
      {(onClose) => (
        <DomainAccess
          project={rest.project}
          service={rest.service}
          route={route}
          host={host}
          plan={rest.plan}
          onClose={onClose}
        />
      )}
    </RowDrawer>
  )
}

/** Opens maintenance mode and the error pages of one domain. */
function PagesButton({ route, host, ...rest }: RowDrawerProps) {
  const t = useT()
  if (!route || !host) return null
  return (
    <RowDrawer
      {...rest}
      label={t("domainPages.open", host)}
      icon={<HardHatIcon aria-hidden="true" />}
      testid="domain-pages"
    >
      {(onClose) => (
        <DomainPages
          project={rest.project}
          service={rest.service}
          route={route}
          host={host}
          traefik={!isGateway(route) && servedByTraefik(route)}
          plan={rest.plan}
          onClose={onClose}
        />
      )}
    </RowDrawer>
  )
}

/** Opens the certificate drawer of one domain of an Ingress. An HTTPRoute's certificate is set on
 * its Gateway, which is the cluster's. */
function CertificateButton({ route, host, ...rest }: RowDrawerProps) {
  const t = useT()
  if (!host) return null
  return (
    <RowDrawer
      {...rest}
      label={t("domainCert.open", host)}
      off={route ? undefined : t("domainCert.gateway")}
      icon={<CertificateIcon aria-hidden="true" />}
    >
      {(onClose) =>
        route ? (
          <DomainCertificate
            project={rest.project}
            service={rest.service}
            route={route}
            host={host}
            plan={rest.plan}
            onClose={onClose}
          />
        ) : null
      }
    </RowDrawer>
  )
}

interface RowDrawerProps {
  route?: Schemas["Route"]
  host?: string | null
  managed?: string
  gate: Gate
  project: string
  service: Service
  plan: (ask: PlanAsk) => boolean
}

/** An icon button on a domain row that opens a lazy drawer. With `off`, the button is off and
 * says why. */
function RowDrawer({
  managed,
  gate,
  label,
  off,
  icon,
  testid,
  children,
}: {
  managed?: string
  gate: Gate
  label: string
  off?: string
  icon: ReactElement
  testid?: string
  children: (onClose: () => void) => ReactNode
}) {
  const [open, setOpen] = useState(false)
  const title = off ?? label
  return (
    <>
      <Managed tool={managed} gate={gate}>
        <Button
          variant="muted"
          size="icon-xs"
          disabled={Boolean(managed) || off !== undefined}
          aria-label={title}
          title={title}
          data-testid={testid}
          onClick={() => setOpen(true)}
        >
          {icon}
        </Button>
      </Managed>
      {open ? <Suspense fallback={null}>{children(() => setOpen(false))}</Suspense> : null}
    </>
  )
}

function Managed({ tool, gate, children }: { tool?: string; gate: Gate; children: ReactElement }) {
  const t = useT()
  if (!tool) return <GateTip gate={gate}>{children}</GateTip>
  return (
    <Tooltip content={t("ops.managedEdit", tool)}>
      <span>{children}</span>
    </Tooltip>
  )
}

type Choice = Pick<Schemas["AddDomainChange"], "certificate" | "parent">

function HostForm({
  value,
  hint,
  choices,
  onCancel,
  onReview,
}: {
  value: string
  /** What the typed host will need, once it is a host name. */
  hint?: (host: string) => ReactNode
  /** The project whose Gateways and the cluster's issuers to offer, for a new domain. */
  choices?: string
  onCancel: () => void
  onReview: (host: string, choice: Choice) => void
}) {
  const t = useT()
  const [issuer, setIssuer] = useState("")
  const [gateway, setGateway] = useState("")
  const gateways =
    useQuery({ ...projectGatewaysQuery(choices ?? ""), enabled: choices !== undefined }).data
      ?.gateways ?? []
  const form = useForm({
    defaultValues: { host: value },
    onSubmit: ({ value: { host } }) => {
      if (!isHost(host)) return
      const name = host.trim().toLowerCase()
      const parent = offered(gateways, name).includes(gateway) ? parentOf(gateway) : undefined
      onReview(name, parent ? { parent } : issuerChoice(issuer))
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
      {choices ? (
        <form.Subscribe selector={(s) => s.values.host.trim().toLowerCase()}>
          {(host) => {
            const listeners = offered(gateways, host)
            const chosen = listeners.includes(gateway) ? gateway : ""
            return (
              <>
                <GatewayChoice listeners={listeners} value={chosen} onChange={setGateway} />
                {chosen ? null : <IssuerChoice host={host} value={issuer} onChange={setIssuer} />}
              </>
            )
          }}
        </form.Subscribe>
      ) : null}
    </form>
  )
}

const issuerChoice = (issuer: string): Pick<Choice, "certificate"> =>
  issuer ? { certificate: { from: "issuer", name: issuer } } : {}

/** Ingress, or one of `listeners`. Shown only when there is one. */
function GatewayChoice({
  listeners,
  value,
  onChange,
}: {
  listeners: string[]
  value: string
  onChange: (gateway: string) => void
}) {
  const t = useT()
  if (listeners.length === 0) return null
  const items = listeners.map((l) => {
    const [namespace, name, listener] = l.split("/")
    return { value: l, label: t("net.viaGateway", `${namespace}/${name}`, listener ?? "") }
  })
  return (
    <div className="flex w-full flex-wrap items-center gap-2">
      <span className="type-meta w-12 shrink-0 text-muted-foreground">{t("net.routeKind")}</span>
      <Select
        size="sm"
        className="sm:w-72"
        value={value}
        onValueChange={(next) => onChange(String(next))}
        items={[{ value: "", label: t("net.viaIngress") }, ...items]}
      />
      {value ? (
        <span className="type-meta text-muted-foreground">{t("net.gatewayHttps")}</span>
      ) : null}
    </div>
  )
}

/** The issuer of a new domain: the default one, or another the person may read. Shown only when
 * there is a choice; a wildcard host needs a DNS-01 issuer. */
function IssuerChoice({
  host,
  value,
  onChange,
}: {
  host: string
  value: string
  onChange: (issuer: string) => void
}) {
  const t = useT()
  const data = useQuery(clusterIssuersQuery).data
  const issuers = data?.state === "installed" ? data.issuers : []
  const others = issuers.filter((i) => i.name !== data?.default)
  if (others.length === 0) return null
  const chosen = issuers.find((i) => i.name === (value || data?.default))
  return (
    <div className="flex w-full flex-wrap items-center gap-2">
      <span className="type-meta w-12 shrink-0 text-muted-foreground">
        {t("domainCert.issuer")}
      </span>
      <Select
        size="sm"
        className="sm:w-72"
        value={value}
        onValueChange={(next) => onChange(String(next))}
        items={[
          { value: "", label: t("domainCert.defaultIssuer", data?.default ?? "—") },
          ...others.map((i) => ({ value: i.name, label: i.name })),
        ]}
      />
      {chosen && !canIssue(chosen, host) ? (
        <span className="type-meta text-warn-strong">{t("domainCert.needsDns", chosen.name)}</span>
      ) : null}
    </div>
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
/** A host the draft changes: renamed, removed, or with changed settings. */
function PendingEdit({ edit, host }: { edit: Schemas["Change"]; host: string }) {
  if (edit.op === "renameDomain")
    return <PendingHost host={edit.to} was={edit.from} state="changed" />
  if (edit.op === "removeDomain") return <PendingHost host={edit.hostname} state="removed" />
  return <PendingHost host={host} state="changed" />
}

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
        : route.https === undefined
          ? t("domains.gatewayHttps")
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
