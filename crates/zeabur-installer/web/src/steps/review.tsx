import { useMemo, type ReactNode } from "react"
import CaretRightIcon from "~icons/ph/caret-right"
import DownloadIcon from "~icons/ph/download-simple"
import WarningIcon from "~icons/ph/warning"

import {
  distributionTitle,
  type CatalogComponent,
  type Plan,
  type PlannedComponent,
  type Removal,
} from "@/api"
import type { Wizard } from "@/app"
import { Code, Panel, Row, Rows } from "@/components/bits"
import { MemoryVerdict, memoryAdvice } from "@/components/memory"
import { coded, named, useT, type Translate } from "@/i18n"
import { memoryFit } from "@/lib/memory"
import { cn } from "@/lib/utils"
import { leftOutReason } from "@/steps/components"
import { StepPage } from "@/steps/frame"
import { CheckRow } from "@/steps/host"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import { Checkbox } from "@zeabur/design-system/components/checkbox"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@zeabur/design-system/components/table"

/** Up to this many components read as a sentence; more become a table grouped by why. */
const few = 7

const whys = ["preset", "enabled", "dependency"] as const

export function ReviewStep({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { config, planned, server } = wizard
  const personal = wizard.use === "personal"
  const catalog = server.catalog?.components ?? []
  const checks = personal ? server.personalChecks : server.checks[config.distribution]
  const failed = checks.filter((c) => c.status === "fail")
  const result = planned && "plan" in planned ? planned : null
  const plan = result?.plan
  const fit = plan ? memoryFit(plan, catalog) : null
  const download = useMemo(
    () => (result ? URL.createObjectURL(new Blob([result.yaml], { type: "text/yaml" })) : ""),
    [result],
  )
  const byId = new Map(catalog.map((c) => [c.id, c]))
  const privileged = plan?.components.filter((c) => byId.get(c.id)?.privileged).length ?? 0
  const leftOut = Object.entries(plan?.leftOut ?? {}).map(
    ([id, reason]) => [byId.get(id)?.displayName ?? id, leftOutReason(t, reason)] as const,
  )
  const many = (plan?.components.length ?? 0) > few
  const removals = plan?.removals ?? []
  const refused = removals.some((r) => r.usedBy.length > 0 || (r.losesData && !r.confirmed))
  const blocked = failed.length
    ? t("review.blocked")
    : fit?.verdict === "over"
      ? t("review.blockedMemory")
      : refused
        ? t("review.blockedRemoval")
        : null
  const advice = fit ? memoryAdvice(t, fit) : ""
  // The memory row already says what `memory.tight` says, and the access row the isolation.
  const repeated = ["memory.tight", "memory.personal.tight", "personal.isolation"]
  const warnings = plan?.warnings.filter((warning) => !repeated.includes(warning.code)) ?? []
  return (
    <StepPage
      title={t("review.title")}
      description={t("review.description", server.configPath)}
      back={
        <Button variant="outline" onClick={() => wizard.go("components")}>
          {t("common.back")}
        </Button>
      }
      next={
        <>
          {blocked ? <span className="text-xs text-warn-strong">{blocked}</span> : null}
          <Button disabled={!!blocked || !plan || wizard.planning} onClick={wizard.install}>
            {t("review.install")}
          </Button>
        </>
      }
    >
      {failed.length ? (
        <Panel
          title={t("review.blockedTitle", failed.length)}
          flush
          action={
            <Button variant="outline" size="sm" onClick={() => wizard.go("host")}>
              {t("review.backToChecks")}
            </Button>
          }
        >
          {failed.map((check) => (
            <CheckRow key={check.name} check={check} />
          ))}
        </Panel>
      ) : null}

      {planned && "error" in planned ? (
        <Alert variant="destructive">
          <AlertTitle>{t("common.error")}</AlertTitle>
          <AlertDescription>{planned.error}</AlertDescription>
        </Alert>
      ) : null}

      {plan && fit ? (
        <Panel title={t("review.summary")}>
          <Rows>
            {plan.use !== "personal" ? (
              <Row label={t("review.after")}>
                <AfterInstall plan={plan} catalog={catalog} />
              </Row>
            ) : null}
            <Row label={t("review.memory")}>
              <div className="flex flex-col gap-1">
                <MemoryVerdict fit={fit} />
                {advice ? <span className="text-xs text-secondary">{advice}</span> : null}
                {plan.memory.unmeasured ? (
                  <span className="text-xs text-tertiary">
                    {t("memory.unmeasured", plan.memory.unmeasured)}
                  </span>
                ) : null}
              </div>
            </Row>
            <Row label={t("review.components")}>
              <div className="flex flex-col gap-1">
                <span>
                  {personal ? t("use.personal") : t(`preset.${config.preset}`)}
                  {t("common.sep")}
                  {t("components.chosen", plan.components.length)}
                  {privileged ? (
                    <>
                      {t("common.sep")}
                      {t("review.privileged", privileged)}
                    </>
                  ) : null}
                </span>
                <span className="text-xs text-secondary">
                  {many
                    ? t("common.list", counts(t, plan))
                    : t(
                        "common.list",
                        plan.components.map((c) => c.displayName),
                      )}
                </span>
                {leftOut.length ? (
                  <span className="text-xs text-tertiary">
                    {t("review.leftOut")}
                    {t("common.colon")}
                    {t(
                      "common.list",
                      leftOut.map(([name, reason]) => t("review.leftOutItem", name, reason)),
                    )}
                  </span>
                ) : null}
              </div>
            </Row>
            <Row label={t("review.kubernetes")}>
              {plan.use === "personal"
                ? t("review.kubesolo", plan.kubesolo.version)
                : plan.cluster.pinned
                  ? t(
                      "review.cluster",
                      distributionTitle[plan.cluster.distribution],
                      plan.cluster.version,
                    )
                  : t(
                      "review.clusterCustom",
                      distributionTitle[plan.cluster.distribution],
                      plan.cluster.version,
                    )}
            </Row>
            {plan.use === "personal" ? (
              <Row label={t("review.access")}>
                <div className="flex min-w-0 flex-col gap-1">
                  <span>{t("review.personalAccess")}</span>
                  <span className="text-xs text-secondary">{t("review.personalLimits")}</span>
                </div>
              </Row>
            ) : (
              <Row label={t("review.access")}>
                <div className="flex min-w-0 flex-col gap-1">
                  <span>
                    {t(config.access.mode === "domain" ? "access.domain" : "access.local")}
                  </span>
                  <span className="font-mono text-xs break-all text-secondary">
                    {plan.zeabur?.url}
                  </span>
                  <span className="font-mono text-xs break-all text-secondary">
                    {plan.identity?.issuer}
                  </span>
                </div>
              </Row>
            )}
            {plan.use !== "personal" && plan.zeabur ? (
              <Row label={t("review.suffixes")}>
                {plan.zeabur.values?.domains?.suffixes?.length ? (
                  <span className="font-mono text-xs break-all">
                    {plan.zeabur.values.domains.suffixes.join(", ")}
                  </span>
                ) : (
                  t("review.suffixesNone")
                )}
              </Row>
            ) : null}
            <Row label={t("review.time")}>{t("review.timeValue")}</Row>
          </Rows>
        </Panel>
      ) : null}

      {removals.length ? (
        <Panel
          title={t("review.removing")}
          flush
          action={<span className="text-xs text-secondary">{removals.length}</span>}
        >
          <p className="px-5 py-3 text-xs text-secondary">{t("review.removingHint")}</p>
          {removals.map((removal) => (
            <RemovalRow
              key={removal.id}
              removal={removal}
              onConfirm={(on) => {
                const { enable, disable, inputs, allowDataLoss = [] } = wizard.active
                const others = allowDataLoss.filter((id) => id !== removal.id)
                wizard.setChoices({
                  enable,
                  disable,
                  inputs,
                  allowDataLoss: on ? [...others, removal.id] : others,
                })
              }}
            />
          ))}
        </Panel>
      ) : null}

      {warnings.length ? (
        <Alert variant="warn">
          <WarningIcon aria-hidden="true" />
          <AlertTitle>{t("review.warnings")}</AlertTitle>
          <AlertDescription>
            {warnings.map((warning) => (
              <p key={warning.code}>
                {coded(t, `warning.${warning.code}`, warning.values, warning.text)}
              </p>
            ))}
          </AlertDescription>
        </Alert>
      ) : null}

      {plan && many ? (
        <Panel
          title={t("step.components")}
          flush
          action={
            <span className="text-xs text-secondary">
              {t("components.chosen", plan.components.length)}
            </span>
          }
        >
          <ComponentTable plan={plan} byId={byId} grouped />
        </Panel>
      ) : null}

      {result && plan ? (
        <Panel
          title={t("review.details")}
          action={
            <a
              href={download}
              download="config.yaml"
              className={buttonVariants({ variant: "outline", size: "sm" })}
            >
              <DownloadIcon aria-hidden="true" />
              {t("common.download")}
            </a>
          }
        >
          <div className="flex flex-col gap-3">
            {many ? null : (
              <Disclosure label={t("review.charts")}>
                <div className="mt-3 overflow-hidden rounded-lg border border-v2-border">
                  <ComponentTable plan={plan} byId={byId} />
                </div>
              </Disclosure>
            )}
            <Disclosure label={t("review.config")}>
              <Code className="mt-3">{result.yaml.trimEnd()}</Code>
            </Disclosure>
            <Disclosure label={t("review.plan")}>
              <Code className="mt-3">{result.text.trimEnd()}</Code>
            </Disclosure>
          </div>
        </Panel>
      ) : null}
    </StepPage>
  )
}

function RemovalRow({
  removal,
  onConfirm,
}: {
  removal: Removal
  onConfirm: (on: boolean) => void
}) {
  const t = useT()
  const stays = [
    ...removal.namespaces.map((ns) => t("review.stays.namespace", ns)),
    ...removal.leftovers.map((item) =>
      item === "customresourcedefinitions"
        ? t("review.stays.crds")
        : item === "persistentvolumeclaims"
          ? t("review.stays.volumes")
          : item,
    ),
    ...removal.manifests.map((name) => t("review.stays.manifests", name)),
  ]
  const id = `remove-${removal.id}`
  return (
    <div className="flex flex-col gap-1 px-5 py-3 text-sm">
      <span className="flex flex-wrap items-baseline gap-x-2">
        <span className="font-medium text-main">{removal.displayName}</span>
        <span className="font-mono text-xs text-tertiary">{removal.helmCharts.join(", ")}</span>
      </span>
      {stays.length ? (
        <span className="text-xs text-secondary">{t("review.stays", stays)}</span>
      ) : null}
      {removal.deletedCrds.length ? (
        <span className="text-xs text-secondary">
          {t("review.deletesCrds", removal.deletedCrds)}
        </span>
      ) : null}
      {removal.usedBy.length ? (
        <span className="text-xs text-fail-strong">{t("review.inUse", removal.usedBy)}</span>
      ) : removal.losesData ? (
        <span className="mt-1 flex items-center gap-2">
          <Checkbox id={id} checked={removal.confirmed} onCheckedChange={onConfirm} />
          <label htmlFor={id} className="text-xs text-warn-strong">
            {t("review.losesData", removal.displayName)}
          </label>
        </span>
      ) : null}
    </div>
  )
}
function Disclosure({
  label,
  small,
  children,
}: {
  label: string
  small?: boolean
  children: ReactNode
}) {
  return (
    <div className={small ? "text-xs" : "text-sm"}>
      <Collapsible>
        <CollapsibleTrigger variant="disclosure">
          <CaretRightIcon aria-hidden="true" />
          {label}
        </CollapsibleTrigger>
        <CollapsibleContent>{children}</CollapsibleContent>
      </Collapsible>
    </div>
  )
}

/** The Zeabur features that need a component, with the components that give them. */
const features = [
  ["git", (id: string) => id.startsWith("builds/")],
  ["updates", (id: string) => id.startsWith("image-updates/")],
  ["postgres", (id: string) => id === "databases/cloudnative-pg"],
  ["usage", (id: string) => id === "metrics/victoria-metrics"],
  ["logs", (id: string) => id.startsWith("logs/")],
] as const

function AfterInstall({ plan, catalog }: { plan: Plan; catalog: CatalogComponent[] }) {
  const t = useT()
  const rows = features.map(([feature, provides]) => ({
    feature,
    chosen: plan.components.filter((c) => provides(c.id)).map((c) => c.displayName),
    offered: catalog.filter((c) => provides(c.id)).map((c) => c.displayName),
  }))
  const has = (feature: (typeof features)[number][0]) =>
    rows.some((row) => row.feature === feature && row.chosen.length)
  const git = rows.find((row) => row.feature === "git")
  const can = [
    t("can.image"),
    has("git") ? t("can.git") : "",
    has("postgres") ? t("can.postgres") : "",
  ].filter(Boolean)
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span>{t("review.can", can)}</span>
      {has("git") ? null : (
        <span className="text-xs text-secondary">
          {git?.offered.length ? t("review.gitNeeds", git.offered) : t("review.gitMissing")}
        </span>
      )}
      <span className="text-xs text-secondary">
        {t(has("postgres") ? "review.dbBackupsLater" : "review.backupsLater")}
      </span>
      <Disclosure label={t("review.features")} small>
        <ul className="mt-2 flex flex-col gap-1 text-xs">
          {rows.map((row) => (
            <li key={row.feature} className={row.chosen.length ? "" : "text-tertiary"}>
              <span className="text-secondary">{t(`feature.${row.feature}`)}</span>
              {t("common.colon")}
              {row.chosen.length
                ? t("common.list", row.chosen)
                : row.offered.length
                  ? t("review.notChosen")
                  : t("review.notInCatalog")}
            </li>
          ))}
          <li className="text-tertiary">
            <span className="text-secondary">{t("feature.backups")}</span>
            {t("common.colon")}
            {t("review.inZeabur")}
          </li>
        </ul>
      </Disclosure>
    </div>
  )
}

function counts(t: Translate, plan: Plan): string[] {
  const n = (why: PlannedComponent["why"]) => plan.components.filter((c) => c.why === why).length
  return [
    n("preset") ? t("review.fromPreset", n("preset")) : "",
    n("enabled") ? t("review.added", n("enabled")) : "",
    n("dependency") ? t("review.needed", n("dependency")) : "",
  ].filter(Boolean)
}

function charts(c: PlannedComponent): { chart: string; version: string }[] {
  return c.releases.map((release) =>
    release.kind === "helmChart"
      ? {
          chart: release.object.spec?.chart?.split("/").at(-1) ?? release.name,
          version: release.object.spec?.version ?? "",
        }
      : { chart: "", version: "" },
  )
}

function ComponentTable({
  plan,
  byId,
  grouped,
}: {
  plan: Plan
  byId: Map<string, CatalogComponent>
  grouped?: boolean
}) {
  const t = useT()
  const groups = grouped
    ? whys
        .map((why) => [why, plan.components.filter((c) => c.why === why)] as const)
        .filter(([, components]) => components.length)
    : [[null, plan.components] as const]
  const leftOut = Object.entries(plan.leftOut)
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{t("review.col.component")}</TableHead>
          <TableHead className="hidden sm:table-cell">{t("review.col.chart")}</TableHead>
          <TableHead className="hidden sm:table-cell">{t("review.col.version")}</TableHead>
          <TableHead className="text-right">{t("review.col.memory")}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {groups.map(([why, components]) => (
          <Group
            key={why ?? "all"}
            label={why ? `${t(`review.group.${why}`)} · ${components.length}` : null}
          >
            {components.map((c) => {
              const releases = charts(c).filter((r) => r.chart)
              const note = byId.get(c.id)?.privileged ? named(t, `note.${c.id}`, "") : ""
              const top = note ? "align-top" : undefined
              return (
                <TableRow key={c.id}>
                  <TableCell className={top}>
                    <span className="flex flex-wrap items-center gap-x-2 gap-y-1 whitespace-normal">
                      {c.displayName}
                      {byId.get(c.id)?.privileged ? (
                        <Badge variant="warn">{t("components.privileged")}</Badge>
                      ) : null}
                    </span>
                    {note ? (
                      <span className="mt-1 block text-xs whitespace-normal text-secondary">
                        {note}
                      </span>
                    ) : null}
                  </TableCell>
                  <TableCell className={cn("hidden sm:table-cell", top)}>
                    {releases.length ? (
                      <span className="font-mono text-xs">
                        {releases.map((r) => r.chart).join(", ")}
                      </span>
                    ) : (
                      t("review.manifests")
                    )}
                  </TableCell>
                  <TableCell className={cn("hidden sm:table-cell", top)}>
                    {releases.length ? (
                      <span className="font-mono text-xs">
                        {releases.map((r) => r.version).join(", ")}
                      </span>
                    ) : (
                      "—"
                    )}
                  </TableCell>
                  <TableCell className={cn("text-right", top)}>
                    <span className="tabular-nums">
                      {c.idleMemoryMib ? t("common.mib", c.idleMemoryMib) : "—"}
                    </span>
                  </TableCell>
                </TableRow>
              )
            })}
          </Group>
        ))}
        {grouped && leftOut.length ? (
          <Group label={`${t("review.group.leftOut")} · ${leftOut.length}`}>
            {leftOut.map(([id, reason]) => (
              <TableRow key={id}>
                <TableCell colSpan={4}>
                  <span className="flex flex-col gap-0.5 whitespace-normal">
                    <span className="text-secondary">{byId.get(id)?.displayName ?? id}</span>
                    <span className="text-xs text-tertiary">{leftOutReason(t, reason)}</span>
                  </span>
                </TableCell>
              </TableRow>
            ))}
          </Group>
        ) : null}
      </TableBody>
    </Table>
  )
}

function Group({ label, children }: { label: string | null; children: ReactNode }) {
  return (
    <>
      {label ? (
        <TableRow>
          <TableHead colSpan={4} scope="colgroup">
            {label}
          </TableHead>
        </TableRow>
      ) : null}
      {children}
    </>
  )
}
