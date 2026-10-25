import { Link } from "@tanstack/react-router"
import { useState, type ReactNode } from "react"
import CaretDownIcon from "~icons/ph/caret-down"
import CaretRightIcon from "~icons/ph/caret-right"
import CaretUpDownIcon from "~icons/ph/caret-up-down"
import ShieldPlusIcon from "~icons/ph/shield-plus"

import type { Schemas } from "@/api/client"
import { networkPresetsQuery, queryClient } from "@/api/queries"
import { FilterField } from "@/components/filter-field"
import { StatusPill } from "@/components/status"
import { Button } from "@zeabur/design-system/components/button"
import { Drawer } from "@zeabur/design-system/components/drawer"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur/design-system/components/menu"
import { SectionHeader } from "@zeabur/design-system/components/section-header"
import { GateTip, useGate, useHereGate } from "@/components/gate"
import { useT, type Translate } from "@/i18n"
import { objectKey } from "@/lib/objects"
import { cn } from "@/lib/utils"
import type { PlanAsk } from "@/components/plan/planner"
import {
  byRules,
  filterPeers,
  leadOf,
  namesShown,
  reachSummary,
  sharedRules,
  showsRules,
  type Lead,
  type Peer,
  type PeerGroup,
} from "@/model/reach"
import type { Health, Service } from "@/model/service"

type Access = Schemas["NetworkPolicyAccess"]

const health: Record<Access, Health> = { allowed: "ok", limited: "warn", blocked: "fail" }

function AccessPill({ access, children }: { access: Access; children?: ReactNode }) {
  const t = useT()
  return (
    <StatusPill health={health[access]} size="sm">
      {t(`reach.${access}`)}
      {children}
    </StatusPill>
  )
}

/** A rule's name, opening it on the Objects page. */
function RuleLink({ project, name }: { project: string; name: string }) {
  return (
    <Link
      to="/projects/$project/objects"
      params={{ project }}
      search={{
        object: objectKey({
          group: "networking.k8s.io",
          version: "v1",
          resource: "networkpolicies",
          name,
        }),
      }}
      className="font-mono text-xs hover:underline"
      translate="no"
    >
      {name}
    </Link>
  )
}

/** The rules that decide a peer, each a link. */
function RuleLinks({ project, via }: { project: string; via: string[] }) {
  const t = useT()
  return via.map((name, i) => (
    <span key={name}>
      {i > 0 ? t("common.sep") : null}
      <RuleLink project={project} name={name} />
    </span>
  ))
}

/** The rules that decide every peer of a list, said once above it. */
function SharedRules({ project, via }: { project: string; via: string[] }) {
  const t = useT()
  if (via.length === 0) return null
  return (
    <p className="type-meta text-muted-foreground">
      {t("reach.via")}
      {t("common.colon")}
      <RuleLinks project={project} via={via} />
    </p>
  )
}

/** Below `sm` a group opens in a drawer rather than in place. */
const narrow = () => window.matchMedia("(max-width: 39.99rem)").matches

export function Reachability({
  project,
  service,
  services,
  reach,
  policies,
  plan,
}: {
  project: string
  service: Service
  services: Service[]
  reach: Schemas["NetworkPolicyService"]
  policies: string[]
  plan: (ask: PlanAsk) => void
}) {
  const t = useT()
  const gate = useGate(project, "changeNetworkPolicies")
  const [problem, setProblem] = useState<string>()
  const [open, setOpen] = useState<Access>()
  const [drawer, setDrawer] = useState<"rules" | "pick" | Access>()
  const name = (id: string) => services.find((s) => s.id === id)?.name ?? id.split("/").pop() ?? id
  const summary = reachSummary(reach, name)
  const choose = async (kind: string, from?: string) => {
    setProblem(undefined)
    try {
      const presets = await queryClient.query(
        networkPresetsQuery(project, `${service.kind}/${service.name}`),
      )
      const preset = presets.items.find((p) => p.kind === kind && (p.from ?? undefined) === from)
      if (!preset) return
      setDrawer(undefined)
      plan({
        title:
          kind === "isolate"
            ? t("reach.planIsolate", service.name)
            : kind === "allow-from-ingress-controller"
              ? t("reach.planAllowIngress", service.name)
              : t("reach.planAllowFrom", name(from ?? ""), service.name),
        changes: [{ op: "create", object: preset.object }],
        service: { kind: service.kind, name: service.name },
      })
    } catch (error) {
      setProblem(error instanceof Error ? error.message : String(error))
    }
  }
  const allow = (peer: Peer) => void choose("allow-from-service", peer.id)
  const closed = summary.groups.filter((g) => g.access !== "allowed").flatMap((g) => g.peers)
  const rules = reach.selectedBy
  const via = showsRules(policies)
  const row =
    "flex flex-wrap items-start gap-x-3 gap-y-1.5 border-t border-rule py-2.5 first:border-t-0"
  const label = "w-28 shrink-0 pt-0.5 text-muted-foreground max-sm:w-full"
  const toggle = (access: Access) => {
    if (narrow()) setDrawer(access)
    else setOpen((was) => (was === access ? undefined : access))
  }
  const shown = drawer && drawer !== "rules" && drawer !== "pick" ? drawer : undefined

  return (
    <section className="flex min-w-0 flex-col gap-3" aria-labelledby="net-rules">
      <SectionHeader
        id="net-rules"
        title={t("reach.title")}
        count={rules.length > 0 ? t("reach.rulesApply", rules.length) : undefined}
        description={
          summary.open
            ? policies.length === 0
              ? t("reach.noPolicies")
              : t("reach.noneSelect")
            : leadSentence(t, service.name, leadOf(reach))
        }
        action={
          !gate.open ? (
            <GateTip gate={gate}>
              <Button variant="outline" size="xs">
                <ShieldPlusIcon aria-hidden="true" data-icon="inline-start" />
                {t("reach.addPolicy")}
                <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
              </Button>
            </GateTip>
          ) : (
            <Menu>
              <MenuTrigger render={<Button variant="outline" size="xs" />}>
                <ShieldPlusIcon aria-hidden="true" data-icon="inline-start" />
                {t("reach.addPolicy")}
                <CaretDownIcon aria-hidden="true" data-icon="inline-end" />
              </MenuTrigger>
              <MenuContent className="w-72">
                <MenuItem onClick={() => void choose("isolate")}>{t("reach.isolate")}</MenuItem>
                <MenuItem onClick={() => void choose("allow-from-ingress-controller")}>
                  {t("reach.allowIngress")}
                </MenuItem>
                {closed.length > 0 ? (
                  <MenuItem onClick={() => setDrawer("pick")}>{t("reach.allowFromPick")}</MenuItem>
                ) : null}
              </MenuContent>
            </Menu>
          )
        }
      />
      {summary.open ? null : (
        <div className="rounded-xl border border-v2-border bg-card px-4 py-1 text-sm">
          <div className={row}>
            <span className={cn(label, "max-sm:hidden")}>{t("reach.policies")}</span>
            <span className="flex min-w-0 flex-1 flex-wrap gap-1.5 max-sm:hidden">
              {rules.map((policy) => (
                <code key={policy} className="rounded bg-muted px-1">
                  <RuleLink project={project} name={policy} />
                </code>
              ))}
            </span>
            <button
              type="button"
              className="flex w-full items-center justify-between gap-2 text-left outline-none sm:hidden"
              onClick={() => setDrawer("rules")}
            >
              {t("reach.rules", rules.length)}
              <CaretRightIcon className="size-3.5 text-muted-foreground" aria-hidden="true" />
            </button>
          </div>
          <div className={row}>
            <span className={label}>{t("reach.in")}</span>
            <div className="flex min-w-0 flex-1 flex-col gap-1">
              {summary.groups.map((group) =>
                summary.few ? (
                  <FewGroup key={group.access} group={group} project={via ? project : undefined} />
                ) : (
                  <div key={group.access} className="flex flex-col gap-2">
                    <GroupButton
                      group={group}
                      expanded={open === group.access}
                      onClick={() => toggle(group.access)}
                    />
                    {open === group.access ? (
                      <GroupTable
                        group={group}
                        project={via ? project : undefined}
                        onAllow={group.access === "allowed" ? undefined : allow}
                      />
                    ) : null}
                  </div>
                ),
              )}
              <span className="flex flex-wrap items-center gap-x-4 gap-y-1.5 py-1">
                <span className="flex items-center gap-1.5">
                  {t("reach.fromDomains")}
                  <AccessPill access={reach.fromIngressController} />
                </span>
                <span className="flex items-center gap-1.5">
                  {t("reach.otherProjects")}
                  <AccessPill access={reach.fromOtherNamespaces} />
                </span>
              </span>
            </div>
          </div>
          <div className={row}>
            <span className={label}>{t("reach.out")}</span>
            <span className="flex flex-wrap items-center gap-x-4 gap-y-1.5">
              <span className="flex items-center gap-1.5">
                {t("reach.dns")}
                <AccessPill access={reach.toDns} />
              </span>
              <span className="flex items-center gap-1.5">
                {t("reach.outside")}
                <AccessPill access={reach.toOutside} />
              </span>
            </span>
          </div>
        </div>
      )}
      <p className="type-meta text-muted-foreground">{t("reach.cni")}</p>
      {problem ? <p className="type-meta text-fail">{problem}</p> : null}
      <Drawer
        open={drawer !== undefined}
        onOpenChange={(next) => (next ? undefined : setDrawer(undefined))}
        size="md"
        closeLabel={t("common.close")}
        title={
          drawer === "rules"
            ? t("reach.policies")
            : drawer === "pick"
              ? t("reach.allowFromPick")
              : shown
                ? `${t(`reach.${shown}`)} ${summary.groups.find((g) => g.access === shown)?.peers.length ?? 0}`
                : undefined
        }
      >
        {drawer === "rules" ? (
          <ul className="flex flex-col divide-y divide-v2-border">
            {rules.map((policy) => (
              <li key={policy} className="truncate py-2.5">
                <RuleLink project={project} name={policy} />
              </li>
            ))}
          </ul>
        ) : drawer === "pick" ? (
          <PeerList peers={closed} project={via ? project : undefined} onAllow={allow} />
        ) : shown ? (
          <PeerList
            peers={summary.groups.find((g) => g.access === shown)?.peers ?? []}
            project={via ? project : undefined}
            onAllow={shown === "allowed" ? undefined : allow}
          />
        ) : null}
      </Drawer>
    </section>
  )
}

function leadSentence(t: Translate, name: string, lead: Lead): string {
  const peers = lead.allowed + lead.limited + lead.blocked
  const clauses: string[] = []
  if (peers > 0)
    clauses.push(
      lead.allowed === 0
        ? t("reach.lead.none", name)
        : lead.allowed === peers
          ? t("reach.lead.all", name)
          : t("reach.lead.only", name, lead.allowed),
    )
  const by = (access: Access, servicesToo: boolean) => [
    ...(servicesToo && lead[access] > 0 ? [t("reach.services", lead[access])] : []),
    ...(lead.otherProjects === access ? [t("reach.lead.otherProjects")] : []),
    ...(lead.domains === access ? [t("reach.lead.domains")] : []),
  ]
  // "1 service is", every other subject is plural.
  const one = (items: string[], access: Access) =>
    items.length === 1 && lead[access] === 1 && items[0] === t("reach.services", 1)
  const blocked = by("blocked", lead.allowed > 0 || lead.limited > 0)
  const limited = by("limited", true)
  const allowed = by("allowed", false)
  if (blocked.length > 0)
    clauses.push(t("reach.lead.blocked", t("common.list", blocked), one(blocked, "blocked")))
  if (limited.length > 0)
    clauses.push(t("reach.lead.limited", t("common.list", limited), one(limited, "limited")))
  if (allowed.length > 0) clauses.push(t("reach.lead.allowed", t("common.list", allowed), false))
  return t("reach.lead", clauses, peers === 0)
}

/** Few peers: the verdict once, then every name; with `project`, each set of names with the
 * rules that decide it. */
function FewGroup({ group, project }: { group: PeerGroup; project?: string }) {
  const t = useT()
  return (
    <span className="flex flex-wrap items-baseline gap-x-2 gap-y-1 py-1">
      <AccessPill access={group.access} />
      <span className="flex min-w-0 flex-col gap-1">
        {(project ? byRules(group.peers) : [{ via: [], peers: group.peers }]).map((part) => (
          <span key={part.via.join(",")} className="flex flex-wrap items-baseline gap-x-2">
            <span className="min-w-0 font-mono text-xs" translate="no">
              {part.peers.map((p) => p.name).join(", ")}
            </span>
            {project && part.via.length > 0 ? (
              <span className="type-meta text-muted-foreground">
                {t("reach.via")}
                {t("common.colon")}
                <RuleLinks project={project} via={part.via} />
              </span>
            ) : null}
          </span>
        ))}
      </span>
    </span>
  )
}

function GroupButton({
  group,
  expanded,
  onClick,
}: {
  group: PeerGroup
  expanded: boolean
  onClick: () => void
}) {
  const t = useT()
  const { names, more } = namesShown(group)
  return (
    <button
      type="button"
      aria-expanded={expanded}
      onClick={onClick}
      className="-mx-2 flex min-w-0 items-center gap-2 rounded-md px-2 py-1 text-left outline-none hover:bg-v2-surface-hover aria-expanded:bg-v2-surface-hover"
    >
      <AccessPill access={group.access}>
        <span className="tabular-nums">{group.peers.length}</span>
      </AccessPill>
      <span className="flex min-w-0 flex-1 items-baseline gap-1.5 max-sm:hidden">
        <span className="min-w-0 truncate font-mono text-xs" translate="no">
          {names.join(", ")}
        </span>
        {names.length > 0 && more > 0 ? (
          <span className="shrink-0 type-meta text-muted-foreground">{t("reach.more", more)}</span>
        ) : null}
      </span>
      <span className="flex-1 sm:hidden" />
      <CaretRightIcon
        className={cn(
          "size-3.5 shrink-0 text-muted-foreground motion-safe:transition-transform",
          expanded && "sm:rotate-90",
        )}
        aria-hidden="true"
      />
    </button>
  )
}

/** A group opened in place: its peers in a bounded, filterable, sortable table; with `project`,
 * the rules that decide them, once when they share them, else in a column. */
function GroupTable({
  group,
  project,
  onAllow,
}: {
  group: PeerGroup
  project?: string
  onAllow?: (peer: Peer) => void
}) {
  const t = useT()
  const gate = useHereGate("changeNetworkPolicies")
  const [query, setQuery] = useState("")
  const [descending, setDescending] = useState(false)
  const peers = filterPeers(group.peers, query)
  const sorted = descending ? peers.toReversed() : peers
  const shared = project ? sharedRules(group.peers) : undefined
  const column = project && !shared ? project : undefined
  return (
    <div className="mb-2 flex flex-col gap-2">
      {project && shared ? <SharedRules project={project} via={shared} /> : null}
      <FilterField value={query} onChange={setQuery} label={t("reach.filter")} />
      <div className="max-h-90 overflow-y-auto rounded-lg border border-v2-border">
        <table className="w-full text-sm" aria-label={t(`reach.${group.access}`)}>
          <thead className="sticky top-0 z-raised border-b border-v2-border bg-card">
            <tr>
              <th className="px-3 py-1.5 text-left type-meta font-medium text-muted-foreground">
                <button
                  type="button"
                  aria-pressed={descending}
                  onClick={() => setDescending((d) => !d)}
                  className="inline-flex items-center gap-1 rounded outline-none hover:text-foreground"
                >
                  {t("reach.service")}
                  <CaretUpDownIcon className="size-3" aria-hidden="true" />
                </button>
              </th>
              {column ? (
                <th className="px-3 py-1.5 text-left type-meta font-medium text-muted-foreground">
                  {t("reach.via")}
                </th>
              ) : null}
              {onAllow ? (
                <th className="px-3 py-1.5">
                  <span className="sr-only">{t("table.actions")}</span>
                </th>
              ) : null}
            </tr>
          </thead>
          <tbody className="divide-y divide-v2-border">
            {sorted.map((peer) => (
              <tr key={peer.id}>
                <td className="max-w-0 truncate px-3 py-1.5 font-mono text-xs" translate="no">
                  {peer.name}
                </td>
                {column ? (
                  <td className="px-3 py-1.5 text-muted-foreground">
                    <RuleLinks project={column} via={peer.via} />
                  </td>
                ) : null}
                {onAllow ? (
                  <td className="w-px px-3 py-1 text-right">
                    <GateTip gate={gate}>
                      <Button variant="ghost" size="xs" onClick={() => onAllow(peer)}>
                        {t("reach.allow")}
                      </Button>
                    </GateTip>
                  </td>
                ) : null}
              </tr>
            ))}
            {sorted.length === 0 ? (
              <tr>
                <td className="px-3 py-2 text-xs text-muted-foreground">{t("reach.noMatch")}</td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  )
}

/** A drawer's list: a filter, then names, each with Allow when it is not allowed yet; with
 * `project`, the rules that decide them, once when they share them. */
function PeerList({
  peers,
  project,
  onAllow,
}: {
  peers: Peer[]
  project?: string
  onAllow?: (peer: Peer) => void
}) {
  const t = useT()
  const gate = useHereGate("changeNetworkPolicies")
  const [query, setQuery] = useState("")
  const shown = filterPeers(peers, query)
  const shared = project ? sharedRules(peers) : undefined
  const each = project && !shared ? project : undefined
  return (
    <div className="flex flex-col gap-3">
      {project && shared ? <SharedRules project={project} via={shared} /> : null}
      <FilterField value={query} onChange={setQuery} label={t("reach.filter")} wide />
      <ul className="flex flex-col divide-y divide-v2-border">
        {shown.map((peer) => (
          <li key={peer.id} className="flex min-h-10 items-center gap-2 py-1">
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate font-mono text-sm" translate="no">
                {peer.name}
              </span>
              {each && peer.via.length > 0 ? (
                <span className="type-meta text-muted-foreground">
                  {t("reach.via")}
                  {t("common.colon")}
                  <RuleLinks project={each} via={peer.via} />
                </span>
              ) : null}
            </span>
            {onAllow ? (
              <GateTip gate={gate}>
                <Button variant="outline" size="xs" onClick={() => onAllow(peer)}>
                  {t("reach.allow")}
                </Button>
              </GateTip>
            ) : null}
          </li>
        ))}
        {shown.length === 0 ? (
          <li className="py-2 text-sm text-muted-foreground">{t("reach.noMatch")}</li>
        ) : null}
      </ul>
    </div>
  )
}
