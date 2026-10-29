import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import CaretRightIcon from "~icons/ph/caret-right"

import { distributionTitle, type Check, type Distribution, type Use } from "@/api"
import { fits, type Wizard } from "@/app"
import { Panel, Row, Rows, StatusDot, StatusPill, type Health } from "@/components/bits"
import { coded, useT } from "@/i18n"
import { StepPage } from "@/steps/frame"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Badge } from "@zeabur/design-system/components/badge"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"

const health: Record<string, Health> = { pass: "ok", warn: "warn", fail: "fail" }
const order: Record<Check["status"], number> = { fail: 0, warn: 1, pass: 2 }
const checkKey = (check: Check) => [check.name, check.code, ...check.values].join(" ")
const distributions: Distribution[] = ["k3s", "rke2"]

export function CheckRow({ check }: { check: Check }) {
  const t = useT()
  return (
    <div className="flex items-start gap-3 px-5 py-3">
      <StatusDot health={health[check.status] ?? "idle"} className="mt-1.5" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm font-medium text-main">
          {coded(t, `check.${check.name}`, [], check.name)}
        </span>
        <span className="text-sm text-secondary">
          {coded(t, `code.${check.code}`, check.values, check.detail)}
        </span>
      </div>
    </div>
  )
}

/** K3s or RKE2, with the one this server's memory suggests and why. */
function DistributionPanel({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { config, server, suggested } = wizard
  const gib = ((server.facts?.memoryMib ?? 0) / 1024).toFixed(1)
  const hint = (d: Distribution) =>
    !fits(server, d) ? t("dist.tooSmall") : d === "k3s" ? t("dist.k3sHint") : t("dist.rke2Hint")
  return (
    <Panel title={t("dist.title")}>
      <div className="flex flex-col gap-3">
        <p className="text-sm text-main">
          {suggested === "k3s" ? t("dist.suggestK3s", gib) : t("dist.suggestRke2", gib)}
        </p>
        <RadioGroup
          aria-label={t("dist.title")}
          value={config.distribution}
          onValueChange={(value) =>
            wizard.setConfig({ ...config, distribution: value as Distribution })
          }
        >
          {distributions.map((d) => (
            <RadioOption
              key={d}
              value={d}
              label={
                <span className="flex items-center gap-2">
                  {distributionTitle[d]}
                  {d === suggested ? (
                    <Badge variant="secondary">{t("dist.suggested")}</Badge>
                  ) : null}
                </span>
              }
              description={hint(d)}
              disabled={!fits(server, d)}
            />
          ))}
        </RadioGroup>
        {config.distribution === "rke2" && suggested === "k3s" ? (
          <p className="text-sm text-secondary">{t("dist.rke2NotSuggested")}</p>
        ) : null}
      </div>
    </Panel>
  )
}

/** A Kubernetes node name, as `node_name::is_valid` checks it. */
const nodeNamePattern = /^[a-z0-9]([-a-z0-9]*[a-z0-9])?(\.[a-z0-9]([-a-z0-9]*[a-z0-9])?)*$/

function NodeName({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const kept = Boolean(wizard.server.facts?.nodeName)
  const value = wizard.active.host.nodeName ?? ""
  return (
    <div className="flex max-w-md flex-col gap-1.5">
      <Input
        value={value}
        mono
        autoComplete="off"
        aria-label={t("host.nodeName")}
        readOnly={kept}
        aria-invalid={value !== "" && (value.length > 253 || !nodeNamePattern.test(value))}
        onChange={(event) => wizard.setNodeName(event.target.value.trim().toLowerCase())}
      />
      <span className="text-xs text-tertiary">
        {kept ? t("host.nodeNameKept") : t("host.nodeNameHint")}
      </span>
    </div>
  )
}

export function HostStep({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { server } = wizard
  const facts = server.facts
  const checks =
    wizard.use === "personal" ? server.personalChecks : server.checks[wizard.config.distribution]
  const failed = checks.filter((c) => c.status === "fail").length
  const warned = checks.filter((c) => c.status === "warn").length
  const exceptions = checks
    .filter((c) => c.status !== "pass")
    .sort((a, b) => order[a.status] - order[b.status])
  const passed = checks.filter((c) => c.status === "pass")
  return (
    <StepPage
      title={t("host.title")}
      description={t("host.description")}
      badges={
        failed ? (
          <StatusPill health="fail">{t("host.failed", failed)}</StatusPill>
        ) : warned ? (
          <StatusPill health="warn">{t("host.warned", warned)}</StatusPill>
        ) : (
          <StatusPill health="ok">{t("host.passed")}</StatusPill>
        )
      }
      next={
        <>
          <Button variant="outline" onClick={wizard.reload}>
            <ArrowClockwiseIcon aria-hidden="true" />
            {t("host.recheck")}
          </Button>
          <Button
            disabled={!server.catalog}
            onClick={() => wizard.go(wizard.use === "personal" ? "components" : "access")}
          >
            {t("common.continue")}
          </Button>
        </>
      }
    >
      <Panel title={t("use.title")}>
        <RadioGroup
          aria-label={t("use.title")}
          value={wizard.use}
          onValueChange={(value) => wizard.setUse(value as Use)}
        >
          <RadioOption
            value="personal"
            label={t("use.personal")}
            description={t("use.personalHint")}
          />
          <RadioOption
            value="team"
            label={t("use.team")}
            description={wizard.teamFits ? t("use.teamHint") : t("use.teamTooSmall")}
            disabled={!wizard.teamFits}
          />
        </RadioGroup>
      </Panel>

      {wizard.use === "team" ? <DistributionPanel wizard={wizard} /> : null}

      <Panel title={t("host.server")}>
        <Rows>
          <Row label={t("host.name")}>
            <span translate="no">{facts?.hostname ?? "—"}</span>
          </Row>
          <Row label={t("host.nodeName")}>
            <NodeName wizard={wizard} />
          </Row>
          <Row label={t("host.arch")}>
            <span className="font-mono text-xs">{facts?.arch ?? "—"}</span>
          </Row>
          <Row label={t("host.memory")}>
            {facts ? t("common.gib", (facts.memoryMib / 1024).toFixed(1)) : "—"}
          </Row>
          <Row label={t("host.addresses")}>
            <span className="flex flex-wrap gap-x-3 gap-y-1 font-mono text-xs">
              {server.addresses.length
                ? server.addresses.map((address) => <span key={address}>{address}</span>)
                : "—"}
            </span>
          </Row>
        </Rows>
      </Panel>

      <Panel
        title={t("host.checks")}
        flush
        action={
          exceptions.length ? (
            <span className="text-xs text-secondary">{t("host.passedCount", passed.length)}</span>
          ) : null
        }
      >
        {exceptions.map((check) => (
          <CheckRow key={checkKey(check)} check={check} />
        ))}
        {passed.length ? (
          <div className="px-5 py-3 text-sm">
            <Collapsible>
              <CollapsibleTrigger variant="disclosure">
                <CaretRightIcon aria-hidden="true" />
                {t("host.passedCount", passed.length)}
              </CollapsibleTrigger>
              <CollapsibleContent>
                <div className="-mx-5 mt-3 -mb-3 flex flex-col divide-y divide-v2-separator border-t border-v2-separator">
                  {passed.map((check) => (
                    <CheckRow key={checkKey(check)} check={check} />
                  ))}
                </div>
              </CollapsibleContent>
            </Collapsible>
          </div>
        ) : null}
      </Panel>

      {server.catalogError ? (
        <Alert variant="destructive">
          <AlertTitle>{t("host.catalog")}</AlertTitle>
          <AlertDescription>{server.catalogError}</AlertDescription>
        </Alert>
      ) : null}

      <p className="text-xs text-tertiary">
        {server.configExists
          ? t("host.configFrom", server.configPath)
          : t("host.configNew", server.configPath)}
      </p>
    </StepPage>
  )
}
