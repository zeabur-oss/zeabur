import { useForm } from "@tanstack/react-form"
import { useState, type ReactNode } from "react"
import ArrowRightIcon from "~icons/ph/arrow-right"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import ClockIcon from "~icons/ph/clock"
import GlobeIcon from "~icons/ph/globe"
import LockSimpleBoldIcon from "~icons/ph/lock-simple-bold"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlugsIcon from "~icons/ph/plugs"
import PlusIcon from "~icons/ph/plus"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { usePlanner } from "@/components/plan/planner"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { StatusPill } from "@/components/status"
import { FormField } from "@/components/form-field"
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
import { Term, TermDetail } from "@zeabur/design-system/components/term"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
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
  const podsOk = ready === service.pods.length && ready > 0
  const planner = usePlanner(project)
  const [adding, setAdding] = useState<string>()
  const [renaming, setRenaming] = useState<{ from: string; to: string }>()
  const managed = service.provenance.tool ?? undefined
  const workload = { kind: service.kind, name: service.name }
  const routeOf = (route: PublicRoute): Schemas["RouteRef"] => ({
    kind: (routesOf(network, service).find((r) => r.name === route.ingress)?.origin.kind ??
      "Ingress") as Schemas["RouteKind"],
    name: route.ingress,
  })
  const canWrite = endpoints.length > 0 && !managed
  const refusedRoutes = refusedAmong(unreadable, refusedPlaces.routes)
  const refusedEndpoints = refusedAmong(unreadable, refusedPlaces.endpoints)
  const routeKinds = [...new Set(routesOf(network, service).map((r) => r.origin.kind as string))]

  const scheduled = service.kind === "CronJob" || service.kind === "Job"
  const noAddress =
    routes.length === 0 &&
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
              title={
                <span className="inline-flex flex-wrap items-baseline gap-1.5">
                  {t("net.publicTitle")}
                  {(routeKinds.length > 0 ? routeKinds : ["Ingress"]).map((kind) => (
                    <TermDetail key={kind}>{kind}</TermDetail>
                  ))}
                </span>
              }
              count={routes.length > 0 ? routes.length : undefined}
              description={t("net.publicAbout")}
              action={
                routes.length > 0 ? (
                  <Managed tool={managed}>
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
            {routes.length === 0 ? (
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
                      <Managed tool={managed}>
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
              <Table label={t("net.publicTitle")} minWidth="sm:min-w-160">
                <thead className={thead}>
                  <tr>
                    <th className={`${head} pl-4`}>{t("table.domain")}</th>
                    <th className={head}>{t("net.path")}</th>
                    <th className={head}>{t("net.sendsTo")}</th>
                    <th className={head}>{t("pod.pods")}</th>
                    <th className={`${head} pr-4`}>
                      <span className="sr-only">{t("table.actions")}</span>
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y max-sm:block">
                  {routes.map((route, i) => (
                    <tr
                      className={row}
                      key={`${route.ingress}/${route.host ?? ""}/${route.path ?? ""}/${i}`}
                    >
                      <td className={`${cell} max-w-72 pl-4 max-sm:w-full max-sm:max-w-none`}>
                        {renaming && route.host === renaming.from ? (
                          <HostForm
                            value={renaming.to}
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
                        ) : (
                          <Host route={route} />
                        )}
                      </td>
                      <td className={cell}>
                        <Path route={route} />
                      </td>
                      <td className={cell}>
                        <span className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                          <ArrowRightIcon
                            className="size-3 shrink-0 text-muted-foreground"
                            aria-hidden="true"
                          />
                          <span className="font-mono whitespace-nowrap" translate="no">
                            {route.backend}:{route.port ?? "?"}
                          </span>
                          <TermDetail>
                            {routeOf(route).kind} {route.ingress}
                          </TermDetail>
                        </span>
                      </td>
                      <td className={cell}>
                        <StatusPill health={podsOk ? "ok" : "fail"} size="sm">
                          {t("pod.readyCount", ready, service.pods.length)}
                        </StatusPill>
                      </td>
                      <td className={`${cell} pr-4 max-sm:ml-auto`}>
                        {route.host ? (
                          <span className="flex items-center justify-end gap-0.5">
                            <Managed tool={managed}>
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
                            <Managed tool={managed}>
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
                  ))}
                </tbody>
              </Table>
            )}
          </section>

          <section className="flex min-w-0 flex-col gap-3" aria-labelledby="net-private">
            <SectionHeader
              id="net-private"
              title={<Term detail="Service">{t("net.privateTitle")}</Term>}
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
  const first = e.ports[0]?.port
  return (
    <li className="flex flex-col gap-2 px-4 py-3">
      <ul className="flex flex-col gap-1">
        {ports.map((p) => {
          const address = p ? `${fqdn}:${p.port}` : fqdn
          return (
            <li
              key={p ? `${p.port}/${p.protocol}` : "none"}
              className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5"
            >
              <span className="flex min-w-0 items-center gap-1">
                <span
                  className="truncate font-mono text-xs font-medium"
                  translate="no"
                  title={address}
                >
                  {address}
                </span>
                <CopyButton value={address} />
              </span>
              {p ? (
                <span
                  className="flex items-center gap-1.5 font-mono type-meta text-muted-foreground"
                  translate="no"
                >
                  {p.name ? <span>{p.name}</span> : null}
                  {p.targetPort && p.targetPort !== String(p.port) ? (
                    <Tooltip content={t("net.forwardsTo", p.targetPort)}>
                      <span>→ {p.targetPort}</span>
                    </Tooltip>
                  ) : null}
                  <span className="type-nano">{p.protocol}</span>
                </span>
              ) : null}
            </li>
          )
        })}
      </ul>
      <p className="flex flex-wrap items-center gap-x-1.5 gap-y-1 type-meta text-muted-foreground">
        {t("net.shortForm")}
        <code className="rounded bg-muted px-1 font-mono text-foreground" translate="no">
          {first === undefined ? e.name : `${e.name}:${first}`}
        </code>
        <span className="ml-auto flex flex-wrap items-center gap-1.5">
          <TermDetail>Service {e.name}</TermDetail>
          <TermDetail>
            {e.type ?? "ClusterIP"}
            {e.clusterIP && e.clusterIP !== "None" ? ` ${e.clusterIP}` : ""}
          </TermDetail>
        </span>
      </p>
    </li>
  )
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

function Managed({ tool, children }: { tool?: string; children: ReactNode }) {
  const t = useT()
  if (!tool) return children
  return (
    <Tooltip content={t("ops.managedEdit", tool)}>
      <span>{children}</span>
    </Tooltip>
  )
}

function HostForm({
  value,
  onCancel,
  onReview,
}: {
  value: string
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
            {t("edit.review")}
          </Button>
        )}
      </form.Subscribe>
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
    <div className="overflow-x-auto rounded-xl border border-v2-border bg-card">
      <table className={`w-full text-sm max-sm:block ${minWidth}`} aria-label={label}>
        {children}
      </table>
    </div>
  )
}

function Host({ route }: { route: PublicRoute }) {
  const t = useT()
  const Icon = route.https ? LockSimpleBoldIcon : GlobeIcon
  return (
    <span className="flex min-w-0 items-center gap-1.5 text-xs">
      <Icon
        className={`size-3.5 shrink-0 ${route.https ? "text-ok" : "text-muted-foreground"}`}
        aria-label={route.https ? t("domains.https") : undefined}
        aria-hidden={route.https ? undefined : true}
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
          <CopyButton value={route.host} />
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
