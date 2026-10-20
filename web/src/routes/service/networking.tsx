// A service's networking as two tables. Public: one row per Ingress backend that reaches the
// service, its columns in request order (host and path → Ingress → Service:port → pods), so the
// row is the request path. Private: one row per Kubernetes Service, with its cluster DNS names.
// Domains are added, changed and removed here; Zeabur picks the route object (the one that
// already reaches the service, else an HTTPRoute when a Gateway exists, else an Ingress), and
// every change opens a plan to read before applying.
import { useForm } from "@tanstack/react-form"
import { useState, type ReactNode } from "react"
import ArrowRightIcon from "~icons/ph/arrow-right"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import GlobeIcon from "~icons/ph/globe"
import LockSimpleBoldIcon from "~icons/ph/lock-simple-bold"
import PencilSimpleIcon from "~icons/ph/pencil-simple"
import PlusIcon from "~icons/ph/plus"
import TrashIcon from "~icons/ph/trash"

import type { Schemas } from "@/api/client"
import { CopyButton } from "@/components/bits"
import { usePlanner } from "@/components/plan/planner"
import { refusedAmong, RefusedKinds } from "@/components/refused"
import { StatusPill } from "@/components/status"
import { FormField } from "@/components/form-field"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Tooltip } from "@zeabur/design-system/components/tooltip"
import { useT } from "@/i18n"
import {
  endpointsOf,
  hostUrl,
  publicRoutesOf,
  routesOf,
  type Network,
  type PublicRoute,
  type Service,
} from "@/model/service"
import { refusedPlaces } from "@/routes/project/data"
import { Reachability } from "@/routes/service/reachability"
import { Section } from "@/routes/service/section"

// RFC 1123 host names, with an optional leading wildcard label as Ingress and HTTPRoute allow: an
// instant hint while typing; the plan checks the name again.
const hostname = /^(\*\.)?([a-z0-9]([-a-z0-9]*[a-z0-9])?\.)*[a-z0-9]([-a-z0-9]*[a-z0-9])?$/
const isHost = (text: string) => {
  const host = text.trim().toLowerCase()
  return host.length > 0 && host.length <= 253 && hostname.test(host)
}

const head = "px-3 py-2 text-left type-meta font-medium text-muted-foreground whitespace-nowrap"
const cell = "px-3 py-2 align-middle max-sm:p-0"
// Below the sm breakpoint a table becomes a list: the header goes, and each row's cells flow
// and wrap like a sentence ("host / → Ingress/shop → api:80 → 3/3 ready").
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
  /** Where its endpoints and routes are looked up. */
  network: Network
  /** The project's NetworkPolicy reachability; absent when it was not read. */
  reachability?: Schemas["NetworkPolicyReachability"]
  /** Kinds the person may not list; routes, Services and NetworkPolicies show in place. */
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

  return (
    <div className="flex flex-col gap-6">
      <Section
        title={t("net.public")}
        action={
          <Managed tool={managed}>
            <Button
              variant="outline"
              size="xs"
              disabled={!canWrite || adding !== undefined}
              onClick={() => setAdding("")}
            >
              <PlusIcon aria-hidden="true" data-icon="inline-start" />
              {t("net.addDomain")}
            </Button>
          </Managed>
        }
      >
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
            <Empty>{t("domains.none")}</Empty>
          ) : null
        ) : (
          <Table label={t("net.public")} minWidth="sm:min-w-160">
            <thead className={thead}>
              <tr>
                <th className={`${head} pl-4`}>{t("table.domain")}</th>
                <th className={head}>{t("net.path")}</th>
                <th className={head}>{t("net.via")}</th>
                <th className={head}>{t("net.target")}</th>
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
                    <Step>
                      <span className="text-muted-foreground">Ingress/</span>
                      {route.ingress}
                    </Step>
                  </td>
                  <td className={cell}>
                    <Step>
                      {route.backend}:{route.port ?? "?"}
                    </Step>
                  </td>
                  <td className={cell}>
                    <span className="flex items-center gap-2">
                      <ArrowRightIcon
                        className="size-3 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                      />
                      <StatusPill health={podsOk ? "ok" : "fail"} size="sm">
                        {t("pod.readyCount", ready, service.pods.length)}
                      </StatusPill>
                    </span>
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
      </Section>

      <Section title={t("net.private")}>
        <RefusedKinds unreadable={refusedEndpoints} />
        {endpoints.length === 0 ? (
          refusedEndpoints.length > 0 ? null : (
            <Empty>{t("net.noService")}</Empty>
          )
        ) : (
          <Table label={t("net.private")} minWidth="sm:min-w-140">
            <thead className={thead}>
              <tr>
                <th className={`${head} pl-4`}>{t("net.dns")}</th>
                <th className={head}>{t("net.ports")}</th>
                <th className={`${head} pr-4`}>{t("net.type")}</th>
              </tr>
            </thead>
            <tbody className="divide-y max-sm:block">
              {endpoints.map((e) => {
                const dns = `${e.name}.${service.namespace}.svc.cluster.local`
                return (
                  <tr key={e.name} className={row}>
                    <td className={`${cell} pl-4 max-sm:w-full`}>
                      <span className="flex min-w-0 items-center gap-1.5">
                        <Tooltip content={`Service ${e.name}`}>
                          <span className="truncate font-mono text-xs font-medium" translate="no">
                            {dns}
                          </span>
                        </Tooltip>
                        <CopyButton value={dns} />
                      </span>
                      <span className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                        {t("net.alsoAs")}
                        <code className="rounded bg-muted px-1 font-mono text-foreground">
                          {e.name}
                        </code>
                        <code className="rounded bg-muted px-1 font-mono text-foreground">
                          {e.name}.{service.namespace}
                        </code>
                      </span>
                    </td>
                    <td className={cell}>
                      <ul className="flex flex-wrap gap-1.5">
                        {e.ports.map((p) => (
                          <li
                            key={`${p.port}/${p.name ?? ""}`}
                            className="inline-flex items-center gap-1.5 font-mono text-xs"
                          >
                            {p.name ? (
                              <span className="text-muted-foreground">{p.name}</span>
                            ) : null}
                            <span className="font-medium">:{p.port}</span>
                            {p.targetPort && p.targetPort !== String(p.port) ? (
                              <span className="text-muted-foreground">→ {p.targetPort}</span>
                            ) : null}
                            <span className="type-nano text-muted-foreground">{p.protocol}</span>
                          </li>
                        ))}
                      </ul>
                    </td>
                    <td className={`${cell} pr-4 font-mono text-xs`}>
                      {e.type ?? "ClusterIP"}
                      {e.clusterIP && e.clusterIP !== "None" ? (
                        <span className="block text-muted-foreground">{e.clusterIP}</span>
                      ) : null}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </Table>
        )}
      </Section>
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

function Managed({ tool, children }: { tool?: string; children: ReactNode }) {
  const t = useT()
  if (!tool) return children
  return (
    <Tooltip content={t("ops.managedEdit", tool)}>
      <span>{children}</span>
    </Tooltip>
  )
}

/** A host name to add, or a new name for one. */
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

function Empty({ children }: { children: ReactNode }) {
  return (
    <p className="rounded-xl border border-dashed px-4 py-3 text-sm text-muted-foreground">
      {children}
    </p>
  )
}

/** A hop after the first: an arrow, then the name. */
function Step({ children }: { children: ReactNode }) {
  return (
    <span className="flex items-center gap-2 font-mono text-xs whitespace-nowrap" translate="no">
      <ArrowRightIcon className="size-3 shrink-0 text-muted-foreground" aria-hidden="true" />
      <span>{children}</span>
    </span>
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
